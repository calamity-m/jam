//! `jam daemon`: the in-memory bulletin board.
//!
//! Listens on a Unix socket, applies incoming events to the registry, and
//! fans every state change out to subscribed clients. Holds no persistent
//! state; losing the daemon loses nothing important.

pub mod registry;

use crate::proto::{self, Request, Response};
use registry::Registry;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Sessions with no event for this long are marked stale. Generous because
/// agents legitimately go quiet mid-task; hooks push, nothing polls.
/// Chosen arbitrarily for the MVP — this is the first candidate for user
/// configuration if the default proves wrong.
const STALE_TIMEOUT_SECS: u64 = 15 * 60;
const STALE_SWEEP_INTERVAL: Duration = Duration::from_secs(5);

struct Shared {
    registry: Registry,
    subscribers: Vec<UnixStream>,
}

impl Shared {
    /// Push the current session list to every subscriber, dropping any
    /// whose connection has gone away.
    fn broadcast(&mut self) {
        let response = Response::Sessions {
            sessions: self.registry.snapshot(),
        };
        let mut line = serde_json::to_string(&response).expect("response serializes");
        line.push('\n');
        self.subscribers
            .retain_mut(|stream| stream.write_all(line.as_bytes()).is_ok());
    }
}

/// Client-side connect, auto-starting a daemon if none is listening
/// ("auto-started on demand by the TUI or notify command").
pub fn connect_or_spawn() -> io::Result<UnixStream> {
    let path = proto::socket_path();
    if let Ok(stream) = UnixStream::connect(&path) {
        return Ok(stream);
    }
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(25));
        if let Ok(stream) = UnixStream::connect(&path) {
            return Ok(stream);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("daemon did not come up on {}", path.display()),
    ))
}

pub fn run() -> io::Result<()> {
    let path = proto::socket_path();
    let listener = bind(&path)?;
    eprintln!("jam daemon listening on {}", path.display());

    let shared = Arc::new(Mutex::new(Shared {
        registry: Registry::default(),
        subscribers: Vec::new(),
    }));

    // Periodic sweep so stale transitions reach subscribers without any
    // event arriving to trigger a broadcast.
    let sweeper = Arc::clone(&shared);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(STALE_SWEEP_INTERVAL);
            let mut shared = sweeper.lock().unwrap();
            if shared
                .registry
                .mark_stale(proto::now_epoch(), STALE_TIMEOUT_SECS)
            {
                shared.broadcast();
            }
        }
    });

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            let _ = handle_client(stream, &shared);
        });
    }
    Ok(())
}

/// Bind the socket, reclaiming a leftover path from a dead daemon but
/// refusing to fight a live one.
fn bind(path: &std::path::Path) -> io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match UnixListener::bind(path) {
        Ok(listener) => Ok(listener),
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            if UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!(
                        "another jam daemon is already running on {}",
                        path.display()
                    ),
                ));
            }
            std::fs::remove_file(path)?;
            UnixListener::bind(path)
        }
        Err(e) => Err(e),
    }
}

fn handle_client(stream: UnixStream, shared: &Mutex<Shared>) -> io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Request>(&line) else {
            continue; // Malformed input never takes the daemon down.
        };
        let mut shared = shared.lock().unwrap();
        match request {
            Request::Event(event) => {
                if shared.registry.apply(&event, proto::now_epoch()) {
                    shared.broadcast();
                }
            }
            Request::Snapshot => {
                let response = Response::Sessions {
                    sessions: shared.registry.snapshot(),
                };
                let mut out = stream.try_clone()?;
                writeln!(out, "{}", serde_json::to_string(&response).unwrap())?;
            }
            Request::Subscribe => {
                let mut subscriber = stream.try_clone()?;
                let response = Response::Sessions {
                    sessions: shared.registry.snapshot(),
                };
                writeln!(subscriber, "{}", serde_json::to_string(&response).unwrap())?;
                shared.subscribers.push(subscriber);
                // Keep reading: the same connection may send dismiss requests.
            }
            Request::Dismiss { session_id } => {
                if shared.registry.dismiss(&session_id) {
                    shared.broadcast();
                }
            }
            Request::MarkStale { session_id } => {
                if shared.registry.set_stale(&session_id) {
                    shared.broadcast();
                }
            }
        }
    }
    Ok(())
}

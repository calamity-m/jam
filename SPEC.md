# jam — agent overview for your multiplexer

A minimal daemon + TUI for people who already have a tmux/zellij setup they like.
Agents report their state via hooks; one pane shows you everything; Enter jumps
you to the agent that needs you. No orchestration, no dashboards, no web.

## Problem

Running several coding agents (Claude Code, Codex, OpenCode, …) across panes,
windows, and sessions means constantly cycling through them to find the one
that is blocked on input. Existing tools solve this by wrapping or replacing
the multiplexer, or by bundling cost tracking, live previews, and web UIs.
None are multiplexer-agnostic; all are heavier than the problem.

## Goals

- Answer one question instantly: **which agents need me, and take me there.**
- Work inside the user's existing tmux _or_ zellij setup, unmodified.
- Integrate with any agent that has a hooks/events mechanism, via one dumb
  notify command.
- Stay small enough that the whole system is understandable in one sitting.

## Non-goals (MVP)

- Cost/token tracking, pane content previews, session transcripts.
- Controlling agents (sending text, interrupting, approving) from the TUI.
- Persistence across daemon restarts; history/analytics.
- Web UI, remote access, multi-machine anything.
- Managing worktrees, branches, or repos.

## Tech stack

- Rust CLI application, built as one `jam` binary.
- Ratatui for the terminal UI.
- Unix socket IPC between CLI commands, daemon, and TUI clients.
- `Cargo.toml` is already seeded with initial dependencies for CLI parsing,
  terminal/TUI rendering, colors, and JSON serialization; add only what each
  deliverable actually needs.
- Keep modules shallow and explicit: no `mod.rs`, no god files.

Suggested project structure:

```text
src
├── cmd
│   ├── daemon.rs
│   ├── notify.rs
│   └── setup.rs
├── cmd.rs
├── daemon
├── daemon.rs
├── lib.rs
├── main.rs
├── notify
├── notify.rs
├── setup
│   ├── claude.rs
│   ├── codex.rs
│   ├── opencode.rs
│   └── pi.rs
├── setup.rs
├── tui
│   ├── agent_pane.rs
│   ├── event_loop.rs
│   ├── header_line.rs
│   └── status_line.rs
└── tui.rs
```

Top-level `*.rs` files own module wiring and public entry points. Matching
subdirectories hold focused implementation files for that area.

## Components

Four pieces, one binary (subcommands):

### 1. Daemon (`jam daemon`)

- Listens on a Unix socket (e.g. `$XDG_RUNTIME_DIR/jam.sock`).
- Holds an in-memory registry: `session_id → {agent, state, title, cwd,
mux, mux_session, pane_ref, last_event, last_event_at}`.
- Auto-started on demand by the TUI or notify command if not running.
- Marks sessions stale if no event arrives within a timeout, and drops
  sessions on an explicit end event.

### 2. Notify command (`jam notify`)

The entire integration surface. Agent hooks call it with an event; it writes
JSON to the socket and exits. Fast, silent, never fails the hook.

```

jam notify --session <id> --agent claude-code --event stop
jam setup <agent> --dry

```

With `--dry`, `jam setup` does not contact the daemon or write config. It
prints the generated hook snippets/commands for copy-paste setup instead.

Event payload (normalized across agents):

| Field        | Notes                                                        |
| ------------ | ------------------------------------------------------------ |
| `session_id` | Stable per agent session; from the agent's hook environment. |
| `agent`      | `pi`, `claude-code`, `codex`, `opencode`, `custom`.          |
| `event`      | `start`, `working`, `waiting_input`, `done`, `error`, `end`. |
| `title`      | Optional human label (e.g. current task or prompt snippet).  |
| `cwd`        | Working directory of the agent.                              |
| `mux`          | `tmux` / `zellij`, detected from environment.                |
| `mux_session`  | Multiplexer session holding the pane. Needed for zellij (pane ids are per-session); unused for tmux (pane ids are server-global). |
| `pane_ref`     | `$TMUX_PANE` / `$ZELLIJ_PANE_ID`, captured by the hook.      |

Agent-specific event names are mapped to the five normalized states by thin
adapter logic in `jam notify` (or by the hook configuration itself). Shipping
`jam setup` support for Pi and Claude Code hooks is part of the MVP; other
agents follow the same recipe.

### 3. TUI (`jam` with no args)

A single flat list. No tabs, no panels.

```

jam 3 agents
─────────────────────────────────────────────────────
● waiting claude ~/code/api fix auth bug
● waiting codex ~/code/web migrate router
○ working claude ~/code/jam write spec doc
─────────────────────────────────────────────────────
↵ go x dismiss q quit

```

- Sorted by attention priority: `waiting_input` / `error` first, then
  `working`, then `done`, then stale.
- Live: updates as the daemon receives events (TUI subscribes over the
  same socket).
- States distinguished by symbol + color; degrade gracefully to ASCII.

### 4. Setup command (`jam setup <agent>`)

Built-in installer for agent hook configuration. It writes or prints the hooks
needed for a supported agent to call `jam notify` with the right normalized
events.

```

jam setup pi
jam setup claude-code [--local] [--ask] [--dry]
jam setup codex [--local] [--ask] [--dry]

```

- Supports `pi`, `claude-code`, and `codex`.
- Hook payloads are plain files in the repo's `hooks/<agent>/` directories,
  embedded into the binary at build time (build.rs copies them into
  `$OUT_DIR/assets`), so the installer is self-contained.
- Installs are **non-destructive and idempotent**: JSON targets (Claude Code
  settings and Codex hooks) use the same ownership-aware merge on the `hooks`
  key — foreign entries and unrelated keys are preserved, stale jam-owned
  entries are upgraded, re-runs are no-ops, and malformed files are refused
  rather than clobbered. Pi's self-contained extension file is overwritten
  when it differs so it can stay synced with jam's embedded payload; future
  plain-file installers should opt into overwrite behavior only when jam owns
  the entire target file.
- Installs immediately by default; `--ask` prints the payload and target
  path first and requires confirmation.
- `--local` installs into the current directory's config instead of the user
  root: `./.claude/settings.local.json` instead of `~/.claude/settings.json`,
  or `./.codex/hooks.json` instead of `~/.codex/hooks.json`.
- Codex maps `SessionStart`, `UserPromptSubmit`, `PreCompact`, `PostCompact`,
  `PostToolUse`, `PermissionRequest`, and `Stop` onto the normalized subset it
  can express. Codex users must review and trust installed hooks with `/hooks`.
- Copy-paste mode is `jam setup <agent> --dry`: it prints the generated hooks instead
  of letting jam modify config.
- Does not manage agents, repos, branches, worktrees, or multiplexer layout;
  it only installs notification hooks.

## Configuration

jam reads one optional config file, `~/.config/jam/config.toml`
(`$XDG_CONFIG_HOME/jam/config.toml` when `XDG_CONFIG_HOME` is set). It has a
single `[tui]` table, and every key is off by default:

```toml
[tui]
quit_on_focus = true       # quit the TUI after Enter successfully focuses a pane
close_pane_on_quit = true  # close jam's own multiplexer pane on any TUI exit
```

The no-args TUI also accepts matching flags, `jam --quit-on-focus[=<bool>]`
and `jam --close-pane-on-quit[=<bool>]` (bare flag means `=true`). Precedence
is **flag > file > default-off**, so the file can be overridden in either
direction from the command line (e.g. `jam --quit-on-focus=false`).

- **`quit_on_focus`** — after Enter, the TUI exits only when the focus both
  succeeds *and* actually lands the viewer on the target. Focus failures keep
  it open with the error, and — because zellij has no cross-session
  `switch-client` — a row in a *different* zellij session focuses but keeps
  the TUI open with a status note rather than stranding the user with no jam
  and no landing. tmux always lands (it moves the client).
- **`close_pane_on_quit`** — on any exit (Enter-quit, `q`, `Esc`, `Ctrl-C`),
  jam best-effort closes its *own* pane (`tmux kill-pane` /
  `zellij action close-pane --pane-id`). This makes a spawned floating jam
  (`zellij run -f -- jam`) leave nothing behind without a spawn-side
  `zellij run -c` / `tmux display-popup -E`, which remain valid alternatives.
  jam only ever closes its own pane, never the target's, and only when it can
  see its multiplexer marker variable, so a stale inherited pane id can never
  cause it to close someone else's pane. Inside a `tmux display-popup` no pane
  id is exposed, so this is a safe no-op there (popups self-close anyway).

Loading rules: a missing file uses defaults; unknown keys/tables are ignored
(so an older binary tolerates a newer config); a malformed file or a
wrong-typed value is a fatal error — jam prints `jam: config <path>: <error>`
and exits without entering the TUI, so an edit typo surfaces immediately.

Tested multiplexer versions: zellij 0.44.3, tmux 3.4. `close-pane --pane-id`
support on zellij older than 0.44.3 is unverified; there, close-pane-on-quit
degrades best-effort to a lingering pane rather than an error.

## User flows

**Monitor.** Open a small pane anywhere, run `jam`. Leave it open. Glance at
it; the top row is always the most urgent agent.

**Jump.** Press Enter on a row → jam focuses that agent's pane via the
multiplexer backend (switching session/tab/window as needed) — even across
tmux sessions. The TUI stays running in its pane by default, or exits on a
successful focus when `quit_on_focus` is set (see Configuration).

**Dismiss.** Press `x` on a `done`/`error`/stale row to drop it from the list.

**Hook setup.** Run `jam setup pi` or `jam setup claude-code`. Jam shows the
hook files/commands it will change, then installs hooks that invoke
`jam notify`. Users can instead run `jam setup <agent> --dry` and copy-paste the
generated snippets manually.

## Multiplexer backends

The only multiplexer-aware code. Each backend implements exactly two
operations:

- `focus(pane_ref)` — bring the given pane into view and focus it.

Backends: `tmux` (via `tmux switch-client` / `select-window` /
`select-pane` / `split-window`), `zellij` (via
`zellij --session <name> action focus-pane-id`; no helper plugin needed as
of zellij 0.44.3 — see open questions). Everything else — daemon, events,
TUI — is multiplexer-agnostic by construction.

## Design principles

- **Hooks push; nothing polls or scrapes.** No reading pane contents, no
  process inspection. If an agent doesn't send events, it isn't tracked.
- **The daemon is a bulletin board, not a controller.** It stores the last
  known state per session and fans it out to TUI subscribers. Losing it
  loses nothing important.
- **One normalized event schema** is the contract between all parts; agents
  and multiplexers vary only at the edges.
- **Every feature request gets weighed against the non-goals list first.**

## Open questions / risks

1. **Zellij focus — RESOLVED (spiked against zellij 0.44.3).** Plain zellij
   has `zellij action focus-pane-id <id>` and it works externally, including
   switching to the tab that holds the pane:
   `zellij --session <name> action focus-pane-id terminal_<n>`. No WASM
   plugin is required. Findings: the command exits non-zero for both the
   harmless "already focused" case and a missing pane, so the backend
   distinguishes them by stderr; pane ids are only unique per session, so
   jam records `$ZELLIJ_SESSION_NAME` alongside the pane id (the event's
   `mux_session` field). Remaining
   constraint: zellij has no `tmux switch-client` equivalent, so jam cannot
   re-attach the viewer's client to a different zellij session — cross-tab
   jumps are automatic, cross-session jumps land correctly once the user
   attaches to that session.
2. **Session→pane mapping drift.** Panes can be moved or closed after the
   hook captured `pane_ref`. MVP answer: verify the pane exists at
   focus-time and mark the session stale if not.
3. **Multiple TUI instances.** Should just work (daemon fans out to all
   subscribers); confirm no assumption of a single client creeps in.

## MVP acceptance

Done when: two Claude Code sessions in different tmux sessions plus one in
zellij all appear in `jam`, states change live as they work/finish/block,
and Enter lands you in the right pane every time.

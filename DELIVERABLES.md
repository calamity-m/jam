# jam deliverables

A rough implementation order for the MVP described in `SPEC.md`. Keep each step small enough to verify before moving on, but avoid turning this into a full task tracker.

## 1. Rust project skeleton and CLI shape

Create the Rust crate, top-level command routing, and the shallow module structure from the spec. The binary should expose the intended entry points: `jam`, `jam daemon`, `jam notify`, and `jam setup <agent>`. `Cargo.toml` already has some seed dependencies for the planned CLI/TUI/serialization stack, so treat dependency additions as incremental rather than part of the initial skeleton.

**Done when:** the project builds with the seeded dependencies, command dispatch is wired, and the source tree follows the no-`mod.rs`, no-god-file structure.

## 2. Shared event model and socket protocol

Define the normalized agent event schema and the Unix socket request/response messages used by notify, daemon, and TUI clients. Keep this contract boring and stable before building behavior on top of it.

**Done when:** commands can serialize/deserialize the shared event payload, including `agent`, `event`, `cwd`, `mux`, and `pane_ref`.

## 3. Daemon registry and IPC loop

Implement `jam daemon` as the in-memory bulletin board: accept events, update the session registry, drop ended sessions, mark stale sessions, and fan state changes out to subscribers.

**Done when:** a local client can send events over the socket and another client can observe live registry updates.

## 4. Notify command and dry output

Implement `jam notify` as the dumb hook-facing command. Normal mode sends one event to the daemon and exits quietly. `jam notify --dry` prints copy-paste hook snippets/commands instead of contacting the daemon.

**Done when:** hooks have a fast, silent event path, and users have a no-write dry path for manual setup.

## 5. Multiplexer focus backends

Implement pane focusing behind a tiny backend interface. Do tmux first because it is straightforward, then spike zellij early enough to resolve whether tab-level navigation is sufficient or a helper plugin is needed.

**Done when:** given a recorded `pane_ref`, jam can focus the matching tmux pane, and the zellij MVP behavior is known and implemented or explicitly constrained.

## 6. Ratatui monitor

Build the default `jam` TUI: one flat, live-updating list sorted by attention priority, with simple keyboard actions for quit, dismiss, and focus selected agent.

**Done when:** running `jam` shows live daemon state and Enter delegates to the multiplexer focus backend.

## 7. Setup installers for supported agents

Implement `jam setup pi` and `jam setup claude-code` as safe installers for notification hooks. They should show planned changes, require confirmation before writing, and point users to `jam notify --dry` for copy-paste setup.

**Done when:** Pi and Claude Code can be configured to emit normalized jam events without hand-writing hooks.

## 8. MVP integration pass

Exercise the full user flow across multiple panes and multiplexers, then tighten only what blocks the spec's MVP acceptance.

**Done when:** two Claude Code sessions in different tmux sessions plus one zellij session appear in `jam`, update live, and Enter lands in the right pane consistently.

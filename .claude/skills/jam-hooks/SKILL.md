---
name: jam-hooks
description: Handle jam hook setup. Use when editing, adding or modifying hooks for coding agents for jam support.
---

# jam hooks

How agent hook payloads work in this repo, and how to change them safely.

## Architecture

Hook payloads are plain files under `hooks/<agent>/` (claude-code, codex,
opencode, pi). `build.rs` copies them into `$OUT_DIR/assets` and generates
`assets.rs`, exposing each agent's files as `(file name, contents)` slices
via `crate::setup::assets` (e.g. `assets::CLAUDE_CODE`, `assets::PI`).
`jam setup <agent>` installs from these embedded payloads, so the binary is
self-contained — no files are read from the repo at runtime.

Key code:

- `build.rs` — embedding; skips dotfiles (`.gitkeep` = placeholder, not payload)
- `src/cmd/setup.rs` — CLI flags: `--dry`, `--local`, `--ask`
- `src/setup/claude.rs` — JSON-merge installer for claude-code
- `src/setup/pi.rs` — verbatim file installer for pi extensions
- `src/proto.rs` — the normalized events hooks must emit and the supported
  `Agent` enum (`pi`, `claude-code`, `codex`, `opencode`, `custom`)

## Normalized events

Every hook maps its agent's native events onto:
`start`, `working`, `waiting_input`, `done`, `error`, `end` — via
`jam notify --agent <agent> --event <event>`. An agent emits only the subset
its hooks can express; never invent new event names without extending
`EventKind` in `src/proto.rs` first.

Current claude-code mapping (in `hooks/claude-code/settings-fragment.json`):
SessionStart→start, UserPromptSubmit→working, PreCompact→working (busy
compacting, not waiting; SessionStart re-fires when compaction finishes),
Notification→waiting_input, Stop→done, SessionEnd→end. `jam notify` reads
`session_id` and `cwd` from
the hook's stdin JSON, so commands need no shell plumbing.

## Install semantics (do not weaken these)

- **Non-destructive**: JSON targets are deep-merged on the `hooks` key only;
  unrelated keys and existing hook entries are preserved; malformed target
  files are refused, never clobbered. Plain-file targets are never
  overwritten when existing content differs (skip loudly instead).
- **Idempotent**: re-running `jam setup` must be a no-op.
- **Targets**: default = user root (`~/.claude/settings.json`,
  `~/.pi/agent/extensions/`); `--local` = current directory
  (`./.claude/settings.local.json`, `./.pi/extensions/` — note pi's local
  path has no `agent/` segment).
- `--ask` previews payload + target and requires confirmation; `--dry`
  prints without writing.

## Changing or adding a hook payload

1. Edit/add files under `hooks/<agent>/`. For JSON-merge agents the file is
   a settings fragment with a top-level `hooks` object; for file-install
   agents (pi) it's the literal file to place (pi discovers `*.ts` under its
   extensions dir).
2. `cargo build` — build.rs re-embeds automatically (`rerun-if-changed=hooks`).
3. Verify with `jam setup <agent> --dry` and `cargo test` (merge-semantics
   tests live in `src/setup/claude.rs`).
4. Exercise a real install only in a sandbox: use `--local` in a scratch
   directory, or override `HOME` to a temp dir for the user-root path.
   Never test against the real `~/.claude` or `~/.pi`.

## Adding a new agent

Drop payload files into the agent's `hooks/<agent>/` dir, extend
`SetupAgent` in `src/cmd/setup.rs`, and add a `src/setup/<agent>.rs`
installer — reuse the claude.rs merge path for JSON-based agents or the
pi.rs verbatim path for file-based ones. If the agent isn't in `build.rs`'s
`AGENTS` list yet, add it there too. Update the `Agent` enum in
`src/proto.rs` only if it's a genuinely new agent kind (not `custom`).

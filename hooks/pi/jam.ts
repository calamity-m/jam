import { spawn } from "node:child_process";
import { basename } from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

type JamEvent = "start" | "working" | "done" | "end";

let fallbackSessionId: string | undefined;

export default function (pi: ExtensionAPI) {
  pi.on("session_start", (_event, ctx) => {
    notify(pi, ctx, "start");
  });

  pi.on("before_agent_start", (event, ctx) => {
    notify(pi, ctx, "working", { titleFromPrompt: event.prompt });
  });

  pi.on("agent_start", (_event, ctx) => {
    notify(pi, ctx, "working");
  });

  pi.on("agent_end", (_event, ctx) => {
    notify(pi, ctx, "done");
  });

  pi.on("session_before_compact", (_event, ctx) => {
    notify(pi, ctx, "working", { title: "Compacting" });
  });

  pi.on("session_compact", (_event, ctx) => {
    notify(pi, ctx, "working", { title: "Compacted" });
  });

  pi.on("session_shutdown", (_event, ctx) => {
    notify(pi, ctx, "end");
  });
}

type NotifyDetails = {
  title?: string;
  titleFromPrompt?: string;
};

// Title fallback order:
// - prompt-backed events let jam own summarization via --title-from-prompt
// - otherwise use an explicit title, then Pi's session name, then cwd basename
function notify(
  pi: ExtensionAPI,
  ctx: ExtensionContext,
  event: JamEvent,
  details: NotifyDetails = {},
) {
  const args = [
    "notify",
    "--agent",
    "pi",
    "--event",
    event,
    "--session",
    sessionId(ctx),
    "--cwd",
    ctx.cwd,
  ];

  if (details.titleFromPrompt !== undefined) {
    args.push("--title-from-prompt");
  } else {
    const sessionName = pi.getSessionName();
    const label = details.title ?? sessionName ?? basename(ctx.cwd);
    if (label) {
      args.push("--title", label);
    }
  }

  const child = spawn("jam", args, {
    detached: true,
    // --title-from-prompt reads hook-style JSON from stdin; all other events
    // ignore stdio so jam can never block Pi's event loop.
    stdio: details.titleFromPrompt === undefined ? "ignore" : ["pipe", "ignore", "ignore"],
  });
  child.on("error", () => {
    // Hooks must never interrupt Pi if jam is unavailable.
  });
  if (details.titleFromPrompt !== undefined) {
    // Reuse jam notify's authoritative prompt summarizer instead of keeping a
    // second implementation in this extension.
    child.stdin?.on("error", () => {});
    child.stdin?.end(JSON.stringify({ prompt: details.titleFromPrompt }));
  }
  child.unref();
}

function sessionId(ctx: ExtensionContext): string {
  const sessionFile = ctx.sessionManager.getSessionFile();
  if (sessionFile) return sessionFile;

  fallbackSessionId ??= `pi:${ctx.cwd}:${process.pid}`;
  return fallbackSessionId;
}

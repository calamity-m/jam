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
    notify(pi, ctx, "working", summarizePrompt(event.prompt));
  });

  pi.on("agent_start", (_event, ctx) => {
    notify(pi, ctx, "working");
  });

  pi.on("agent_end", (_event, ctx) => {
    notify(pi, ctx, "done");
  });

  pi.on("session_before_compact", (_event, ctx) => {
    notify(pi, ctx, "working", "Compacting");
  });

  pi.on("session_compact", (_event, ctx) => {
    notify(pi, ctx, "working", "Compacted");
  });

  pi.on("session_shutdown", (_event, ctx) => {
    notify(pi, ctx, "end");
  });
}

function notify(pi: ExtensionAPI, ctx: ExtensionContext, event: JamEvent, title?: string) {
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

  const sessionName = pi.getSessionName();
  const label = title ?? sessionName ?? basename(ctx.cwd);
  if (label) {
    args.push("--title", label);
  }

  const child = spawn("jam", args, {
    detached: true,
    stdio: "ignore",
  });
  child.on("error", () => {
    // Hooks must never interrupt Pi if jam is unavailable.
  });
  child.unref();
}

function sessionId(ctx: ExtensionContext): string {
  const sessionFile = ctx.sessionManager.getSessionFile();
  if (sessionFile) return sessionFile;

  fallbackSessionId ??= `pi:${ctx.cwd}:${process.pid}`;
  return fallbackSessionId;
}

// Mirrors summarize_prompt in src/notify.rs, the authoritative behavior:
// collapse whitespace, 80-char cap (77 + "..."), undefined for blank input.
function summarizePrompt(prompt: string): string | undefined {
  const singleLine = prompt.replace(/\s+/g, " ").trim();
  if (!singleLine) return undefined;
  return singleLine.length > 80 ? `${singleLine.slice(0, 77)}...` : singleLine;
}

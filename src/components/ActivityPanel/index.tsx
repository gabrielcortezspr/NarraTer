import { memo, useMemo, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Activity, Trash2, X } from "lucide-react";
import type { ActivityEvent } from "@/lib/tauri";
import { useActivityStore, visibleEvents, type ActivityKind } from "@/stores/activity";
import { useCanvasStore } from "@/stores/canvas";
import { useTerminalsStore } from "@/stores/terminals";
import StatusDot, { STATUS_DOT } from "@/components/ui/StatusDot";

// Activity panel (Ctrl+J): the log the backend writes as things happen
// (activity.rs) — every turn state, every ask from request to answer, direct
// messages, starts and exits. Nothing here is written by an agent.
//
// docs/design.md ("Palco"): colour is state, never decoration — an ask waiting
// in the queue = --queue, a delivered ask (a line crossing) = --accent,
// working = --accent, ready = --ok, needs you = --warn, failure = --danger;
// what isn't a state (notes, messages, starts) stays neutral (--text-3).
// Type: 15 px panel title, 12 px rows, 11 px meta (the floor in the chrome);
// Inter with tabular-nums for times, JetBrains Mono only for ids.

const KINDS: { id: ActivityKind; label: string }[] = [
  { id: "all", label: "All" },
  { id: "state", label: "States" },
  { id: "ask", label: "Asks" },
  { id: "message", label: "Messages" },
];

// The backend's turn states (TurnState::as_str) in the panel's words
const STATE_WORD: Record<string, string> = { running: "working", idle: "ready", attention: "needs you" };

const ANSWER_COLOR: Record<string, string> = {
  done: "var(--ok)",
  answered: "var(--ok)",
  attention: "var(--warn)",
  timeout: "var(--danger)",
  cancelled: "var(--danger)",
  failed: "var(--danger)",
  exited: "var(--danger)",
};

function colorOf(e: ActivityEvent): string {
  switch (e.kind) {
    case "state":
      return e.status === "running" ? "var(--accent)" : e.status === "attention" ? "var(--warn)" : "var(--ok)";
    case "answer":
      return ANSWER_COLOR[e.status ?? ""] ?? "var(--text-3)";
    case "ask":
      return "var(--queue)";
    case "delivered":
      return "var(--accent)";
    case "exit":
      return "var(--danger)";
    default:
      return "var(--text-3)";
  }
}

function timeOf(ts: number): string {
  const d = new Date(ts);
  const time = d.toLocaleTimeString(undefined, { hour12: false });
  const today = new Date().toDateString() === d.toDateString();
  return today ? time : `${d.toLocaleDateString(undefined, { day: "2-digit", month: "2-digit" })} ${time}`;
}

const short = (ask?: string) => (ask ? ask.replace(/^ask-/, "") : "");

/** Folded text: first two non-empty lines, at most PREVIEW chars. Done in code —
 * WebKitGTK (Tauri on Linux) ignores CSS line-clamp on pre-wrapped text. */
const PREVIEW = 160;
function preview(text: string): string {
  const lines = text.split("\n").filter((l) => l.trim() !== "");
  let out = lines.slice(0, 2).join("\n");
  if (out.length > PREVIEW) out = out.slice(0, PREVIEW);
  return out.length < text.trim().length ? out.trimEnd() + " …" : out;
}

/** One line of the log: who, what, and the text it carries (click to unfold). */
const Row = memo(function Row({ e, onAsk, onAgent }: { e: ActivityEvent; onAsk: (id: string) => void; onAgent: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const color = colorOf(e);
  const who = (id: string, label: string) => (
    <button onClick={() => onAgent(id)} className="font-medium text-ink hover:text-accent-text transition-colors">
      {label}
    </button>
  );
  const askChip = e.ask ? (
    <button
      onClick={() => onAsk(e.ask!)}
      title="Show only this ask"
      className="font-mono text-2xs text-ink-muted hover:text-accent-text transition-colors"
    >
      #{short(e.ask)}
    </button>
  ) : null;

  let line: React.ReactNode;
  switch (e.kind) {
    case "state":
      line = <>{who(e.terminal, e.label)} <span className="text-ink-2">{STATE_WORD[e.status ?? ""] ?? e.status}</span></>;
      break;
    case "ask":
      line = <>{who(e.terminal, e.label)} <span className="text-ink-muted">asked</span> {who(e.peer ?? "", e.peer_label ?? "?")} {askChip}</>;
      break;
    case "delivered":
      line = <>{who(e.terminal, e.label)} <span className="text-ink-muted">received the ask</span> {askChip}</>;
      break;
    case "answer":
      line = (
        <>
          {who(e.terminal, e.label)} <span className="text-ink-muted">→</span> {who(e.peer ?? "", e.peer_label ?? "?")}{" "}
          <span style={{ color }}>{e.status}</span> {askChip}
        </>
      );
      break;
    case "note":
      line = <>{who(e.terminal, e.label)} {askChip}</>;
      break;
    case "message":
      line = <>{who(e.terminal, e.label)} <span className="text-ink-muted">→</span> {who(e.peer ?? "", e.peer_label ?? "?")} <span className="text-ink-muted">{e.status}</span></>;
      break;
    case "spawn":
      line = <>{who(e.terminal, e.label)} <span className="text-ink-muted">started</span></>;
      break;
    case "exit":
      line = <>{who(e.terminal, e.label)} <span style={{ color }}>exited</span> <span className="text-ink-muted">code {e.status}</span></>;
      break;
  }

  const hasText = !!e.text;
  return (
    <div className="group px-3 py-1.5 hover:bg-hover transition-colors">
      <div className="flex items-baseline gap-2 text-xs leading-snug">
        <span className="shrink-0 mt-[5px] w-1.5 h-1.5 rounded-full self-start" style={{ background: color }} />
        <span className="shrink-0 text-2xs text-ink-muted tabular-nums">{timeOf(e.ts)}</span>
        <span className="min-w-0 flex-1 truncate">{line}</span>
      </div>
      {(e.detail || hasText) && (
        <div className="pl-[22px] mt-0.5">
          {e.detail && <div className="text-2xs text-ink-muted tabular-nums">{e.detail}</div>}
          {hasText && (
            <button onClick={() => setOpen((v) => !v)} className="block w-full text-left" title={open ? "Fold" : "Unfold"}>
              <div className="text-xs text-ink-2 whitespace-pre-wrap break-words">
                {open ? e.text : preview(e.text!)}
              </div>
            </button>
          )}
        </div>
      )}
    </div>
  );
});

export default function ActivityPanel() {
  const open = useActivityStore((s) => s.open);
  const events = useActivityStore((s) => s.events);
  const terminal = useActivityStore((s) => s.terminal);
  const ask = useActivityStore((s) => s.ask);
  const kind = useActivityStore((s) => s.kind);
  const showFlips = useActivityStore((s) => s.showFlips);
  const { setOpen, setTerminal, setAsk, setKind, setShowFlips, clear } = useActivityStore.getState();
  const reduceMotion = useReducedMotion();

  const nodes = useCanvasStore((s) => s.nodes);
  const sessions = useTerminalsStore((s) => s.sessions);
  const agents = useMemo(
    () =>
      nodes
        .filter((n) => n.type === "terminal")
        .map((n) => ({ id: n.id, label: (n.data as { label?: string }).label ?? n.id })),
    [nodes]
  );
  const { shown, flips } = useMemo(
    () => visibleEvents({ events, terminal, ask, kind, showFlips }),
    [events, terminal, ask, kind, showFlips]
  );

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key="activity"
          // Panels: 180 ms on --ease (docs/design.md, "Movimento"); none with reduced motion
          initial={reduceMotion ? false : { x: 24, opacity: 0 }}
          animate={{ x: 0, opacity: 1 }}
          exit={reduceMotion ? { opacity: 0, transition: { duration: 0 } } : { x: 24, opacity: 0 }}
          transition={{ duration: 0.18, ease: [0.2, 0.8, 0.2, 1] }}
          className="absolute top-0 right-0 h-full w-[360px] z-30 flex flex-col glass border-l border-line shadow-2xl"
        >
          {/* Header */}
          <div className="flex items-center gap-2 px-3 py-2.5 border-b border-line shrink-0">
            <Activity size={15} className="text-ink-2 shrink-0" />
            <span className="text-[15px] text-ink font-semibold flex-1">Activity</span>
            <span className="text-2xs text-ink-muted tabular-nums">{shown.length}</span>
            <button
              onClick={() => {
                if (window.confirm("Clear the activity log? This deletes it from disk.")) clear().catch(console.error);
              }}
              className="text-ink-muted hover:text-danger-text transition-colors p-1 rounded hover:bg-hover"
              title="Clear the log"
            >
              <Trash2 size={12} />
            </button>
            <button
              onClick={() => setOpen(false)}
              className="text-ink-muted hover:text-ink transition-colors p-1 rounded hover:bg-hover"
              title="Close (Ctrl+J)"
            >
              <X size={13} />
            </button>
          </div>

          {/* Agents now: state light + filter */}
          <div className="px-3 py-2 border-b border-line shrink-0 flex flex-wrap gap-1.5">
            {agents.length === 0 && <span className="text-2xs text-ink-muted">No terminals on this canvas</span>}
            {agents.map((a) => {
              const status = sessions[a.id]?.status ?? "spawning";
              const active = terminal === a.id;
              return (
                <button
                  key={a.id}
                  onClick={() => setTerminal(active ? null : a.id)}
                  title={`${STATUS_DOT[status].label} — ${active ? "show all" : "show only this agent"}`}
                  className={`flex items-center gap-1.5 h-6 px-2.5 rounded-full text-2xs border transition-colors ${
                    active ? "border-accent/60 bg-accent/20 text-ink" : "border-line text-ink-2 hover:bg-hover hover:border-line-strong"
                  }`}
                >
                  <StatusDot status={status} />
                  {a.label}
                </button>
              );
            })}
          </div>

          {/* Kind filter + active ask filter */}
          <div className="px-3 py-1.5 border-b border-line shrink-0 flex items-center gap-1">
            {KINDS.map((k) => (
              <button
                key={k.id}
                onClick={() => setKind(k.id)}
                className={`h-6 px-2 rounded-md text-2xs transition-colors ${
                  kind === k.id ? "bg-hover text-ink" : "text-ink-muted hover:text-ink-2 hover:bg-hover"
                }`}
              >
                {k.label}
              </button>
            ))}
            <div className="ml-auto flex items-center gap-1">
              {(flips > 0 || showFlips) && (
                <button
                  onClick={() => setShowFlips(!showFlips)}
                  className={`h-6 px-2 rounded-md text-2xs tabular-nums transition-colors ${
                    showFlips ? "bg-hover text-ink" : "text-ink-muted hover:text-ink-2 hover:bg-hover"
                  }`}
                  title="States undone within 2 s (agents booting, redraws). The log on disk keeps them."
                >
                  {showFlips ? "Hide quick flips" : `+${flips} quick flips`}
                </button>
              )}
              {ask && (
                <button
                  onClick={() => setAsk(null)}
                  className="flex items-center gap-1 h-6 px-2 rounded-md text-2xs font-mono bg-accent/20 text-ink hover:bg-accent/30"
                  title="Show every ask again"
                >
                  #{short(ask)} <X size={10} />
                </button>
              )}
            </div>
          </div>

          {/* Log, newest first */}
          <div className="flex-1 overflow-y-auto py-1">
            {shown.length === 0 ? (
              <div className="px-4 py-8 text-center text-xs text-ink-muted">
                Nothing yet. States, asks and messages show up here as they happen.
              </div>
            ) : (
              shown.map((e) => (
                <Row
                  key={`${e.ts}-${e.kind}-${e.terminal}-${e.ask ?? ""}-${e.status ?? ""}`}
                  e={e}
                  onAsk={setAsk}
                  onAgent={(id) => id && setTerminal(id)}
                />
              ))
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

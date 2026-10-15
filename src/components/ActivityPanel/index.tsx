import { memo, useMemo, useState } from "react";
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
// Colours follow docs/design.md: colour is state. Working = accent, ready =
// ok, needs you = warn, failure = danger, asks in flight = info, the queue /
// "waiting on the transcript" note = queue yellow.

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
  exited: "var(--danger)",
};

function colorOf(e: ActivityEvent): string {
  switch (e.kind) {
    case "state":
      return e.status === "running" ? "var(--accent)" : e.status === "attention" ? "var(--warn)" : "var(--ok)";
    case "answer":
      return ANSWER_COLOR[e.status ?? ""] ?? "var(--text-3)";
    case "ask":
    case "delivered":
      return "var(--info)";
    case "note":
      return "var(--queue)";
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
      className="font-mono text-[10px] text-ink-muted hover:text-accent-text transition-colors"
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
      <div className="flex items-baseline gap-2 text-[11px] leading-snug">
        <span className="shrink-0 mt-[3px] w-1.5 h-1.5 rounded-full self-start" style={{ background: color }} />
        <span className="shrink-0 font-mono text-[10px] text-ink-muted tabular-nums">{timeOf(e.ts)}</span>
        <span className="min-w-0 flex-1 truncate">{line}</span>
      </div>
      {(e.detail || hasText) && (
        <div className="pl-[22px] mt-0.5">
          {e.detail && <div className="text-[10px] text-ink-muted">{e.detail}</div>}
          {hasText && (
            <button
              onClick={() => setOpen((v) => !v)}
              className={`w-full text-left text-[10.5px] text-ink-2 whitespace-pre-wrap break-words font-mono ${open ? "block" : "line-clamp-2"}`}
              title={open ? "Fold" : "Unfold"}
            >
              {e.text}
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
  const { setOpen, setTerminal, setAsk, setKind, clear } = useActivityStore.getState();

  const nodes = useCanvasStore((s) => s.nodes);
  const sessions = useTerminalsStore((s) => s.sessions);
  const agents = useMemo(
    () =>
      nodes
        .filter((n) => n.type === "terminal")
        .map((n) => ({ id: n.id, label: (n.data as { label?: string }).label ?? n.id })),
    [nodes]
  );
  const shown = useMemo(() => visibleEvents({ events, terminal, ask, kind }), [events, terminal, ask, kind]);

  if (!open) return null;

  return (
    <div className="absolute top-0 right-0 h-full w-[360px] z-30 flex flex-col glass border-l border-line shadow-2xl">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2.5 border-b border-line shrink-0">
        <Activity size={13} className="text-accent-text shrink-0" />
        <span className="text-xs text-ink font-medium flex-1">Activity</span>
        <span className="text-[10px] text-ink-muted tabular-nums">{shown.length}</span>
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
        {agents.length === 0 && <span className="text-[10.5px] text-ink-muted">No terminals on this canvas</span>}
        {agents.map((a) => {
          const status = sessions[a.id]?.status ?? "spawning";
          const active = terminal === a.id;
          return (
            <button
              key={a.id}
              onClick={() => setTerminal(active ? null : a.id)}
              title={`${STATUS_DOT[status].label} — ${active ? "show all" : "show only this agent"}`}
              className={`flex items-center gap-1.5 h-6 px-2 rounded-full text-[10.5px] border transition-colors ${
                active ? "border-accent/60 bg-accent/15 text-ink" : "border-line text-ink-2 hover:bg-hover"
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
            className={`h-6 px-2 rounded-md text-[10.5px] transition-colors ${
              kind === k.id ? "bg-raised text-ink" : "text-ink-muted hover:text-ink-2 hover:bg-hover"
            }`}
          >
            {k.label}
          </button>
        ))}
        {ask && (
          <button
            onClick={() => setAsk(null)}
            className="ml-auto flex items-center gap-1 h-6 px-2 rounded-md text-[10.5px] bg-info/15 text-ink-2 hover:bg-info/25"
            title="Show every ask again"
          >
            #{short(ask)} <X size={10} />
          </button>
        )}
      </div>

      {/* Log, newest first */}
      <div className="flex-1 overflow-y-auto py-1">
        {shown.length === 0 ? (
          <div className="px-4 py-8 text-center text-[11px] text-ink-muted">
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
    </div>
  );
}

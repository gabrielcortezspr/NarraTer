import type { SessionStatus } from "@/stores/terminals";

// State colours (docs/design.md): working = the narrator's accent, ready = ok,
// needs you = warn, starting = muted, exited = danger.
export const STATUS_DOT: Record<SessionStatus, { color: string; label: string }> = {
  spawning: { color: "var(--text-3)", label: "Starting…" },
  running: { color: "var(--accent)", label: "Working" },
  idle: { color: "var(--ok)", label: "Ready" },
  attention: { color: "var(--warn)", label: "Needs you — waiting for input" },
  exited: { color: "var(--danger)", label: "Exited" },
};

/** Small state light; breathes while the agent works, rings when it needs you. */
export default function StatusDot({ status }: { status: SessionStatus }) {
  const { color, label } = STATUS_DOT[status];
  return (
    <span title={label} className="relative shrink-0 flex items-center justify-center w-2.5 h-2.5">
      {status === "attention" && (
        <span className="absolute inset-0 rounded-full animate-ping" style={{ background: color, opacity: 0.35 }} />
      )}
      <span
        className={`relative rounded-full ${status === "running" ? "narrater-breathe" : ""}`}
        style={{ width: 7, height: 7, background: color, boxShadow: status === "running" ? `0 0 8px ${color}` : "none" }}
      />
    </span>
  );
}

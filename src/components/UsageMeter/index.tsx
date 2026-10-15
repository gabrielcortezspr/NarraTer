import type { UsageWindow } from "@/lib/tauri";
import { formatReset, usageColor } from "@/stores/usage";

// Usage meters of the "Palco" profile (docs/design.md, "Uso dos agentes"):
// quiet below 60%, yellow below 80%, red above; numbers in tabular figures.

/** Small ring for a session's context window, shown in an agent tile header. */
export function ContextRing({ pct, title }: { pct: number; title: string }) {
  const r = 5.5;
  const c = 2 * Math.PI * r;
  const clamped = Math.max(0, Math.min(100, pct));
  return (
    <span title={title} className="shrink-0 flex items-center gap-1 text-2xs tabular-nums text-ink-muted select-none">
      <svg width={14} height={14} viewBox="0 0 14 14" className="-rotate-90">
        <circle cx={7} cy={7} r={r} fill="none" stroke="var(--line-strong)" strokeWidth={2} />
        <circle
          cx={7}
          cy={7}
          r={r}
          fill="none"
          stroke={clamped < 60 ? "var(--text-2)" : usageColor(clamped)}
          strokeWidth={2}
          strokeLinecap="round"
          strokeDasharray={`${(clamped / 100) * c} ${c}`}
          style={{ transition: "stroke-dasharray 400ms var(--ease)" }}
        />
      </svg>
      {Math.round(clamped)}%
    </span>
  );
}

/** One plan window (5 h or 7 days): label, thin bar, percentage, reset time. */
export function UsageBar({ label, window }: { label: string; window: UsageWindow }) {
  const pct = Math.max(0, Math.min(100, window.used_percent));
  const color = usageColor(pct);
  const reset = formatReset(window.resets_at);
  return (
    <div className="flex flex-col gap-1" title={reset ? `${label}: ${pct.toFixed(1)}% used · resets ${reset}` : undefined}>
      <div className="flex items-center justify-between text-2xs">
        <span className="text-ink-2">{label}</span>
        <span className="tabular-nums text-ink-muted">
          <span className="text-ink font-medium">{Math.round(pct)}%</span>
          {reset && <span> · {reset}</span>}
        </span>
      </div>
      <div className="h-1 rounded-full bg-hover overflow-hidden">
        <div
          className="h-full rounded-full"
          style={{
            width: `${pct}%`,
            background: pct < 60 ? "var(--text-2)" : color,
            transition: "width 400ms var(--ease), background 200ms",
          }}
        />
      </div>
    </div>
  );
}

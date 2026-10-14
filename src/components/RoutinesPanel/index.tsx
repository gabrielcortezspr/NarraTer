import { useState } from "react";
import { Play, Pause, Trash2, Zap, Bell } from "lucide-react";
import { useRoutinesStore } from "@/stores/routines";
import { useCanvasStore } from "@/stores/canvas";
import { routineDelete, routineRun, routineSetEnabled, type RoutineView } from "@/lib/tauri";
import { toast } from "@/stores/toasts";

// Sidebar list of the scene's routines (the backend scheduler owns them;
// agents in Maestro mode create them with `narrater routine create`).
const STATUS_COLOR: Record<RoutineView["status"], string> = {
  enabled: "var(--ok)",
  paused: "var(--text-3)",
  finished: "var(--text-4)",
};

export default function RoutinesPanel() {
  const routines = useRoutinesStore((s) => s.routines);
  const nodes = useCanvasStore((s) => s.nodes);
  const [confirming, setConfirming] = useState<string | null>(null);

  // Show the terminal's current name (it may have been renamed on the canvas)
  const targetName = (r: RoutineView) => {
    if (!r.target_id) return "reminder";
    const node = nodes.find((n) => n.id === r.target_id);
    return (node?.data as { label?: string } | undefined)?.label ?? `${r.target_label} (gone)`;
  };

  const act = (fn: () => Promise<unknown>) => fn().catch((e) => toast.error(String(e)));

  return (
    <div className="px-3 pt-4 pb-1">
      <div className="flex items-center justify-between px-1">
        <span className="text-2xs text-ink-muted font-semibold uppercase tracking-[0.06em]">
          Routines
        </span>
        <span className="text-2xs tabular-nums text-ink-muted">{routines.length}</span>
      </div>

      {routines.length === 0 ? (
        <p className="text-2xs text-ink-muted mt-1.5 px-1 leading-snug">
          None yet — an agent in Maestro mode schedules them with{" "}
          <code className="text-ink-muted">narrater routine create</code>.
        </p>
      ) : (
        <div className="mt-1.5 space-y-1 max-h-48 overflow-y-auto">
          {routines.map((r) => (
            <div
              key={r.id}
              className="group rounded-md px-2 py-1.5 bg-surface border border-line hover:border-line-strong"
              title={`${r.command}${r.last_result ? `\n\nLast: ${r.last_result}` : ""}`}
            >
              <div className="flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: STATUS_COLOR[r.status] }} />
                <span className={`text-[10px] truncate flex-1 ${r.status === "enabled" ? "text-ink" : "text-ink-muted"}`}>
                  {r.name}
                </span>
                <div className="hidden group-hover:flex items-center gap-0.5 shrink-0">
                  <button onClick={() => act(() => routineRun(r.id))} title="Fire now" className="p-0.5 text-ink-muted hover:text-queue">
                    <Zap size={10} />
                  </button>
                  {r.status === "enabled" ? (
                    <button onClick={() => act(() => routineSetEnabled(r.id, false))} title="Pause" className="p-0.5 text-ink-muted hover:text-ink">
                      <Pause size={10} />
                    </button>
                  ) : (
                    <button onClick={() => act(() => routineSetEnabled(r.id, true))} title="Resume" className="p-0.5 text-ink-muted hover:text-ok">
                      <Play size={10} />
                    </button>
                  )}
                  {confirming === r.id ? (
                    <button
                      onClick={() => { setConfirming(null); act(() => routineDelete(r.id)); }}
                      onMouseLeave={() => setConfirming(null)}
                      title="Click again to delete"
                      className="px-1 text-[9px] text-danger-text hover:text-ink"
                    >
                      delete?
                    </button>
                  ) : (
                    <button onClick={() => setConfirming(r.id)} title="Delete" className="p-0.5 text-ink-muted hover:text-danger-text">
                      <Trash2 size={10} />
                    </button>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-1 mt-0.5 text-[9px] text-ink-faint">
                {r.target_id ? null : <Bell size={8} />}
                <span className="truncate">
                  {r.schedule} → {targetName(r)}
                  {r.next_fire && r.status === "enabled" ? ` · next ${r.next_fire.slice(5, 16)}` : ""}
                  {r.status === "finished" ? " · finished" : ""}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

import { useState } from "react";
import { Clock, Play, Pause, Trash2, Zap, Bell } from "lucide-react";
import { useRoutinesStore } from "@/stores/routines";
import { useCanvasStore } from "@/stores/canvas";
import { routineDelete, routineRun, routineSetEnabled, type RoutineView } from "@/lib/tauri";
import { toast } from "@/stores/toasts";

// Sidebar list of the scene's routines (the backend scheduler owns them;
// agents in Maestro mode create them with `narrater routine create`).
const STATUS_COLOR: Record<RoutineView["status"], string> = {
  enabled: "#4ade80",
  paused: "#6b7280",
  finished: "#3a3a3a",
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
    <div className="px-3 pt-3 pb-1 border-t border-[#1a1a1a]">
      <div className="flex items-center justify-between">
        <span className="text-[10px] text-[#555] font-medium uppercase tracking-wider flex items-center gap-1">
          <Clock size={10} /> Routines
        </span>
        <span className="text-[10px] text-[#333]">{routines.length}</span>
      </div>

      {routines.length === 0 ? (
        <p className="text-[9px] text-[#3a3a3a] mt-1.5 leading-snug">
          None yet — an agent in Maestro mode schedules them with{" "}
          <code className="text-[#555]">narrater routine create</code>.
        </p>
      ) : (
        <div className="mt-1.5 space-y-1 max-h-48 overflow-y-auto">
          {routines.map((r) => (
            <div
              key={r.id}
              className="group rounded-md px-2 py-1.5 bg-[#141414] border border-[#1f1f1f] hover:border-[#2a2a2a]"
              title={`${r.command}${r.last_result ? `\n\nLast: ${r.last_result}` : ""}`}
            >
              <div className="flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: STATUS_COLOR[r.status] }} />
                <span className={`text-[10px] truncate flex-1 ${r.status === "enabled" ? "text-[#ccc]" : "text-[#555]"}`}>
                  {r.name}
                </span>
                <div className="hidden group-hover:flex items-center gap-0.5 shrink-0">
                  <button onClick={() => act(() => routineRun(r.id))} title="Fire now" className="p-0.5 text-[#555] hover:text-[#fbbf24]">
                    <Zap size={10} />
                  </button>
                  {r.status === "enabled" ? (
                    <button onClick={() => act(() => routineSetEnabled(r.id, false))} title="Pause" className="p-0.5 text-[#555] hover:text-white">
                      <Pause size={10} />
                    </button>
                  ) : (
                    <button onClick={() => act(() => routineSetEnabled(r.id, true))} title="Resume" className="p-0.5 text-[#555] hover:text-[#4ade80]">
                      <Play size={10} />
                    </button>
                  )}
                  {confirming === r.id ? (
                    <button
                      onClick={() => { setConfirming(null); act(() => routineDelete(r.id)); }}
                      onMouseLeave={() => setConfirming(null)}
                      title="Click again to delete"
                      className="px-1 text-[9px] text-[#f87171] hover:text-white"
                    >
                      delete?
                    </button>
                  ) : (
                    <button onClick={() => setConfirming(r.id)} title="Delete" className="p-0.5 text-[#555] hover:text-[#f87171]">
                      <Trash2 size={10} />
                    </button>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-1 mt-0.5 text-[9px] text-[#444]">
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

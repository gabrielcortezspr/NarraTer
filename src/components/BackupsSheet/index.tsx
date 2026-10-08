import { useEffect, useState } from "react";
import { History, X } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import { saveNow } from "@/hooks/useAutoSave";
import { toast } from "@/stores/toasts";
import { sceneBackups, sceneRestoreBackup, type BackupInfo } from "@/lib/tauri";

// A workspace's backups: earlier versions of its canvas, kept as it's saved
// (at most one every 10 minutes, the last 30). Restoring one keeps what's
// there now as a backup too, so nothing is lost either way.

function ago(at: number): string {
  const mins = Math.round((Date.now() - at) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(at).toLocaleString();
}

export default function BackupsSheet({ scene, onClose }: { scene: string; onClose: () => void }) {
  const [list, setList] = useState<BackupInfo[] | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);

  useEffect(() => {
    sceneBackups(scene).then(setList).catch(() => setList([]));
  }, [scene]);

  const restore = async (b: BackupInfo) => {
    try {
      // Whatever is unsaved goes to disk first (it becomes a backup too)
      await saveNow();
      await sceneRestoreBackup(scene, b.file);
      await useCanvasStore.getState().reloadScene(scene);
      toast.success(`"${scene}" restored to ${ago(b.at)} — the version you had is now a backup`);
      onClose();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50" onMouseDown={onClose}>
      <div
        className="w-[400px] max-w-[92vw] rounded-xl bg-[#141414] border border-[#2a2a2a] shadow-2xl p-4 flex flex-col gap-3"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-sm text-white"><History size={14} /> Backups of “{scene}”</div>
          <button onClick={onClose} className="text-[#555] hover:text-white"><X size={14} /></button>
        </div>
        <div className="text-[10px] text-[#555]">
          Saved as you work — at most one every 10 minutes, the last 30 kept.
        </div>
        <div className="max-h-[50vh] overflow-y-auto flex flex-col gap-1">
          {list === null && <div className="text-xs text-[#555]">loading…</div>}
          {list?.length === 0 && <div className="text-xs text-[#555]">No backups yet — they appear as the workspace is saved over time.</div>}
          {list?.map((b) => (
            <div key={b.file} className="flex items-center justify-between gap-2 px-2 py-1.5 rounded-md hover:bg-[#1c1c1c]">
              <div className="flex flex-col">
                <span className="text-xs text-[#ddd]">{ago(b.at)}</span>
                <span className="text-[10px] text-[#666]">
                  {b.nodes} nodes · {b.terminals} terminals{b.floors ? ` · ${b.floors} floors` : ""}
                </span>
              </div>
              {confirm === b.file ? (
                <button onClick={() => restore(b)} className="text-[11px] px-2 py-0.5 rounded bg-[#8b5cf6] text-white">
                  Restore this?
                </button>
              ) : (
                <button onClick={() => setConfirm(b.file)} className="text-[11px] text-[#c4b5fd] hover:text-white">
                  Restore
                </button>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

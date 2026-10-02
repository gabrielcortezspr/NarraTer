import { useEffect, useRef, useState } from "react";
import { GitBranch, Layers, Plus, Check, X } from "lucide-react";
import { GROUND, floorOf, useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { createFloor, deleteFloor, floorIsDirty } from "@/lib/floors";
import { toast } from "@/stores/toasts";

// Scene name + directory, and the floors of the scene: switch, create (a git
// worktree on its own branch when the scene's directory is a repository),
// copy a floor's path, delete one (UI only, like Maestri).
interface Menu {
  id: string;
  x: number;
  y: number;
  /** delete → confirm; dirty → confirm discarding uncommitted changes */
  stage: "menu" | "confirm" | "dirty";
}

function shortPath(p: string): string {
  const parts = p.split("/").filter(Boolean);
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : p;
}

export default function FloorBar() {
  const current = useWorkspacesStore((s) => s.current);
  const sceneDir = useWorkspacesStore((s) => s.layout.scenes[s.current]?.dir ?? null);
  const floors = useCanvasStore((s) => s.floors);
  const currentFloor = useCanvasStore((s) => s.currentFloor);
  const setFloor = useCanvasStore((s) => s.setFloor);
  const counts = useCanvasStore((s) => {
    const c: Record<string, number> = {};
    s.nodes.forEach((n) => { c[floorOf(n)] = (c[floorOf(n)] ?? 0) + 1; });
    return JSON.stringify(c);
  });
  const nodeCount = JSON.parse(counts) as Record<string, number>;

  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [copyGround, setCopyGround] = useState(false);
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<Menu | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (creating) inputRef.current?.focus();
  }, [creating]);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [menu]);

  const submit = async () => {
    if (!name.trim()) { setCreating(false); return; }
    setBusy(true);
    try {
      const summary = await createFloor({ name, copyGround });
      toast.success(summary);
      const created = useCanvasStore.getState().floors.find((f) => f.name === name.trim());
      if (created) setFloor(created.id);
      setCreating(false);
      setName("");
      setCopyGround(false);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string, force: boolean) => {
    setMenu(null);
    try {
      await deleteFloor(id, force);
      toast.success("Floor deleted — its branch and commits stay in the repository");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const chip = (id: string, label: string, branch?: string | null) => {
    const active = currentFloor === id;
    return (
      <button
        key={id}
        onClick={() => setFloor(id)}
        onContextMenu={(e) => {
          if (id === GROUND) return;
          e.preventDefault();
          setMenu({ id, x: e.clientX, y: e.clientY, stage: "menu" });
        }}
        title={branch ? `branch ${branch} · right-click for more` : id === GROUND ? "Ground floor" : "Right-click for more"}
        className={`flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] border transition-colors shrink-0 whitespace-nowrap ${
          active ? "border-[#8b5cf6] text-[#c4b5fd] bg-[#8b5cf614]" : "border-[#222] text-[#555] hover:text-[#999] hover:border-[#333]"
        }`}
      >
        {branch && <GitBranch size={9} />}
        {label}
        {(nodeCount[id] ?? 0) > 0 && <span className="text-[#444]">{nodeCount[id]}</span>}
      </button>
    );
  };

  const menuFloor = menu ? floors.find((f) => f.id === menu.id) : undefined;

  return (
    // Stops short of the centered toolbar (~420px wide); chips scroll sideways
    <div className="absolute top-0 left-0 h-10 flex items-center gap-2 px-4 z-10" style={{ maxWidth: "calc(50% - 220px)" }}>
      <span className="text-[#3a3a3a] text-xs select-none truncate min-w-[40px] max-w-[160px]" title={sceneDir ?? "No working directory — set one from the scene's menu in the sidebar"}>
        {current}
        {sceneDir && <span className="text-[#2a2a2a]"> · {shortPath(sceneDir)}</span>}
      </span>

      {(floors.length > 0 || creating) && <Layers size={11} className="text-[#333] shrink-0" />}
      {floors.length > 0 && (
        <div className="flex items-center gap-1 overflow-x-auto min-w-0 [scrollbar-width:none]">
          {chip(GROUND, "Ground")}
          {floors.map((f) => chip(f.id, f.name, f.branch))}
        </div>
      )}

      {creating ? (
        <div className="flex items-center gap-1 shrink-0">
          <input
            ref={inputRef}
            value={name}
            disabled={busy}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
              if (e.key === "Escape") { setCreating(false); setName(""); }
            }}
            placeholder="floor name…"
            className="w-28 bg-[#1a1a1a] border border-[#333] text-white text-[10px] rounded px-2 py-0.5 focus:outline-none focus:border-[#8b5cf6]"
          />
          <label className="flex items-center gap-1 text-[9px] text-[#555] select-none" title="Start the floor with a copy of the ground floor's layout">
            <input type="checkbox" checked={copyGround} onChange={(e) => setCopyGround(e.target.checked)} className="accent-[#8b5cf6]" />
            copy ground
          </label>
          <button onClick={submit} disabled={busy} className="text-[#4ade80] hover:text-white"><Check size={11} /></button>
          <button onClick={() => { setCreating(false); setName(""); }} className="text-[#555] hover:text-white"><X size={11} /></button>
        </div>
      ) : (
        <button
          onClick={() => setCreating(true)}
          title={sceneDir ? "New floor — a git worktree on its own branch when the directory is a repository" : "New floor"}
          className="flex items-center gap-0.5 text-[10px] text-[#3a3a3a] hover:text-[#8b5cf6] shrink-0"
        >
          <Plus size={11} /> {floors.length === 0 && "floor"}
        </button>
      )}

      {menu && menuFloor && (
        <div
          className="fixed z-50 bg-[#1a1a1a] border border-[#2a2a2a] rounded-lg shadow-xl py-1 w-[230px]"
          style={{ top: menu.y, left: menu.x }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {menu.stage === "menu" && (
            <>
              <div className="px-3 py-1 text-[9px] text-[#555] break-all">
                {menuFloor.branch ? `branch ${menuFloor.branch} · ${menuFloor.dir}` : "plain floor (shares the scene's directory)"}
              </div>
              {menuFloor.dir && (
                <button
                  onClick={() => { navigator.clipboard.writeText(menuFloor.dir!).catch(() => {}); setMenu(null); }}
                  className="w-full text-left px-3 py-1.5 text-xs text-[#ccc] hover:bg-[#2a2a2a]"
                >
                  Copy path
                </button>
              )}
              <button
                onClick={async () => {
                  const dirty = await floorIsDirty(menu.id).catch(() => false);
                  setMenu({ ...menu, stage: dirty ? "dirty" : "confirm" });
                }}
                className="w-full text-left px-3 py-1.5 text-xs text-[#f87171] hover:bg-[#2a2a2a]"
              >
                Delete floor…
              </button>
            </>
          )}
          {menu.stage === "confirm" && (
            <button onClick={() => remove(menu.id, false)} className="w-full text-left px-3 py-1.5 text-xs text-[#f87171] hover:bg-[#2a2a2a]">
              Delete '{menuFloor.name}' and everything on it?{menuFloor.branch ? " (the branch stays)" : ""}
            </button>
          )}
          {menu.stage === "dirty" && (
            <button onClick={() => remove(menu.id, true)} className="w-full text-left px-3 py-1.5 text-xs text-[#f87171] hover:bg-[#2a2a2a]">
              Its worktree has uncommitted changes — discard them and delete?
            </button>
          )}
        </div>
      )}
    </div>
  );
}

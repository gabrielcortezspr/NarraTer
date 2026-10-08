import { useEffect, useRef, useState } from "react";
import { ExternalLink, GitBranch, GitMerge, GitPullRequest, Layers, Plus, Check, X } from "lucide-react";
import { GROUND, floorOf, useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { createFloor, deleteFloor, describeStatus, floorIsDirty, landFloor } from "@/lib/floors";
import {
  floorBranches, floorPrMerge, floorPrStatus, openInEditor, type FloorState, type FloorStatusLine,
} from "@/lib/tauri";
import { toast } from "@/stores/toasts";

// Scene name + directory, and the floors of the scene: switch, create (a git
// worktree on its own branch when the scene's directory is a repository),
// copy a floor's path, land its branch, delete it. Each chip shows the
// status line its agents post (`narrater floor status`).
interface Menu {
  id: string;
  x: number;
  y: number;
  /** delete → confirm; dirty → confirm discarding uncommitted changes; land → pick a target; merge → confirm the PR merge */
  stage: "menu" | "confirm" | "dirty" | "land" | "merge";
}

/** "#42", "42" or a pull/merge request link typed as a new floor's name. */
const PR_REF = /^#\d+$|^https?:\/\/\S+\/(pull|merge_requests)\/\d+/;

const STATE_COLOR: Record<FloorState, string> = {
  working: "#60a5fa",
  blocked: "#f87171",
  review: "#fbbf24",
  done: "#4ade80",
};

/** Land target: "" = keep the branch only, otherwise merge into it. */
interface LandForm {
  branches: string[];
  into: string;
  deleteBranch: boolean;
}

function shortPath(p: string): string {
  const parts = p.split("/").filter(Boolean);
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : p;
}

export default function FloorBar() {
  const current = useWorkspacesStore((s) => s.current);
  const sceneDir = useWorkspacesStore((s) => s.layout.scenes[s.current]?.dir ?? null);
  const floors = useCanvasStore((s) => s.floors);
  const groundStatus = useCanvasStore((s) => s.groundStatus);
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
  const [land, setLand] = useState<LandForm | null>(null);
  /** Pull request status of the floor whose menu is open (null: loading) */
  const [prStatus, setPrStatus] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (creating) inputRef.current?.focus();
  }, [creating]);

  const menuId = menu?.id;
  useEffect(() => {
    setPrStatus(null);
    const floor = floors.find((f) => f.id === menuId);
    if (!floor?.pull_request || !floor.worktree) return;
    let live = true;
    floorPrStatus(floor.worktree, floor.pull_request)
      .then((s) => live && setPrStatus(s))
      .catch((e) => live && setPrStatus(`status unavailable — ${e}`));
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [menuId]);

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
      const ref = name.trim();
      const { name: created_, summary } = await createFloor(
        PR_REF.test(ref) ? { name: "", pullRequest: ref, copyGround } : { name, copyGround },
      );
      toast.success(summary);
      const created = useCanvasStore.getState().floors.find((f) => f.name === created_);
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

  const openLand = async () => {
    if (!menu) return;
    const floor = floors.find((f) => f.id === menu.id);
    const branches = floor?.worktree ? await floorBranches(floor.worktree, sceneDir).catch(() => []) : [];
    const targets = branches.filter((b) => b !== floor?.branch);
    setLand({ branches: targets, into: targets[0] ?? "", deleteBranch: false });
    setMenu({ ...menu, stage: "land" });
  };

  const doMerge = async (id: string) => {
    const floor = floors.find((f) => f.id === id);
    setMenu(null);
    if (!floor?.pull_request || !floor.worktree) return;
    setBusy(true);
    try {
      toast.success(await floorPrMerge(floor.worktree, floor.pull_request));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doLand = async (id: string) => {
    if (!land) return;
    setMenu(null);
    setBusy(true);
    try {
      const summary = await landFloor(id, { into: land.into || undefined, deleteBranch: land.into ? land.deleteBranch : false });
      toast.success(`Landed — ${summary}`);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
      setLand(null);
    }
  };

  const chip = (id: string, label: string, branch?: string | null, status?: FloorStatusLine | null) => {
    const active = currentFloor === id;
    const tip = [
      branch ? `branch ${branch}` : id === GROUND ? "Ground floor" : null,
      status ? describeStatus(status) : null,
      id === GROUND ? null : "right-click for more",
    ].filter(Boolean).join(" · ");
    return (
      <button
        key={id}
        onClick={() => setFloor(id)}
        onContextMenu={(e) => {
          if (id === GROUND) return;
          e.preventDefault();
          setMenu({ id, x: e.clientX, y: e.clientY, stage: "menu" });
        }}
        title={tip}
        className={`flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] border transition-colors shrink-0 whitespace-nowrap ${
          active ? "border-[#8b5cf6] text-[#c4b5fd] bg-[#8b5cf614]" : "border-[#222] text-[#555] hover:text-[#999] hover:border-[#333]"
        }`}
      >
        {status && <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: STATE_COLOR[status.state] }} />}
        {branch && (floors.find((f) => f.id === id)?.pull_request ? <GitPullRequest size={9} /> : <GitBranch size={9} />)}
        {label}
        {status?.progress && <span className="text-[#666]">{status.progress}</span>}
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
          {chip(GROUND, "Ground", null, groundStatus)}
          {floors.map((f) => chip(f.id, f.name, f.branch, f.status))}
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
            placeholder="name, or #PR / PR link…"
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
          className="fixed z-50 bg-[#1a1a1a] border border-[#2a2a2a] rounded-lg shadow-xl py-1 w-[250px]"
          style={{ top: menu.y, left: menu.x }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {menu.stage === "menu" && (
            <>
              <div className="px-3 py-1 text-[9px] text-[#555] break-all">
                {menuFloor.branch ? `branch ${menuFloor.branch} · ${menuFloor.dir}` : "plain floor (shares the scene's directory)"}
              </div>
              {menuFloor.pull_request && (
                <div className="px-3 pb-1 flex flex-col gap-1 text-[10px]">
                  <div className="text-[#ccc] break-words">
                    #{menuFloor.pull_request.number} {menuFloor.pull_request.title}
                    {!menuFloor.pull_request.same_repo && <span className="text-[#777]"> · from a fork (commits here can't reach it)</span>}
                  </div>
                  <div className="text-[#777] break-words">{prStatus ?? "checking the pull request…"}</div>
                  <div className="flex gap-2">
                    <button
                      onClick={() => { openInEditor("xdg-open", menuFloor.pull_request!.url).catch((e) => toast.error(String(e))); setMenu(null); }}
                      className="flex items-center gap-1 text-[#93c5fd] hover:text-white"
                    >
                      <ExternalLink size={10} /> Open
                    </button>
                    <button
                      onClick={() => setMenu({ ...menu, stage: "merge" })}
                      disabled={busy}
                      className="flex items-center gap-1 text-[#c4b5fd] hover:text-white"
                    >
                      <GitMerge size={10} /> Merge…
                    </button>
                  </div>
                </div>
              )}
              {menuFloor.status && (
                <div className="px-3 pb-1 text-[10px] break-words" style={{ color: STATE_COLOR[menuFloor.status.state] }}>
                  {describeStatus(menuFloor.status)}
                </div>
              )}
              {menuFloor.branch && (
                <button
                  onClick={openLand}
                  disabled={busy}
                  className="w-full flex items-center gap-1.5 text-left px-3 py-1.5 text-xs text-[#ccc] hover:bg-[#2a2a2a]"
                >
                  <GitMerge size={11} /> Land…
                </button>
              )}
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
          {menu.stage === "merge" && menuFloor.pull_request && (
            <button onClick={() => doMerge(menu.id)} className="w-full text-left px-3 py-1.5 text-xs text-[#c4b5fd] hover:bg-[#2a2a2a]">
              Merge pull request #{menuFloor.pull_request.number} on {menuFloor.pull_request.provider === "gitlab" ? "GitLab" : "GitHub"}?
            </button>
          )}
          {menu.stage === "land" && land && (
            <div className="px-3 py-2 flex flex-col gap-2 text-[10px] text-[#999]">
              <label className="flex flex-col gap-1">
                Bring '{menuFloor.branch}' into
                <select
                  value={land.into}
                  onChange={(e) => setLand({ ...land, into: e.target.value })}
                  className="bg-[#111] border border-[#333] rounded px-1 py-0.5 text-[#ddd] focus:outline-none focus:border-[#8b5cf6]"
                >
                  {land.branches.map((b) => <option key={b} value={b}>{b}</option>)}
                  <option value="">nothing — keep the branch only</option>
                </select>
              </label>
              {land.into && (
                <label className="flex items-center gap-1 select-none">
                  <input
                    type="checkbox"
                    checked={land.deleteBranch}
                    onChange={(e) => setLand({ ...land, deleteBranch: e.target.checked })}
                    className="accent-[#8b5cf6]"
                  />
                  delete '{menuFloor.branch}' after merging
                </label>
              )}
              <div className="text-[9px] text-[#555]">
                The floor and its worktree go; uncommitted work or a conflict stops it with nothing changed.
              </div>
              <button
                onClick={() => doLand(menu.id)}
                className="self-start flex items-center gap-1 px-2 py-0.5 rounded border border-[#8b5cf6] text-[#c4b5fd] hover:bg-[#8b5cf61a]"
              >
                <GitMerge size={10} /> Land '{menuFloor.name}'
              </button>
            </div>
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

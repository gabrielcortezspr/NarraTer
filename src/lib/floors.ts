import { GROUND, useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { floorPrepare, floorRemove, floorStatus, type FloorMeta } from "@/lib/tauri";

// Floors of the open scene: creation (git worktree when possible) and
// removal, shared by the floor bar and `narrater floor …`.

export interface CreateFloorOpts {
  name: string;
  branch?: string;
  existingBranch?: boolean;
  noGit?: boolean;
  copyGround?: boolean;
}

/** Working directory of a floor: its worktree, else the scene's directory. */
export function floorDir(floorId: string): string | null {
  const scene = useWorkspacesStore.getState();
  const sceneDir = scene.dirOf(scene.current);
  if (floorId === GROUND) return sceneDir;
  return useCanvasStore.getState().floors.find((f) => f.id === floorId)?.dir ?? sceneDir;
}

export function findFloor(name: string): FloorMeta | "ground" | undefined {
  const wanted = name.trim().toLowerCase();
  if (wanted === "ground") return "ground";
  return useCanvasStore.getState().floors.find((f) => f.name.toLowerCase() === wanted);
}

/** Creates a floor on the open scene; returns a one-line summary. */
export async function createFloor(opts: CreateFloorOpts): Promise<string> {
  const name = opts.name.trim();
  if (!name) throw new Error("the floor needs a name");
  if (name.toLowerCase() === "ground" || findFloor(name)) throw new Error(`a floor named '${name}' already exists`);
  const ws = useWorkspacesStore.getState();
  const root = ws.dirOf(ws.current);
  const info = await floorPrepare({
    root, scene: ws.current, name, branch: opts.branch, existingBranch: opts.existingBranch, noGit: opts.noGit,
  });
  const floor: FloorMeta = {
    id: `floor-${crypto.randomUUID().slice(0, 8)}`,
    name,
    branch: info.branch,
    worktree: info.worktree,
    dir: info.dir,
  };
  useCanvasStore.getState().addFloor(floor, opts.copyGround ? { retargetFrom: root } : undefined);
  return info.branch
    ? `floor '${name}' created — git-isolated worktree on branch '${info.branch}' at ${info.dir}`
    : `floor '${name}' created — ${info.note ?? "plain floor"}`;
}

/**
 * Deletes a floor (UI only, like Maestri). The worktree goes, its branch and
 * commits stay in the repository. `force` discards uncommitted changes.
 */
export async function deleteFloor(id: string, force: boolean): Promise<void> {
  const floor = useCanvasStore.getState().floors.find((f) => f.id === id);
  if (!floor) return;
  if (floor.worktree) await floorRemove(floor.worktree, force);
  useCanvasStore.getState().removeFloor(id);
}

/** Does the floor's worktree have uncommitted changes? */
export async function floorIsDirty(id: string): Promise<boolean> {
  const floor = useCanvasStore.getState().floors.find((f) => f.id === id);
  if (!floor?.worktree) return false;
  return (await floorStatus(floor.worktree)).dirty;
}

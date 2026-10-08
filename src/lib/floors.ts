import { GROUND, useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { toast } from "@/stores/toasts";
import {
  floorDelete, floorLand, floorPrepare, floorRemove, floorStatus, systemNotify,
  type FloorMeta, type FloorState, type FloorStatusLine,
} from "@/lib/tauri";

// Floors of the open scene: creation (git worktree when possible), status
// lines, landing and removal, shared by the floor bar and `narrater floor …`.

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
 * Deletes a floor from the floor bar. The worktree goes, its branch and
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

export const FLOOR_STATES: FloorState[] = ["working", "blocked", "review", "done"];
/** Moving into these tells the user (the work needs them, or is ready). */
const NOTIFY_STATES = new Set<FloorState>(["blocked", "review", "done"]);
const STATUS_MAX = 120;

export function floorName(id: string): string {
  if (id === GROUND) return "Ground";
  return useCanvasStore.getState().floors.find((f) => f.id === id)?.name ?? id;
}

export function statusOf(id: string): FloorStatusLine | null {
  const s = useCanvasStore.getState();
  return id === GROUND ? s.groundStatus : s.floors.find((f) => f.id === id)?.status ?? null;
}

/** "2/5", "40%" or "0.4" → display form ("2/5", "40%"); null when invalid. */
export function parseProgress(raw: string): string | null {
  const v = raw.trim();
  const frac = v.match(/^(\d+)\s*\/\s*(\d+)$/);
  if (frac) {
    const [done, total] = [Number(frac[1]), Number(frac[2])];
    return total > 0 && done <= total ? `${done}/${total}` : null;
  }
  const pct = v.match(/^(\d+(?:\.\d+)?)\s*%$/);
  const n = pct ? Number(pct[1]) / 100 : /^\d*\.?\d+$/.test(v) ? Number(v) : NaN;
  return n >= 0 && n <= 1 ? `${Math.round(n * 100)}%` : null;
}

/** One line: "[review] PR open, ready to land (3/5) · 4 min ago". */
export function describeStatus(st: FloorStatusLine | null): string {
  if (!st) return "(no status)";
  const mins = Math.round((Date.now() - st.at) / 60000);
  const ago = mins < 1 ? "just now" : mins < 60 ? `${mins} min ago` : `${Math.round(mins / 60)} h ago`;
  return `[${st.state}] ${st.text || "(no text)"}${st.progress ? ` (${st.progress})` : ""} · ${ago}`;
}

/**
 * Posts a floor's status line. Moving into blocked/review/done notifies the
 * user; new text or progress in the same state doesn't. Returns the line.
 */
export function postStatus(
  id: string,
  update: { text?: string; state?: FloorState; progress?: string | null },
  by: string,
): FloorStatusLine {
  const prev = statusOf(id);
  const text = (update.text ?? prev?.text ?? "").replace(/\s+/g, " ").trim();
  const line: FloorStatusLine = {
    text: text.length > STATUS_MAX ? `${text.slice(0, STATUS_MAX - 1)}…` : text,
    state: update.state ?? "working",
    progress: update.progress === undefined ? prev?.progress ?? null : update.progress,
    at: Date.now(),
  };
  useCanvasStore.getState().setFloorStatus(id, line);
  if (NOTIFY_STATES.has(line.state) && prev?.state !== line.state) {
    const title = `NarraTer · ${floorName(id)} is ${line.state}`;
    const body = `${by}: ${line.text || line.state}`;
    (line.state === "blocked" ? toast.error : toast.success)(`${floorName(id)} · ${line.state} — ${line.text}`);
    systemNotify(title, body).catch(() => {});
  }
  return line;
}

export interface LandOpts {
  into?: string;
  deleteBranch?: boolean;
  keepFloor?: boolean;
}

/**
 * Lands a git-isolated floor: its branch (merged into `into` when given)
 * stays in the project and the floor goes, unless keepFloor. Throws, with
 * nothing changed, on uncommitted work or a conflicting merge.
 */
export async function landFloor(id: string, opts: LandOpts): Promise<string> {
  const floor = useCanvasStore.getState().floors.find((f) => f.id === id);
  if (!floor) throw new Error("no such floor");
  if (!floor.worktree || !floor.branch) {
    throw new Error(`'${floor.name}' is a plain floor — it has no branch to land; delete it instead`);
  }
  const summary = await floorLand({ worktree: floor.worktree, branch: floor.branch, ...opts });
  if (!opts.keepFloor) useCanvasStore.getState().removeFloor(id);
  return summary;
}

/**
 * Deletes a floor from the CLI (Maestri's `floor delete`): its nodes, its
 * worktree and — unless keepBranch — its branch. Throws, with nothing
 * changed, when the worktree has uncommitted work and force is off.
 */
export async function deleteFloorFully(id: string, keepBranch: boolean, force: boolean): Promise<string> {
  const floor = useCanvasStore.getState().floors.find((f) => f.id === id);
  if (!floor) throw new Error("no such floor");
  const summary = floor.worktree
    ? await floorDelete({ worktree: floor.worktree, branch: floor.branch ?? null, keepBranch, force })
    : "plain floor (no worktree)";
  useCanvasStore.getState().removeFloor(id);
  return summary;
}

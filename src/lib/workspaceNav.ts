import { useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useSketchStore } from "@/stores/sketch";
import { saveNow } from "@/hooks/useAutoSave";

// Moving between workspaces — shared by the sidebar and the keyboard
// shortcuts (Maestri Pro's "navigate between workspaces"). Switching never
// stops anything: the workspace left behind keeps running in the background.

/** Most recently used first (this session). */
let mru: string[] = [];
let switching: Promise<void> | null = null;

/** Workspaces in sidebar order: root items, root folders, then each group. */
export function orderedWorkspaces(): string[] {
  const { list, layout } = useWorkspacesStore.getState();
  const meta = (n: string) => layout.scenes[n] ?? {};
  const at = (group: string | null, folder: string | null) =>
    list.filter((n) => (meta(n).group ?? null) === group && (meta(n).folder ?? null) === folder);
  const out = [
    ...at(null, null),
    ...layout.folders.filter((f) => !f.group).flatMap((f) => at(null, f.name)),
    ...layout.groups.flatMap((g) => [
      ...layout.folders.filter((f) => f.group === g).flatMap((f) => at(g, f.name)),
      ...at(g, null),
    ]),
  ];
  // Anything the layout doesn't place (shouldn't happen) goes last
  return [...out, ...list.filter((n) => !out.includes(n))];
}

/** Recently used workspaces, current first; the rest in sidebar order. */
export function recentWorkspaces(): string[] {
  const current = useWorkspacesStore.getState().current;
  const all = orderedWorkspaces();
  const seen = [current, ...mru.filter((n) => n !== current && all.includes(n))];
  return [...seen, ...all.filter((n) => !seen.includes(n))];
}

export async function switchWorkspace(name: string): Promise<void> {
  const ws = useWorkspacesStore.getState();
  if (name === ws.current || !ws.list.includes(name)) return;
  // One switch at a time: a burst of shortcuts lands on the last target
  if (switching) await switching;
  if (name === useWorkspacesStore.getState().current) return;
  switching = (async () => {
    const from = useWorkspacesStore.getState().current;
    // saveNow (not saveScene directly): cancels the auto-save timer — a
    // timer firing after the load would write the new canvas into the old file.
    await saveNow();
    await useCanvasStore.getState().loadScene(name);
    useWorkspacesStore.getState().setCurrent(name);
    // A stroke in progress doesn't follow (the scene's drawings do)
    useSketchStore.getState().cancelStroke();
    mru = [name, from, ...mru.filter((n) => n !== name && n !== from)];
  })();
  try {
    await switching;
  } finally {
    switching = null;
  }
}

/** Next (+1) / previous (-1) workspace in sidebar order, wrapping. */
export function cycleWorkspace(delta: number): Promise<void> {
  const all = orderedWorkspaces();
  const i = all.indexOf(useWorkspacesStore.getState().current);
  return switchWorkspace(all[(i + delta + all.length) % all.length]);
}

/** The Nth workspace in sidebar order (1-based). */
export function gotoWorkspace(n: number): Promise<void> {
  const target = orderedWorkspaces()[n - 1];
  return target ? switchWorkspace(target) : Promise.resolve();
}

/** Back to the workspace used before this one. */
export function lastWorkspace(): Promise<void> {
  const prev = recentWorkspaces()[1];
  return prev ? switchWorkspace(prev) : Promise.resolve();
}

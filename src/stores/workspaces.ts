import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { listScenes, saveScene, deleteScene, renameScene, workspacesLayout, workspaceSetDir } from "@/lib/tauri";
import type { WorkspacesLayout } from "@/lib/tauri";
import { useCanvasStore } from "@/stores/canvas";

interface WorkspacesStore {
  list: string[];
  current: string;
  /** Working directory, group and folder of each workspace (backend-owned). */
  layout: WorkspacesLayout;
  loadLayout: () => Promise<void>;
  /** A workspace's working directory (absolute), if it has one. */
  dirOf: (name: string) => string | null;
  setDir: (name: string, dir: string | null) => Promise<void>;
  loadList: () => Promise<void>;
  setCurrent: (name: string) => void;
  createWorkspace: (name: string) => Promise<void>;
  deleteWorkspace: (name: string) => Promise<void>;
  renameWorkspace: (oldName: string, newName: string) => Promise<void>;
}

export const useWorkspacesStore = create<WorkspacesStore>((set, get) => ({
  list: ["default"],
  current: "default",
  layout: { groups: [], folders: [], scenes: {} },

  loadLayout: async () => {
    try {
      set({ layout: await workspacesLayout() });
    } catch (e) {
      console.error("workspaces layout:", e);
    }
  },

  dirOf: (name) => get().layout.scenes[name]?.dir ?? null,

  setDir: async (name, dir) => {
    await workspaceSetDir(name, dir);
    await get().loadLayout();
  },

  loadList: async () => {
    try {
      let list = await listScenes();
      if (!list.includes("default")) list = ["default", ...list];
      set({ list });
    } catch {
      set({ list: ["default"] });
    }
  },

  setCurrent: (name) => set({ current: name }),

  createWorkspace: async (name) => {
    const trimmed = name.trim();
    if (!trimmed || get().list.includes(trimmed)) return;
    await saveScene(trimmed, { nodes: [], edges: [] });
    set((s) => ({ list: [...s.list, trimmed] }));
  },

  deleteWorkspace: async (name) => {
    if (name === "default") return;
    // Running in the background? Its terminals stop first
    useCanvasStore.getState().forgetScene(name);
    await deleteScene(name);
    set((s) => {
      const list = s.list.filter((w) => w !== name);
      return { list, current: s.current === name ? "default" : s.current };
    });
  },

  renameWorkspace: async (oldName, newName) => {
    const trimmed = newName.trim();
    if (!trimmed || oldName === "default" || get().list.includes(trimmed)) return;
    await renameScene(oldName, trimmed);
    useCanvasStore.getState().renameLiveScene(oldName, trimmed);
    set((s) => ({
      list: s.list.map((w) => (w === oldName ? trimmed : w)),
      current: s.current === oldName ? trimmed : s.current,
    }));
  },
}));

/**
 * Backend → sidebar: agents changed the workspace layout (`narrater
 * workspace …`), or asked for a rename — done here so the open scene's
 * name follows.
 */
export async function initWorkspaceEvents(): Promise<() => void> {
  const offChanged = await listen("workspaces_changed", () => {
    const { loadList, loadLayout } = useWorkspacesStore.getState();
    loadList();
    loadLayout();
  });
  const offRename = await listen<{ old: string; new: string }>("workspace_rename_request", (e) => {
    const { renameWorkspace, loadLayout } = useWorkspacesStore.getState();
    renameWorkspace(e.payload.old, e.payload.new).then(loadLayout).catch(console.error);
  });
  return () => {
    offChanged();
    offRename();
  };
}

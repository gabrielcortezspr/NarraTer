import { create } from "zustand";
import { listHistorias, saveHistoria, deleteHistoria, renameHistoria } from "@/lib/tauri";

interface WorkspacesStore {
  list: string[];
  current: string;
  loadList: () => Promise<void>;
  setCurrent: (name: string) => void;
  createWorkspace: (name: string) => Promise<void>;
  deleteWorkspace: (name: string) => Promise<void>;
  renameWorkspace: (oldName: string, newName: string) => Promise<void>;
}

export const useWorkspacesStore = create<WorkspacesStore>((set, get) => ({
  list: ["default"],
  current: "default",

  loadList: async () => {
    try {
      let list = await listHistorias();
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
    await saveHistoria(trimmed, { nodes: [], edges: [] });
    set((s) => ({ list: [...s.list, trimmed] }));
  },

  deleteWorkspace: async (name) => {
    if (name === "default") return;
    await deleteHistoria(name);
    set((s) => {
      const list = s.list.filter((w) => w !== name);
      return { list, current: s.current === name ? "default" : s.current };
    });
  },

  renameWorkspace: async (oldName, newName) => {
    const trimmed = newName.trim();
    if (!trimmed || oldName === "default" || get().list.includes(trimmed)) return;
    await renameHistoria(oldName, trimmed);
    set((s) => ({
      list: s.list.map((w) => (w === oldName ? trimmed : w)),
      current: s.current === oldName ? trimmed : s.current,
    }));
  },
}));

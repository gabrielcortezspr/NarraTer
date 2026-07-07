import { create } from "zustand";

export type SaveState = "saved" | "dirty" | "saving";

interface PersistenceStore {
  state: SaveState;
  set: (state: SaveState) => void;
}

// Auto-save state exposed to the UI (toolbar indicator).
export const usePersistenceStore = create<PersistenceStore>((set) => ({
  state: "saved",
  set: (state) => set({ state }),
}));

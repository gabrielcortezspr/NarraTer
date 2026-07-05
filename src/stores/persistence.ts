import { create } from "zustand";

export type SaveState = "saved" | "dirty" | "saving";

interface PersistenceStore {
  state: SaveState;
  set: (state: SaveState) => void;
}

// Estado do auto-save exposto para a UI (indicador na toolbar).
export const usePersistenceStore = create<PersistenceStore>((set) => ({
  state: "saved",
  set: (state) => set({ state }),
}));

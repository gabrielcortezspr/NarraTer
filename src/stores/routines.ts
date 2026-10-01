import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { routinesList, type RoutineView } from "@/lib/tauri";
import { toast } from "@/stores/toasts";

// Mirror of the backend scheduler's routines for the current scene: the
// backend owns them (and fires them); this store only displays.
interface RoutinesStore {
  routines: RoutineView[];
  set: (routines: RoutineView[]) => void;
}

export const useRoutinesStore = create<RoutinesStore>((set) => ({
  routines: [],
  set: (routines) => set({ routines }),
}));

/** Keeps the store in sync and toasts fires; returns the unlisten. */
export async function initRoutines(): Promise<() => void> {
  const offChanged = await listen<RoutineView[]>("routines_changed", (e) => useRoutinesStore.getState().set(e.payload));
  const offFired = await listen<{ name: string; outcome: string; notify: boolean }>("routine_fired", (e) => {
    if (!e.payload.notify) return;
    if (e.payload.outcome.startsWith("skipped")) toast.warning(`Routine ${e.payload.outcome.replace(/^skipped: /, "")}`);
    else toast.success(`Routine ${e.payload.outcome.replace(/^ok: /, "")}`);
  });
  routinesList().then((r) => useRoutinesStore.getState().set(r)).catch(console.error);
  return () => {
    offChanged();
    offFired();
  };
}

import { useEffect } from "react";
import { useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { usePersistenceStore } from "@/stores/persistence";
import { toast } from "@/stores/toasts";

const DEBOUNCE_MS = 1000;

let timer: ReturnType<typeof setTimeout> | null = null;

async function persist(): Promise<void> {
  timer = null;
  const name = useWorkspacesStore.getState().current;
  usePersistenceStore.getState().set("saving");
  try {
    await useCanvasStore.getState().saveScene(name);
    // A change may have landed during the save — in that case a new timer
    // is already armed and the state stays dirty.
    usePersistenceStore.getState().set(timer ? "dirty" : "saved");
  } catch (err) {
    console.error("auto-save failed:", err);
    toast.error(`Failed to save scene "${name}": ${err}`);
    usePersistenceStore.getState().set("dirty");
  }
}

// Flushes the pending save, if any. Required before switching scenes — a
// timer firing after the load would write the new canvas into the old file.
export function flushSave(): Promise<void> {
  if (timer) {
    clearTimeout(timer);
    return persist();
  }
  return Promise.resolve();
}

// Immediate unconditional save (Ctrl+S).
export function saveNow(): Promise<void> {
  if (timer) clearTimeout(timer);
  return persist();
}

// Watches the canvas store and persists changes with a debounce. The
// `hydrated` gate (on both sides of the transition) keeps the load itself
// from triggering a save.
export function useAutoSave() {
  useEffect(
    () =>
      useCanvasStore.subscribe((s, prev) => {
        if (!s.hydrated || !prev.hydrated) return;
        if (s.nodes === prev.nodes && s.edges === prev.edges) return;
        usePersistenceStore.getState().set("dirty");
        if (timer) clearTimeout(timer);
        timer = setTimeout(persist, DEBOUNCE_MS);
      }),
    []
  );
}

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
    await useCanvasStore.getState().saveHistoria(name);
    // Uma mudança pode ter chegado durante o save — nesse caso um novo timer
    // já está armado e o estado continua sujo.
    usePersistenceStore.getState().set(timer ? "dirty" : "saved");
  } catch (err) {
    console.error("auto-save falhou:", err);
    toast.error(`Falha ao salvar a história "${name}": ${err}`);
    usePersistenceStore.getState().set("dirty");
  }
}

// Descarrega o save pendente, se houver. Obrigatório antes de trocar de
// workspace — um timer disparando após o load gravaria o canvas novo no
// arquivo antigo.
export function flushSave(): Promise<void> {
  if (timer) {
    clearTimeout(timer);
    return persist();
  }
  return Promise.resolve();
}

// Save incondicional imediato (Ctrl+S).
export function saveNow(): Promise<void> {
  if (timer) clearTimeout(timer);
  return persist();
}

// Observa o canvas store e persiste mudanças com debounce. O gate `hydrated`
// (dos dois lados da transição) impede que o próprio load dispare um save.
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

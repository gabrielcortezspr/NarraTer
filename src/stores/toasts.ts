import { create } from "zustand";

// Visible feedback for errors that used to die in console.error (PTY spawn,
// save, editor) and warnings (agent crashed). Toasts dismiss themselves.

export type ToastKind = "error" | "warning" | "success";

export interface Toast {
  id: string;
  kind: ToastKind;
  text: string;
}

const TTL_MS = 6000;

interface ToastsStore {
  toasts: Toast[];
  push: (kind: ToastKind, text: string) => void;
  dismiss: (id: string) => void;
}

export const useToastsStore = create<ToastsStore>((set) => ({
  toasts: [],

  push: (kind, text) => {
    const id = crypto.randomUUID();
    set((s) => ({ toasts: [...s.toasts, { id, kind, text }] }));
    setTimeout(() => {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    }, TTL_MS);
  },

  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

/** Shortcut for use outside components (stores, hooks, listeners). */
export const toast = {
  error: (text: string) => useToastsStore.getState().push("error", text),
  warning: (text: string) => useToastsStore.getState().push("warning", text),
  success: (text: string) => useToastsStore.getState().push("success", text),
};

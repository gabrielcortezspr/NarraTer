import { create } from "zustand";

// The prompt composer: which terminal it writes to, and a draft per
// terminal (kept across closes and restarts — a per-viewer convenience).

const DRAFTS_KEY = "narrater.composer.drafts";

function loadDrafts(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(DRAFTS_KEY) ?? "{}") ?? {};
  } catch {
    return {};
  }
}

function saveDrafts(drafts: Record<string, string>): void {
  try {
    localStorage.setItem(DRAFTS_KEY, JSON.stringify(drafts));
  } catch {
    // storage unavailable: drafts just don't survive a restart
  }
}

interface ComposerStore {
  /** Terminal the composer is open for (null: closed) */
  target: string | null;
  drafts: Record<string, string>;
  open: (terminalId: string) => void;
  close: () => void;
  setDraft: (terminalId: string, text: string) => void;
}

export const useComposerStore = create<ComposerStore>((set, get) => ({
  target: null,
  drafts: loadDrafts(),
  open: (terminalId) => set({ target: terminalId }),
  close: () => set({ target: null }),
  setDraft: (terminalId, text) => {
    const drafts = { ...get().drafts };
    if (text) drafts[terminalId] = text;
    else delete drafts[terminalId];
    set({ drafts });
    saveDrafts(drafts);
  },
}));

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import type { ChatThread } from "@/lib/tauri";
import { useLedgerStore } from "@/stores/ledger";

// NarraTer Chat panel state: which terminal's chat is open, the thread shown
// per terminal, unread agent replies, and a version per thread so the open
// panel reloads when the backend appends.
interface ChatStore {
  openTerminal: string | null;
  threadOf: Record<string, ChatThread>;
  unread: Record<string, number>;
  versions: Record<string, number>;
  open: (terminalId: string) => void;
  close: () => void;
  setThread: (terminalId: string, thread: ChatThread) => void;
}

export const chatKey = (terminalId: string, thread: string) => `${terminalId}:${thread}`;

export const useChatStore = create<ChatStore>((set) => ({
  openTerminal: null,
  threadOf: {},
  unread: {},
  versions: {},

  open: (terminalId) => {
    // One side panel at a time
    useLedgerStore.getState().setOpenEdge(null);
    set((s) => ({ openTerminal: terminalId, unread: { ...s.unread, [terminalId]: 0 } }));
  },

  close: () => set({ openTerminal: null }),

  setThread: (terminalId, thread) => set((s) => ({ threadOf: { ...s.threadOf, [terminalId]: thread } })),
}));

/** Keeps the panel and the unread badges in sync with the backend. */
export async function initChat(): Promise<() => void> {
  return listen<{ terminal_id: string; thread: string; from_agent: boolean }>("chat_changed", (e) => {
    const { terminal_id: id, thread, from_agent } = e.payload;
    useChatStore.setState((s) => {
      const key = chatKey(id, thread);
      const visible = s.openTerminal === id && (s.threadOf[id] ?? "blue") === thread;
      return {
        versions: { ...s.versions, [key]: (s.versions[key] ?? 0) + 1 },
        unread: from_agent && !visible ? { ...s.unread, [id]: (s.unread[id] ?? 0) + 1 } : s.unread,
      };
    });
  });
}

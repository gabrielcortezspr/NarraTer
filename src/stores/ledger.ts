import { create } from "zustand";

// Observability for inter-agent conversations: the backend emits `narrater_msg`
// for each send/ask/reply/broadcast; this holds the "last activity per pair"
// (edge pulses) and which edge has the history panel open.

/** Normalized pair key — the same for A→B and B→A. */
export const pairKey = (a: string, b: string) => [a, b].sort().join("|");

export interface OpenEdge {
  source: string;
  target: string;
}

interface LedgerStore {
  /** pairKey → timestamp (ms) of the last message exchanged. */
  lastActivity: Record<string, number>;
  /** Agent-pipe edge with the history panel open, if any. */
  openEdge: OpenEdge | null;
  bump: (from: string, to: string, ts: number) => void;
  setOpenEdge: (edge: OpenEdge | null) => void;
}

export const useLedgerStore = create<LedgerStore>((set) => ({
  lastActivity: {},
  openEdge: null,

  bump: (from, to, ts) =>
    set((s) => ({ lastActivity: { ...s.lastActivity, [pairKey(from, to)]: ts } })),

  setOpenEdge: (edge) => set({ openEdge: edge }),
}));

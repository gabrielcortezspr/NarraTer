import { create } from "zustand";

// Observabilidade das conversas entre agentes: o backend emite `narrater_msg`
// para cada send/ask/reply/broadcast; aqui vive o "última atividade por par"
// (pulso das edges) e qual edge está com o painel de histórico aberto.

/** Chave normalizada do par — a mesma para A→B e B→A. */
export const pairKey = (a: string, b: string) => [a, b].sort().join("|");

export interface OpenEdge {
  source: string;
  target: string;
}

interface LedgerStore {
  /** pairKey → timestamp (ms) da última mensagem trafegada. */
  lastActivity: Record<string, number>;
  /** Edge agent-pipe com o painel de histórico aberto, se houver. */
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

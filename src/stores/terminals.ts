import { create } from "zustand";

export type SessionStatus = "spawning" | "running" | "exited";

export interface TerminalSession {
  id: string;
  status: SessionStatus;
  exitCode?: number;
}

interface TerminalsStore {
  sessions: Record<string, TerminalSession>;
  addSession: (id: string) => void;
  setRunning: (id: string) => void;
  setExited: (id: string, code: number) => void;
  removeSession: (id: string) => void;
}

export const useTerminalsStore = create<TerminalsStore>((set) => ({
  sessions: {},

  addSession: (id) =>
    set((s) => ({
      sessions: { ...s.sessions, [id]: { id, status: "spawning" } },
    })),

  setRunning: (id) =>
    set((s) => ({
      sessions: { ...s.sessions, [id]: { ...s.sessions[id], status: "running" } },
    })),

  setExited: (id, code) =>
    set((s) => ({
      sessions: {
        ...s.sessions,
        [id]: { ...s.sessions[id], status: "exited", exitCode: code },
      },
    })),

  removeSession: (id) =>
    set((s) => {
      const next = { ...s.sessions };
      delete next[id];
      return { sessions: next };
    }),
}));

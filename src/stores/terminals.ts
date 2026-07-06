import { create } from "zustand";
import type { QueueItem } from "@/lib/tauri";

export type SessionStatus = "spawning" | "running" | "idle" | "exited";

export interface TerminalSession {
  id: string;
  status: SessionStatus;
  exitCode?: number;
}

interface TerminalsStore {
  sessions: Record<string, TerminalSession>;
  /// Pending queued messages per terminal (backend inbox, event pty_queue)
  queues: Record<string, QueueItem[]>;
  addSession: (id: string) => void;
  setRunning: (id: string) => void;
  setStatus: (id: string, status: "running" | "idle") => void;
  setQueue: (id: string, items: QueueItem[]) => void;
  setExited: (id: string, code: number) => void;
  removeSession: (id: string) => void;
}

export const useTerminalsStore = create<TerminalsStore>((set) => ({
  sessions: {},
  queues: {},

  addSession: (id) =>
    set((s) => ({
      sessions: { ...s.sessions, [id]: { id, status: "spawning" } },
    })),

  setRunning: (id) =>
    set((s) => ({
      sessions: { ...s.sessions, [id]: { ...s.sessions[id], status: "running" } },
    })),

  setStatus: (id, status) =>
    set((s) => {
      const session = s.sessions[id];
      // Ignore late status ticks for sessions already gone or exited
      if (!session || session.status === "exited") return s;
      return { sessions: { ...s.sessions, [id]: { ...session, status } } };
    }),

  setQueue: (id, items) =>
    set((s) => ({ queues: { ...s.queues, [id]: items } })),

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
      const queues = { ...s.queues };
      delete queues[id];
      return { sessions: next, queues };
    }),
}));

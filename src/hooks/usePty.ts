import { useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { ptySpawn, ptyWrite, ptyResize, ptyKill, getCommandForAgent } from "@/lib/tauri";
import { useTerminalsStore } from "@/stores/terminals";
import type { AgentType } from "@/lib/tauri";

export interface PtyOutputEvent {
  id: string;
  data: string;
}

export interface PtyExitEvent {
  id: string;
  code: number;
}

export function usePty() {
  const { addSession, setRunning, setExited, removeSession } = useTerminalsStore();

  const spawn = useCallback(
    async (id: string, agentType: AgentType, cols: number, rows: number, customCommand?: string, label?: string) => {
      addSession(id);
      const command = getCommandForAgent(agentType, customCommand);
      try {
        await ptySpawn({ id, command, cols, rows, label });
        setRunning(id);
      } catch (err) {
        console.error("PTY spawn failed:", err);
        setExited(id, 1);
      }
    },
    [addSession, setRunning, setExited]
  );

  const write = useCallback((id: string, data: string) => {
    ptyWrite(id, data).catch(console.error);
  }, []);

  const resize = useCallback((id: string, cols: number, rows: number) => {
    ptyResize(id, cols, rows).catch(console.error);
  }, []);

  const kill = useCallback(
    (id: string) => {
      ptyKill(id).catch(console.error);
      removeSession(id);
    },
    [removeSession]
  );

  return { spawn, write, resize, kill };
}

export function usePtyOutput(id: string, onData: (data: string) => void) {
  return useCallback(async () => {
    const unlisten = await listen<PtyOutputEvent>("pty_output", (event) => {
      if (event.payload.id === id) {
        onData(event.payload.data);
      }
    });
    return unlisten;
  }, [id, onData]);
}

export function usePtyExit(id: string, onExit: (code: number) => void) {
  return useCallback(async () => {
    const unlisten = await listen<PtyExitEvent>("pty_exit", (event) => {
      if (event.payload.id === id) {
        onExit(event.payload.code);
      }
    });
    return unlisten;
  }, [id, onExit]);
}

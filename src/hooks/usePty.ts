import { useCallback } from "react";
import { ptySpawn, ptyWrite, ptyResize, ptyKill, getSpawnSpec } from "@/lib/tauri";
import { useTerminalsStore } from "@/stores/terminals";
import { useCanvasStore } from "@/stores/canvas";
import type { AgentType } from "@/lib/tauri";

export function usePty() {
  const { addSession, setRunning, setExited, removeSession } = useTerminalsStore();

  const spawn = useCallback(
    async (
      id: string,
      agentType: AgentType,
      cols: number,
      rows: number,
      customCommand?: string,
      label?: string,
      systemPrompt?: string
    ) => {
      addSession(id);
      const { command, args } = getSpawnSpec(agentType, customCommand, systemPrompt);
      try {
        const effectiveLabel = await ptySpawn({ id, command, args, cols, rows, label, agentType });
        setRunning(id);
        // Backend deduplicates labels (they address agents in narrater send);
        // reflect the effective one on the node so UI and routing agree
        if (effectiveLabel && effectiveLabel !== label) {
          useCanvasStore.getState().updateNodeData(id, { label: effectiveLabel });
        }
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

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import Canvas from "@/components/Canvas";
import Sidebar from "@/components/Sidebar";
import { useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useRolesStore } from "@/stores/roles";
import { useTerminalsStore } from "@/stores/terminals";
import { useLedgerStore } from "@/stores/ledger";
import { toast } from "@/stores/toasts";
import Toaster from "@/components/Toaster";
import { useAutoSave } from "@/hooks/useAutoSave";
import { initCanvasBridge } from "@/lib/canvasBridge";
import type { LedgerEntry, QueueItem } from "@/lib/tauri";

export default function App() {
  const { loadHistoria } = useCanvasStore();
  const { loadList } = useWorkspacesStore();
  const { load: loadRoles } = useRolesStore();
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  useAutoSave();

  useEffect(() => {
    const init = async () => {
      await Promise.all([loadList(), loadRoles(), loadHistoria("default")]);
    };
    init();
  }, []);

  // Global terminal status sync (backend state machine → terminals store)
  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    listen<{ id: string; status: "running" | "idle" }>("pty_status", (event) => {
      useTerminalsStore.getState().setStatus(event.payload.id, event.payload.status);
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));

    listen<{ id: string; code: number }>("pty_exit", (event) => {
      const { id, code } = event.payload;
      useTerminalsStore.getState().setExited(id, code);
      if (code !== 0) {
        const node = useCanvasStore.getState().nodes.find((n) => n.id === id);
        const label = (node?.data as { label?: string } | undefined)?.label ?? id;
        toast.warning(`"${label}" encerrou com código ${code}`);
      }
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));

    listen<{ id: string; pending: number; items: QueueItem[] }>("pty_queue", (event) => {
      useTerminalsStore.getState().setQueue(event.payload.id, event.payload.items ?? []);
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));

    // Ledger de mensagens entre agentes → pulso nas edges e histórico ao vivo
    listen<LedgerEntry>("narrater_msg", (event) => {
      const { from, to, ts } = event.payload;
      useLedgerStore.getState().bump(from, to, ts);
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));

    // Agentes manipulando o canvas via MCP (canvas_request → store → respond)
    initCanvasBridge().then((fn) => (cancelled ? fn() : unlisteners.push(fn)));

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  return (
    <div className="w-full h-full flex bg-canvas-bg overflow-hidden">
      <Sidebar
        collapsed={sidebarCollapsed}
        onToggle={() => setSidebarCollapsed((v) => !v)}
      />
      <div className="flex-1 min-w-0 relative">
        <Canvas />
      </div>
      <Toaster />
    </div>
  );
}

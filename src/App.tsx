import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import Canvas from "@/components/Canvas";
import Sidebar from "@/components/Sidebar";
import { useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useRolesStore } from "@/stores/roles";
import { useTerminalsStore } from "@/stores/terminals";

export default function App() {
  const { loadHistoria } = useCanvasStore();
  const { loadList } = useWorkspacesStore();
  const { load: loadRoles } = useRolesStore();
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

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
      useTerminalsStore.getState().setExited(event.payload.id, event.payload.code);
    }).then((fn) => (cancelled ? fn() : unlisteners.push(fn)));

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
    </div>
  );
}

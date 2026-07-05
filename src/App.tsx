import { useEffect, useState } from "react";
import Canvas from "@/components/Canvas";
import Sidebar from "@/components/Sidebar";
import { useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useRolesStore } from "@/stores/roles";

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

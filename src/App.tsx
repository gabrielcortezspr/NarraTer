import { useEffect } from "react";
import Canvas from "@/components/Canvas";
import { useCanvasStore } from "@/stores/canvas";

export default function App() {
  const { loadHistoria } = useCanvasStore();

  useEffect(() => {
    loadHistoria("default");
  }, [loadHistoria]);

  useEffect(() => {
    const handleSave = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        useCanvasStore.getState().saveHistoria("default");
      }
    };
    window.addEventListener("keydown", handleSave);
    return () => window.removeEventListener("keydown", handleSave);
  }, []);

  return (
    <div className="w-full h-full bg-canvas-bg">
      <Canvas />
    </div>
  );
}

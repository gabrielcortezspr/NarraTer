import { StickyNote, TerminalSquare } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import { useToolStore } from "@/stores/tool";
import Kbd from "@/components/ui/Kbd";

// Empty-canvas welcome — disappears once the first node is created. Only the
// buttons capture the mouse; everything else lets the canvas pan/zoom through.
export default function EmptyState({ onTerminal }: { onTerminal: () => void }) {
  const isEmpty = useCanvasStore((s) => s.hydrated && s.nodes.length === 0);
  const setTool = useToolStore((s) => s.setTool);

  if (!isEmpty) return null;

  return (
    <div className="absolute inset-0 z-10 flex items-center justify-center pointer-events-none select-none">
      <div className="flex flex-col items-center gap-5 -mt-10">
        <div className="text-2xl font-semibold tracking-tight">
          <span className="text-ink">Narra</span>
          <span className="text-accent">Ter</span>
        </div>
        <p className="text-xs text-ink-muted">Your agent canvas is empty.</p>

        <div className="flex gap-3 pointer-events-auto">
          <button
            onClick={onTerminal}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-medium text-white bg-accent-dim hover:bg-accent transition-colors"
          >
            <TerminalSquare size={14} /> New terminal
          </button>
          <button
            onClick={() => setTool("note")}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-medium text-ink-muted bg-canvas-tile border border-canvas-border hover:text-ink hover:border-canvas-hover transition-colors"
          >
            <StickyNote size={14} /> New note
          </button>
        </div>

        <div className="flex items-center gap-4 text-[10px] text-ink-faint">
          <span className="flex items-center gap-1.5"><Kbd>T</Kbd> terminal</span>
          <span className="flex items-center gap-1.5"><Kbd>N</Kbd> note</span>
          <span className="flex items-center gap-1.5"><Kbd>D</Kbd> draw</span>
          <span className="flex items-center gap-1.5"><Kbd>Ctrl+S</Kbd> save</span>
        </div>
      </div>
    </div>
  );
}

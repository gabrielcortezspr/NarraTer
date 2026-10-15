import { motion, AnimatePresence } from "framer-motion";
import {
  MousePointer2, Terminal, StickyNote, Type, Folder, Paperclip, Globe,
  Pencil, Eraser, Undo2, Check, Loader2, Save, Activity,
} from "lucide-react";
import { useActivityStore } from "@/stores/activity";
import { useToolStore, type Tool } from "@/stores/tool";
import { useSketchStore } from "@/stores/sketch";
import { usePersistenceStore } from "@/stores/persistence";
import { saveNow } from "@/hooks/useAutoSave";

// Apple's dark system colours (docs/design.md) + white
const SKETCH_COLORS = ["#8f7cff", "#ff453a", "#30d158", "#ffd60a", "#0a84ff", "#ff7eb6", "#f5f5f7"];

interface ToolButton {
  tool: Tool;
  icon: typeof MousePointer2;
  label: string;
  shortcut: string;
}

const TOOL_BUTTONS: ToolButton[] = [
  { tool: "select", icon: MousePointer2, label: "Select", shortcut: "V" },
  { tool: "terminal", icon: Terminal, label: "Terminal", shortcut: "T" },
  { tool: "note", icon: StickyNote, label: "Note", shortcut: "N" },
  { tool: "text", icon: Type, label: "Text", shortcut: "X" },
  { tool: "files", icon: Folder, label: "Files", shortcut: "F" },
  { tool: "attachment", icon: Paperclip, label: "Attachment", shortcut: "A" },
  { tool: "portal", icon: Globe, label: "Portal", shortcut: "W" },
];

const DISABLED_TOOLS: ReadonlySet<Tool> = new Set();

interface Props {
  // Terminal and Attachment are actions (they open a picker/dialog), not
  // persistent modes — the Canvas owns that flow.
  onTerminal: () => void;
  onAttachment: () => void;
}

export default function Toolbar({ onTerminal, onAttachment }: Props) {
  const { active, setTool } = useToolStore();
  const { undo: undoSketch, clear: clearSketch, color, setColor, size, setSize } = useSketchStore();
  const saveState = usePersistenceStore((s) => s.state);
  const activityOpen = useActivityStore((s) => s.open);
  const toggleActivity = useActivityStore((s) => s.toggle);

  const handleTool = (tool: Tool) => {
    if (DISABLED_TOOLS.has(tool)) return;
    if (tool === "terminal") { onTerminal(); return; }
    if (tool === "attachment") { onAttachment(); return; }
    setTool(active === tool && tool !== "select" ? "select" : tool);
  };

  return (
    <div className="absolute top-4 left-1/2 -translate-x-1/2 z-30">
      <div
        className="glass flex items-center gap-0.5 p-1 rounded-full border border-line shadow-float"
      >
        {TOOL_BUTTONS.map(({ tool, icon: Icon, label, shortcut }) => {
          const isActive = active === tool;
          const disabled = DISABLED_TOOLS.has(tool);
          return (
            <motion.button
              key={tool}
              whileTap={disabled ? undefined : { scale: 0.9 }}
              onClick={() => handleTool(tool)}
              title={disabled ? `${label} — coming soon` : `${label} (${shortcut})`}
              className={`w-8 h-8 flex items-center justify-center rounded-full transition-colors
                ${isActive
                  ? "bg-accent/20 text-accent-text"
                  : disabled
                  ? "text-ink-faint cursor-not-allowed"
                  : "text-ink-2 hover:text-ink hover:bg-hover"}`}
            >
              <Icon size={15} />
            </motion.button>
          );
        })}

        <div className="w-px h-5 bg-line mx-0.5" />

        {/* Draw + expandable sub-controls */}
        <motion.button
          whileTap={{ scale: 0.9 }}
          onClick={() => handleTool("draw")}
          title="Draw (D)"
          className={`w-8 h-8 flex items-center justify-center rounded-full transition-colors
            ${active === "draw"
              ? "bg-accent/20 text-accent-text"
              : "text-ink-2 hover:text-ink hover:bg-hover"}`}
        >
          <Pencil size={15} />
        </motion.button>

        <AnimatePresence>
          {active === "draw" && (
            <motion.div
              initial={{ opacity: 0, width: 0 }}
              animate={{ opacity: 1, width: "auto" }}
              exit={{ opacity: 0, width: 0 }}
              className="flex items-center gap-1 overflow-hidden"
            >
              {SKETCH_COLORS.map((c) => (
                <button
                  key={c}
                  onClick={() => setColor(c)}
                  className="w-4 h-4 rounded-full border-2 transition-transform hover:scale-110 shrink-0"
                  style={{ background: c, borderColor: c === color ? "var(--text)" : "transparent" }}
                  title="Color"
                />
              ))}
              <button
                onClick={() => setSize(Math.max(2, size - 2))}
                className="w-6 h-6 flex items-center justify-center rounded-full text-ink-2 hover:text-ink text-xs shrink-0"
              >
                −
              </button>
              <span className="text-2xs tabular-nums text-ink-muted w-4 text-center shrink-0">{size}</span>
              <button
                onClick={() => setSize(Math.min(20, size + 2))}
                className="w-6 h-6 flex items-center justify-center rounded-full text-ink-2 hover:text-ink text-xs shrink-0"
              >
                +
              </button>
              <button
                onClick={undoSketch}
                title="Undo (Ctrl+Z)"
                className="w-6 h-6 flex items-center justify-center rounded-full text-ink-2 hover:text-ink shrink-0"
              >
                <Undo2 size={12} />
              </button>
              <button
                onClick={clearSketch}
                title="Clear sketch"
                className="w-6 h-6 flex items-center justify-center rounded-full text-danger-text hover:text-ink shrink-0"
              >
                <Eraser size={12} />
              </button>
            </motion.div>
          )}
        </AnimatePresence>

        <div className="w-px h-5 bg-line mx-0.5" />

        {/* Activity log (Ctrl+J) */}
        <button
          onClick={() => toggleActivity()}
          title="Activity — states, asks and messages (Ctrl+J)"
          className={`flex items-center justify-center w-8 h-8 rounded-full transition-colors ${
            activityOpen ? "bg-accent/15 text-accent-text" : "text-ink-2 hover:bg-hover hover:text-ink"
          }`}
        >
          <Activity size={14} />
        </button>

        {/* Auto-save indicator (Ctrl+S forces) */}
        <button
          onClick={() => saveNow().catch(console.error)}
          title="Auto-save on — Ctrl+S saves now"
          className={`flex items-center gap-1 px-2.5 h-8 rounded-full text-2xs font-medium transition-colors hover:bg-hover ${
            saveState === "saving" ? "text-accent-text" : saveState === "saved" ? "text-ink-muted" : "text-ink-2"
          }`}
        >
          {saveState === "saving" ? (
            <Loader2 size={11} className="animate-spin" />
          ) : saveState === "saved" ? (
            <Check size={11} />
          ) : (
            <Save size={11} />
          )}
          {saveState === "saving" ? "Saving" : saveState === "saved" ? "Saved" : "Save"}
        </button>
      </div>
    </div>
  );
}

import { motion, AnimatePresence } from "framer-motion";
import {
  MousePointer2, Terminal, StickyNote, Type, Folder, Paperclip, Globe,
  Pencil, Eraser, Undo2, Check, Loader2, Save,
} from "lucide-react";
import { useToolStore, type Tool } from "@/stores/tool";
import { useSketchStore } from "@/stores/sketch";
import { usePersistenceStore } from "@/stores/persistence";
import { saveNow } from "@/hooks/useAutoSave";

const SKETCH_COLORS = ["#8b5cf6", "#f87171", "#4ade80", "#fbbf24", "#60a5fa", "#f472b6", "#ffffff"];

interface ToolButton {
  tool: Tool;
  icon: typeof MousePointer2;
  label: string;
  shortcut: string;
}

const TOOL_BUTTONS: ToolButton[] = [
  { tool: "select", icon: MousePointer2, label: "Seletor", shortcut: "V" },
  { tool: "terminal", icon: Terminal, label: "Terminal", shortcut: "T" },
  { tool: "note", icon: StickyNote, label: "Nota", shortcut: "N" },
  { tool: "text", icon: Type, label: "Texto", shortcut: "X" },
  { tool: "files", icon: Folder, label: "Arquivos", shortcut: "F" },
  { tool: "attachment", icon: Paperclip, label: "Anexo", shortcut: "A" },
  { tool: "portal", icon: Globe, label: "Portal", shortcut: "W" },
];

// Ferramentas ainda sem tile implementado — botão visível porém inerte.
const DISABLED_TOOLS: ReadonlySet<Tool> = new Set(["portal"]);

interface Props {
  // Terminal e Anexo são ações (abrem picker/dialog), não modos persistentes —
  // o dono do fluxo é o Canvas.
  onTerminal: () => void;
  onAttachment: () => void;
}

export default function Toolbar({ onTerminal, onAttachment }: Props) {
  const { active, setTool } = useToolStore();
  const { undo: undoSketch, clear: clearSketch, color, setColor, size, setSize } = useSketchStore();
  const saveState = usePersistenceStore((s) => s.state);

  const handleTool = (tool: Tool) => {
    if (DISABLED_TOOLS.has(tool)) return;
    if (tool === "terminal") { onTerminal(); return; }
    if (tool === "attachment") { onAttachment(); return; }
    setTool(active === tool && tool !== "select" ? "select" : tool);
  };

  return (
    <div className="absolute top-4 left-1/2 -translate-x-1/2 z-30">
      <div
        className="flex items-center gap-1 px-2 py-1.5 rounded-full border border-[#2a2a2a]
          bg-[#161616]/95 backdrop-blur"
        style={{ boxShadow: "0 8px 32px rgba(0,0,0,0.5)" }}
      >
        {TOOL_BUTTONS.map(({ tool, icon: Icon, label, shortcut }) => {
          const isActive = active === tool;
          const disabled = DISABLED_TOOLS.has(tool);
          return (
            <motion.button
              key={tool}
              whileTap={disabled ? undefined : { scale: 0.9 }}
              onClick={() => handleTool(tool)}
              title={disabled ? `${label} — em breve` : `${label} (${shortcut})`}
              className={`w-8 h-8 flex items-center justify-center rounded-full transition-colors
                ${isActive
                  ? "bg-[#8b5cf6]/20 text-[#8b5cf6]"
                  : disabled
                  ? "text-[#3a3a3a] cursor-not-allowed"
                  : "text-[#8a8a8a] hover:text-white hover:bg-[#222]"}`}
            >
              <Icon size={15} />
            </motion.button>
          );
        })}

        <div className="w-px h-5 bg-[#2a2a2a] mx-0.5" />

        {/* Desenho + sub-controles expansíveis */}
        <motion.button
          whileTap={{ scale: 0.9 }}
          onClick={() => handleTool("draw")}
          title="Desenho (D)"
          className={`w-8 h-8 flex items-center justify-center rounded-full transition-colors
            ${active === "draw"
              ? "bg-[#8b5cf6]/20 text-[#8b5cf6]"
              : "text-[#8a8a8a] hover:text-white hover:bg-[#222]"}`}
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
                  style={{ background: c, borderColor: c === color ? "#fff" : "transparent" }}
                  title="Cor"
                />
              ))}
              <button
                onClick={() => setSize(Math.max(2, size - 2))}
                className="w-6 h-6 flex items-center justify-center rounded-full text-[#888] hover:text-white text-xs shrink-0"
              >
                −
              </button>
              <span className="text-[10px] text-[#666] w-3 text-center shrink-0">{size}</span>
              <button
                onClick={() => setSize(Math.min(20, size + 2))}
                className="w-6 h-6 flex items-center justify-center rounded-full text-[#888] hover:text-white text-xs shrink-0"
              >
                +
              </button>
              <button
                onClick={undoSketch}
                title="Desfazer (Ctrl+Z)"
                className="w-6 h-6 flex items-center justify-center rounded-full text-[#888] hover:text-white shrink-0"
              >
                <Undo2 size={12} />
              </button>
              <button
                onClick={clearSketch}
                title="Limpar desenho"
                className="w-6 h-6 flex items-center justify-center rounded-full text-[#f87171] hover:text-white shrink-0"
              >
                <Eraser size={12} />
              </button>
            </motion.div>
          )}
        </AnimatePresence>

        <div className="w-px h-5 bg-[#2a2a2a] mx-0.5" />

        {/* Indicador de auto-save (Ctrl+S força) */}
        <button
          onClick={() => saveNow().catch(console.error)}
          title="Auto-save ativo — Ctrl+S salva agora"
          className="flex items-center gap-1 px-2 h-8 rounded-full text-[10px] transition-colors hover:bg-[#222]"
          style={{ color: saveState === "saved" ? "#4ade80" : saveState === "saving" ? "#8b5cf6" : "#6b7280" }}
        >
          {saveState === "saving" ? (
            <Loader2 size={11} className="animate-spin" />
          ) : saveState === "saved" ? (
            <Check size={11} />
          ) : (
            <Save size={11} />
          )}
          {saveState === "saving" ? "Salvando" : saveState === "saved" ? "Salvo" : "Salvar"}
        </button>
      </div>
    </div>
  );
}

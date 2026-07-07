import { useEffect, useMemo, useRef, useState } from "react";
import { useReactFlow } from "@xyflow/react";
import { Bot, Code2, FileText, Globe, Image, Save, Search, StickyNote, Terminal, TerminalSquare, Type, Wrench } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { AppNode, TerminalNodeData } from "@/stores/canvas";
import { useToolStore } from "@/stores/tool";
import { saveNow } from "@/hooks/useAutoSave";
import { toast } from "@/stores/toasts";
import Kbd from "@/components/ui/Kbd";

// Command palette (Ctrl+K): find an agent/node by name without pan-and-squint,
// and trigger quick actions. Enter frames and selects the chosen node.

interface PaletteItem {
  key: string;
  icon: React.ReactNode;
  label: string;
  sub?: string;
  run: () => void;
}

function nodeIcon(node: AppNode): React.ReactNode {
  switch (node.type) {
    case "terminal": {
      const t = (node.data as TerminalNodeData).agentType;
      return t === "claude" ? <Bot size={13} /> : t === "codex" ? <Code2 size={13} /> : t === "custom" ? <Wrench size={13} /> : <Terminal size={13} />;
    }
    case "note": return <StickyNote size={13} />;
    case "text": return <Type size={13} />;
    case "filetree": return <FileText size={13} />;
    case "attachment": return <Image size={13} />;
    case "portal": return <Globe size={13} />;
    default: return <Search size={13} />;
  }
}

function nodeLabel(node: AppNode): string {
  const data = node.data as { label?: string; text?: string; url?: string; fileName?: string };
  return data.label ?? data.fileName ?? data.url ?? data.text?.slice(0, 40) ?? node.id;
}

interface Props {
  open: boolean;
  onClose: () => void;
  onNewTerminal: () => void;
}

export default function CommandPalette({ open, onClose, onNewTerminal }: Props) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const nodes = useCanvasStore((s) => s.nodes);
  const setTool = useToolStore((s) => s.setTool);
  const { fitView } = useReactFlow();

  useEffect(() => {
    if (open) {
      setQuery("");
      setIndex(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  const items = useMemo<PaletteItem[]>(() => {
    const focusNode = (id: string) => {
      onClose();
      const { onNodesChange, nodes: all } = useCanvasStore.getState();
      onNodesChange(all.map((n) => ({ id: n.id, type: "select" as const, selected: n.id === id })));
      fitView({ nodes: [{ id }], maxZoom: 1.1, duration: 250 });
    };

    const nodeItems: PaletteItem[] = nodes.map((n) => {
      const data = n.data as TerminalNodeData;
      return {
        key: n.id,
        icon: nodeIcon(n),
        label: nodeLabel(n),
        sub: n.type === "terminal" ? (data.roleName ?? data.agentType) : n.type ?? undefined,
        run: () => focusNode(n.id),
      };
    });

    const actions: PaletteItem[] = [
      { key: "act-terminal", icon: <TerminalSquare size={13} />, label: "New terminal", sub: "action", run: () => { onClose(); onNewTerminal(); } },
      { key: "act-note", icon: <StickyNote size={13} />, label: "New note", sub: "action", run: () => { onClose(); setTool("note"); } },
      {
        key: "act-save",
        icon: <Save size={13} />,
        label: "Save now",
        sub: "action",
        run: () => {
          onClose();
          saveNow().then(() => toast.success("Scene saved")).catch((e) => toast.error(`Failed to save: ${e}`));
        },
      },
    ];

    const q = query.trim().toLowerCase();
    const all = [...nodeItems, ...actions];
    if (!q) return all;
    return all.filter((i) => i.label.toLowerCase().includes(q) || i.sub?.toLowerCase().includes(q));
  }, [nodes, query, fitView, onClose, onNewTerminal, setTool]);

  useEffect(() => {
    setIndex((i) => Math.min(i, Math.max(items.length - 1, 0)));
  }, [items.length]);

  if (!open) return null;

  return (
    <div
      className="absolute inset-0 z-50 flex items-start justify-center pt-[18vh] bg-black/40"
      onMouseDown={onClose}
    >
      <div
        className="w-[440px] max-w-[90%] rounded-xl overflow-hidden bg-canvas-panel border border-canvas-border shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 px-3 py-2.5 border-b border-canvas-border">
          <Search size={13} className="text-ink-faint shrink-0" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") { e.preventDefault(); onClose(); }
              else if (e.key === "ArrowDown") { e.preventDefault(); setIndex((i) => Math.min(i + 1, items.length - 1)); }
              else if (e.key === "ArrowUp") { e.preventDefault(); setIndex((i) => Math.max(i - 1, 0)); }
              else if (e.key === "Enter") { e.preventDefault(); items[index]?.run(); }
            }}
            placeholder="Search agents, nodes or actions…"
            aria-label="Search agents, nodes or actions"
            className="flex-1 bg-transparent text-xs text-ink outline-none placeholder-[#555]"
          />
          <Kbd>Esc</Kbd>
        </div>

        <div className="max-h-[300px] overflow-y-auto py-1">
          {items.length === 0 && (
            <div className="px-3 py-4 text-center text-[10px] text-ink-faint">Nothing found.</div>
          )}
          {items.map((item, i) => (
            <button
              key={item.key}
              onClick={item.run}
              onMouseEnter={() => setIndex(i)}
              className={`flex items-center gap-2.5 w-full px-3 py-2 text-left transition-colors ${
                i === index ? "bg-canvas-border text-ink" : "text-ink-muted"
              }`}
            >
              <span className={i === index ? "text-accent" : "text-ink-faint"}>{item.icon}</span>
              <span className="text-xs truncate flex-1">{item.label}</span>
              {item.sub && <span className="text-[9px] text-ink-faint uppercase tracking-wide shrink-0">{item.sub}</span>}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

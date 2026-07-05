import { useEffect, useRef, useCallback } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import { listen } from "@tauri-apps/api/event";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Bot, Code2, Terminal, Wrench, X, GripVertical } from "lucide-react";
import { usePty } from "@/hooks/usePty";
import { useCanvasStore } from "@/stores/canvas";
import type { TerminalNodeData } from "@/stores/canvas";
import type { Node, NodeProps } from "@xyflow/react";
import type { AgentType } from "@/lib/tauri";
import "@xterm/xterm/css/xterm.css";

type TerminalNode = Node<TerminalNodeData, "terminal">;

const AGENT_COLORS: Record<AgentType, string> = {
  shell: "#6b7280",
  claude: "#8b5cf6",
  codex: "#3b82f6",
  custom: "#14b8a6",
};

const AGENT_ICONS: Record<AgentType, React.ReactNode> = {
  shell: <Terminal size={12} />,
  claude: <Bot size={12} />,
  codex: <Code2 size={12} />,
  custom: <Wrench size={12} />,
};

const AGENT_LABELS: Record<AgentType, string> = {
  shell: "Shell",
  claude: "Claude",
  codex: "Codex",
  custom: "Custom",
};

const XTERM_THEME = {
  background: "#0e0e0e",
  foreground: "#e5e7eb",
  cursor: "#8b5cf6",
  cursorAccent: "#0e0e0e",
  selection: "rgba(139,92,246,0.3)",
  black: "#1a1a2e",
  red: "#f87171",
  green: "#4ade80",
  yellow: "#fbbf24",
  blue: "#60a5fa",
  magenta: "#c084fc",
  cyan: "#22d3ee",
  white: "#e5e7eb",
  brightBlack: "#374151",
  brightRed: "#fca5a5",
  brightGreen: "#86efac",
  brightYellow: "#fde68a",
  brightBlue: "#93c5fd",
  brightMagenta: "#d8b4fe",
  brightCyan: "#67e8f9",
  brightWhite: "#f9fafb",
};

export default function TerminalTile({ id, data, selected }: NodeProps<TerminalNode>) {
  const termDivRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const { spawn, write, kill } = usePty();
  const removeNode = useCanvasStore((s) => s.removeNode);
  const agentType = data.agentType ?? "shell";
  const accentColor = AGENT_COLORS[agentType];

  // Init xterm and PTY
  useEffect(() => {
    if (!termDivRef.current) return;

    const term = new XTerm({
      theme: XTERM_THEME,
      fontFamily: '"JetBrains Mono", "Cascadia Code", "Fira Code", monospace',
      fontSize: 13,
      lineHeight: 1.4,
      cursorBlink: true,
      cursorStyle: "block",
      scrollback: 5000,
      allowTransparency: true,
    });

    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);
    term.open(termDivRef.current);

    // Delay fit to let layout settle
    setTimeout(() => {
      fitAddon.fit();
      spawn(id, agentType, term.cols, term.rows, data.command);
    }, 50);

    term.onData((data) => write(id, data));

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    // Observe container resize
    const observer = new ResizeObserver(() => {
      if (fitAddonRef.current && xtermRef.current) {
        try {
          fitAddonRef.current.fit();
        } catch {}
      }
    });
    if (termDivRef.current.parentElement) {
      observer.observe(termDivRef.current.parentElement);
    }
    resizeObserverRef.current = observer;

    return () => {
      observer.disconnect();
      term.dispose();
      kill(id);
    };
  }, []);  // intentionally run once

  // Listen for PTY output
  useEffect(() => {
    let unlistenOutput: (() => void) | undefined;
    let unlistenExit: (() => void) | undefined;

    listen<{ id: string; data: string }>("pty_output", (event) => {
      if (event.payload.id === id && xtermRef.current) {
        xtermRef.current.write(event.payload.data);
      }
    }).then((fn) => { unlistenOutput = fn; });

    listen<{ id: string; code: number }>("pty_exit", (event) => {
      if (event.payload.id === id && xtermRef.current) {
        xtermRef.current.writeln("\r\n\x1b[2m[Process exited]\x1b[0m");
      }
    }).then((fn) => { unlistenExit = fn; });

    return () => {
      unlistenOutput?.();
      unlistenExit?.();
    };
  }, [id]);

  const handleClose = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      removeNode(id);
    },
    [id, removeNode]
  );

  return (
    <div
      className="flex flex-col w-full h-full rounded-xl overflow-hidden"
      style={{
        background: "#1a1a1a",
        boxShadow: selected
          ? `0 0 0 1px ${accentColor}, 0 8px 32px rgba(0,0,0,0.5)`
          : "0 4px 24px rgba(0,0,0,0.4), 0 0 0 1px rgba(255,255,255,0.06)",
        transition: "box-shadow 0.15s ease",
      }}
    >
      <NodeResizer
        minWidth={360}
        minHeight={220}
        isVisible={selected}
        lineStyle={{ borderColor: accentColor }}
        handleStyle={{ borderColor: accentColor, background: "#1a1a1a" }}
      />

      {/* Header — drag handle */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab active:cursor-grabbing select-none"
        style={{
          background: "#222",
          borderBottom: `1px solid #2a2a2a`,
        }}
      >
        <GripVertical size={12} className="text-[#444] shrink-0" />

        {/* Agent badge */}
        <span
          className="flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded"
          style={{ color: accentColor, background: `${accentColor}18` }}
        >
          {AGENT_ICONS[agentType]}
          {AGENT_LABELS[agentType]}
        </span>

        <span className="text-[#555] text-xs truncate flex-1">{data.label}</span>

        {/* Close */}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleClose}
          className="text-[#555] hover:text-[#f87171] transition-colors p-0.5 rounded hover:bg-[#2a2a2a] nodrag"
        >
          <X size={12} />
        </button>
      </div>

      {/* Terminal body */}
      <div
        ref={termDivRef}
        className="flex-1 min-h-0 nodrag nowheel"
        style={{ padding: "4px 2px 2px 4px", background: XTERM_THEME.background }}
        onMouseDown={(e) => e.stopPropagation()}
      />

      {/* Connection handles */}
      <Handle type="target" position={Position.Left} style={{ background: accentColor, border: "none", width: 8, height: 8 }} />
      <Handle type="source" position={Position.Right} style={{ background: accentColor, border: "none", width: 8, height: 8 }} />
    </div>
  );
}

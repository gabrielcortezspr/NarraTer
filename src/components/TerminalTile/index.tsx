import { useEffect, useRef, useCallback, useState } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import { listen } from "@tauri-apps/api/event";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Bot, Code2, Terminal, Wrench, X, GripVertical, FolderOpen, Clock, Plug } from "lucide-react";
import { usePty } from "@/hooks/usePty";
import { useCanvasStore } from "@/stores/canvas";
import { useTerminalsStore } from "@/stores/terminals";
import type { SessionStatus } from "@/stores/terminals";
import { openInEditor } from "@/lib/tauri";
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

const STATUS_DOT: Record<SessionStatus, { color: string; label: string }> = {
  spawning: { color: "#fbbf24", label: "Iniciando…" },
  running: { color: "#4ade80", label: "Executando" },
  idle: { color: "#6b7280", label: "Ocioso" },
  exited: { color: "#f87171", label: "Encerrado" },
};

const EDITORS = [
  { label: "VS Code", cmd: "code" },
  { label: "Zed", cmd: "zed" },
  { label: "Cursor", cmd: "cursor" },
  { label: "Neovim", cmd: "nvim" },
];

export default function TerminalTile({ id, data, selected }: NodeProps<TerminalNode>) {
  const termDivRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const scheduleTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const cwdRef = useRef<string>("~");
  const { spawn, write, resize, kill } = usePty();
  const removeNode = useCanvasStore((s) => s.removeNode);
  const pipeCount = useCanvasStore((s) =>
    s.edges.filter((e) => (e.source === id || e.target === id) && e.type === "agent-pipe").length
  );
  const agentType = data.agentType ?? "shell";
  const accentColor = AGENT_COLORS[agentType];
  const sessionStatus = useTerminalsStore((s) => s.sessions[id]?.status ?? "spawning");
  const statusDot = STATUS_DOT[sessionStatus];
  const [showEditorMenu, setShowEditorMenu] = useState(false);

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

    setTimeout(() => {
      fitAddon.fit();
      spawn(id, agentType, term.cols, term.rows, data.command, data.label).then(() => {
        if (data.instructions?.trim()) {
          write(id, data.instructions.trim() + "\n");
        }
        // If this terminal has pipe connections (loaded from historia), send skill description
        const { edges, nodes } = useCanvasStore.getState();
        const pipeEdges = edges.filter((e) => (e.source === id || e.target === id) && e.type === "agent-pipe");
        if (pipeEdges.length > 0) {
          const connectedLabels = pipeEdges.map((e) => {
            const otherId = e.source === id ? e.target : e.source;
            const otherNode = nodes.find((n) => n.id === otherId);
            return (otherNode?.data as TerminalNodeData | undefined)?.label ?? otherId;
          });
          const skillMsg =
            `\r\n\x1b[35m[NarraTer]\x1b[0m Agentes conectados: \x1b[1m${connectedLabels.join(", ")}\x1b[0m\r\n` +
            `Use: \x1b[36mnarrater send "<nome>" "mensagem"\x1b[0m\r\n\r\n`;
          write(id, skillMsg);
        }
      });
    }, 50);

    term.onData((d) => write(id, d));

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    const observer = new ResizeObserver(() => {
      if (fitAddonRef.current && xtermRef.current) {
        try {
          fitAddonRef.current.fit();
          resize(id, xtermRef.current.cols, xtermRef.current.rows);
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
  }, []); // intentionally run once

  // PTY output listener
  useEffect(() => {
    let cancelled = false;
    let unlistenOutput: (() => void) | undefined;
    let unlistenExit: (() => void) | undefined;
    let outputBuffer = "";

    listen<{ id: string; data: string }>("pty_output", (event) => {
      if (event.payload.id !== id) return;
      if (xtermRef.current) {
        xtermRef.current.write(event.payload.data);
      }
      // Track cwd from shell prompt (heuristic: capture last line with $)
      outputBuffer += event.payload.data;
      const lines = outputBuffer.split("\n");
      outputBuffer = lines[lines.length - 1];
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenOutput = fn;
    });

    listen<{ id: string; code: number }>("pty_exit", (event) => {
      if (event.payload.id === id && xtermRef.current) {
        xtermRef.current.writeln("\r\n\x1b[2m[Process exited]\x1b[0m");
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenExit = fn;
    });

    return () => {
      cancelled = true;
      unlistenOutput?.();
      unlistenExit?.();
    };
  }, [id]);

  // Scheduled prompts
  useEffect(() => {
    if (!data.scheduleCommand?.trim() || !data.scheduleIntervalSecs) return;
    const cmd = data.scheduleCommand.trim();
    const ms = data.scheduleIntervalSecs * 1000;
    scheduleTimerRef.current = setInterval(() => {
      write(id, cmd + "\n");
    }, ms);
    return () => {
      if (scheduleTimerRef.current) clearInterval(scheduleTimerRef.current);
    };
  }, [id, data.scheduleCommand, data.scheduleIntervalSecs, write]);

  const handleClose = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      removeNode(id);
    },
    [id, removeNode]
  );

  const handleOpenEditor = useCallback(
    async (editorCmd: string) => {
      setShowEditorMenu(false);
      try {
        await openInEditor(editorCmd, cwdRef.current);
      } catch (err) {
        console.warn("Failed to open editor:", err);
      }
    },
    []
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

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab active:cursor-grabbing select-none"
        style={{ background: "#222", borderBottom: "1px solid #2a2a2a" }}
      >
        <GripVertical size={12} className="text-[#444] shrink-0" />

        <span
          title={statusDot.label}
          className="shrink-0 rounded-full"
          style={{
            width: 7,
            height: 7,
            background: statusDot.color,
            boxShadow: sessionStatus === "running" ? `0 0 6px ${statusDot.color}` : "none",
          }}
        />

        <span
          className="flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded"
          style={{ color: accentColor, background: `${accentColor}18` }}
        >
          {AGENT_ICONS[agentType]}
          {AGENT_LABELS[agentType]}
        </span>

        {/* Role badge */}
        {data.roleName && (
          <span
            className="flex items-center gap-1 text-[9px] font-medium px-1.5 py-0.5 rounded-full shrink-0"
            style={{ color: data.roleColor ?? "#888", background: `${data.roleColor ?? "#888"}18`, border: `1px solid ${data.roleColor ?? "#888"}30` }}
          >
            {data.roleName}
          </span>
        )}

        <span className="text-[#555] text-xs truncate flex-1">{data.label}</span>

        {/* Agent pipe badge */}
        {pipeCount > 0 && (
          <span
            title={`Conectado a ${pipeCount} agente${pipeCount > 1 ? "s" : ""} via pipe`}
            className="flex items-center gap-0.5 text-[9px] font-medium px-1.5 py-0.5 rounded-full shrink-0"
            style={{ color: "#a78bfa", background: "#8b5cf618", border: "1px solid #8b5cf630" }}
          >
            <Plug size={8} />
            {pipeCount}
          </span>
        )}

        {/* Scheduled prompt indicator */}
        {data.scheduleCommand?.trim() && (
          <span
            title={`Agendado: "${data.scheduleCommand}" a cada ${data.scheduleIntervalSecs}s`}
            className="flex items-center gap-0.5 text-[9px] text-[#fbbf24] opacity-70"
          >
            <Clock size={9} />
            {data.scheduleIntervalSecs}s
          </span>
        )}

        {/* Open in editor */}
        <div className="relative nodrag">
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => { e.stopPropagation(); setShowEditorMenu((v) => !v); }}
            className="text-[#555] hover:text-[#60a5fa] transition-colors p-0.5 rounded hover:bg-[#2a2a2a]"
            title="Abrir no editor"
          >
            <FolderOpen size={12} />
          </button>
          {showEditorMenu && (
            <div
              className="absolute top-6 right-0 bg-[#1a1a1a] border border-[#2a2a2a] rounded-lg shadow-xl py-1 z-50 min-w-[110px]"
              onMouseDown={(e) => e.stopPropagation()}
            >
              {EDITORS.map((ed) => (
                <button
                  key={ed.cmd}
                  onClick={() => handleOpenEditor(ed.cmd)}
                  className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[#ccc] hover:bg-[#2a2a2a] hover:text-white transition-colors whitespace-nowrap"
                >
                  {ed.label}
                </button>
              ))}
            </div>
          )}
        </div>

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

      <Handle type="target" position={Position.Left} style={{ background: accentColor, border: "none", width: 8, height: 8 }} />
      <Handle type="source" position={Position.Right} style={{ background: accentColor, border: "none", width: 8, height: 8 }} />
    </div>
  );
}

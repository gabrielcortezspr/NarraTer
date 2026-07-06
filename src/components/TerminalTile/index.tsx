import { memo, useEffect, useRef, useCallback, useState } from "react";
import { Handle, Position, NodeResizer, useStore } from "@xyflow/react";
import { Bot, Code2, Terminal, Wrench, X, GripVertical, FolderOpen, Clock, Plug, RotateCcw } from "lucide-react";
import { attachTerminal, detachTerminal, ensureTerminal, fitTerminal, respawnTerminal } from "@/lib/terminalManager";
import { ptyWrite } from "@/lib/tauri";
import { buildAgentSystemPrompt } from "@/lib/agentPrompt";
import { useCanvasStore } from "@/stores/canvas";
import { useTerminalsStore } from "@/stores/terminals";
import type { SessionStatus } from "@/stores/terminals";
import { openInEditor, ptyQueueCancel } from "@/lib/tauri";
import type { QueueItem } from "@/lib/tauri";
import Badge from "@/components/ui/Badge";
import IconButton from "@/components/ui/IconButton";
import { toast } from "@/stores/toasts";
import { EDITORS } from "@/lib/editors";
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

const TERMINAL_BG = "#0e0e0e";

// Abaixo deste zoom o conteúdo do terminal é ilegível — troca por card (LOD)
const LOD_ZOOM = 0.35;

// Referência estável para o selector da fila (evita re-render por identidade)
const NO_QUEUE: QueueItem[] = [];

const STATUS_DOT: Record<SessionStatus, { color: string; label: string }> = {
  spawning: { color: "#fbbf24", label: "Iniciando…" },
  running: { color: "#4ade80", label: "Executando" },
  idle: { color: "#6b7280", label: "Ocioso" },
  exited: { color: "#f87171", label: "Encerrado" },
};

function TerminalTile({ id, data, selected }: NodeProps<TerminalNode>) {
  const termDivRef = useRef<HTMLDivElement>(null);
  const scheduleTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const cwdRef = useRef<string>("~");
  // Só re-renderiza ao cruzar o limiar, não a cada tick de zoom
  const lod = useStore((s) => s.transform[2] < LOD_ZOOM);
  const removeNode = useCanvasStore((s) => s.removeNode);
  const pipeCount = useCanvasStore((s) =>
    s.edges.filter((e) => (e.source === id || e.target === id) && e.type === "agent-pipe").length
  );
  const agentType = data.agentType ?? "shell";
  const accentColor = AGENT_COLORS[agentType];
  const sessionStatus = useTerminalsStore((s) => s.sessions[id]?.status ?? "spawning");
  const exitCode = useTerminalsStore((s) => s.sessions[id]?.exitCode);
  const statusDot = STATUS_DOT[sessionStatus];
  const queueItems = useTerminalsStore((s) => s.queues[id] ?? NO_QUEUE);
  const queuePending = queueItems.length;
  const [showEditorMenu, setShowEditorMenu] = useState(false);
  const [showQueueMenu, setShowQueueMenu] = useState(false);

  // Anexa o terminal gerenciado (xterm/PTY vivem fora do React — desmontar o
  // tile por culling/LOD não mata o processo nem perde o scrollback)
  useEffect(() => {
    const parent = termDivRef.current;
    if (!parent) return;

    // Claude gets identity/role/protocol as a durable system prompt (plus
    // the narrater MCP tools); other agents keep the composer injection
    const isClaude = agentType === "claude";
    const { edges, nodes } = useCanvasStore.getState();
    const labelFor = (nodeId: string) => {
      const node = nodes.find((n) => n.id === nodeId);
      return (node?.data as TerminalNodeData | undefined)?.label ?? nodeId;
    };
    ensureTerminal(id, {
      agentType,
      command: data.command,
      label: data.label,
      instructions: data.instructions,
      systemPrompt: isClaude
        ? buildAgentSystemPrompt({
            label: data.label,
            roleName: data.roleName,
            instructions: data.instructions,
          })
        : undefined,
      pipes: {
        outgoing: edges.filter((e) => e.source === id && e.type === "agent-pipe").map((e) => labelFor(e.target)),
        incoming: edges.filter((e) => e.target === id && e.type === "agent-pipe").map((e) => labelFor(e.source)),
      },
    });
    attachTerminal(id, parent);

    const observer = new ResizeObserver(() => fitTerminal(id));
    if (parent.parentElement) observer.observe(parent.parentElement);

    return () => {
      observer.disconnect();
      detachTerminal(id);
    };
  }, [id]); // spawn opts são lidos uma vez — mudanças exigem respawn

  // Scheduled prompts
  useEffect(() => {
    if (!data.scheduleCommand?.trim() || !data.scheduleIntervalSecs) return;
    const cmd = data.scheduleCommand.trim();
    const ms = data.scheduleIntervalSecs * 1000;
    scheduleTimerRef.current = setInterval(() => {
      ptyWrite(id, cmd + "\n").catch(console.error);
    }, ms);
    return () => {
      if (scheduleTimerRef.current) clearInterval(scheduleTimerRef.current);
    };
  }, [id, data.scheduleCommand, data.scheduleIntervalSecs]);

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
        toast.error(`Não foi possível abrir o editor (${editorCmd})`);
      }
    },
    []
  );

  return (
    <div
      className="relative flex flex-col w-full h-full rounded-xl overflow-hidden"
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

      {/* Fio de identidade: 1px na cor do agente para escanear o canvas */}
      <div
        className="h-px shrink-0"
        style={{ background: `linear-gradient(90deg, transparent, ${accentColor}, transparent)` }}
      />

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab active:cursor-grabbing select-none"
        style={{ background: "#222", borderBottom: "1px solid #2a2a2a" }}
      >
        <GripVertical size={12} className="text-[#444] shrink-0" />

        <span
          title={statusDot.label}
          className={`shrink-0 rounded-full ${sessionStatus === "running" ? "animate-pulse" : ""}`}
          style={{
            width: 7,
            height: 7,
            background: statusDot.color,
            boxShadow: sessionStatus === "running" ? `0 0 6px ${statusDot.color}` : "none",
          }}
        />

        <Badge color={accentColor} rounded="md" className="text-[10px]">
          {AGENT_ICONS[agentType]}
          {AGENT_LABELS[agentType]}
        </Badge>

        {/* Role badge */}
        {data.roleName && <Badge color={data.roleColor ?? "#888"}>{data.roleName}</Badge>}

        <span className="text-[#555] text-xs truncate flex-1">{data.label}</span>

        {/* Queued messages badge — clique mostra a fila e permite cancelar */}
        {queuePending > 0 && (
          <div className="relative nodrag shrink-0">
            <button
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => { e.stopPropagation(); setShowQueueMenu((v) => !v); }}
              title={`${queuePending} mensagem${queuePending > 1 ? "s" : ""} aguardando entrega — clique para ver`}
              className="flex items-center gap-0.5 text-[9px] font-medium px-1.5 py-0.5 rounded-full"
              style={{ color: "#fbbf24", background: "#fbbf2418", border: "1px solid #fbbf2430" }}
            >
              <Clock size={8} />
              {queuePending}
            </button>
            {showQueueMenu && (
              <div
                className="absolute top-6 right-0 bg-[#1a1a1a] border border-[#2a2a2a] rounded-lg shadow-xl py-1 z-50 w-[260px]"
                onMouseDown={(e) => e.stopPropagation()}
              >
                <div className="px-3 py-1 text-[9px] uppercase tracking-wider text-[#666] select-none">
                  Fila de mensagens
                </div>
                {queueItems.map((item, i) => (
                  <div
                    key={`${i}-${item.msg_id ?? item.msg.slice(0, 12)}`}
                    className="flex items-start gap-2 px-3 py-1.5 text-[10px] hover:bg-[#222]"
                  >
                    <div className="flex-1 min-w-0">
                      <span className="text-[#fbbf24] font-medium">{item.from_label}</span>
                      {item.msg_id && <span className="text-[#666]"> #{item.msg_id}</span>}
                      <div className="truncate text-[#999]" title={item.msg}>{item.msg}</div>
                    </div>
                    <button
                      onClick={() => ptyQueueCancel(id, i).catch(console.error)}
                      title="Cancelar mensagem"
                      className="text-[#555] hover:text-[#f87171] transition-colors shrink-0 mt-0.5"
                    >
                      <X size={10} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Agent pipe badge */}
        {pipeCount > 0 && (
          <Badge color="#a78bfa" title={`Conectado a ${pipeCount} agente${pipeCount > 1 ? "s" : ""} via pipe`}>
            <Plug size={8} />
            {pipeCount}
          </Badge>
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
          <IconButton title="Abrir no editor" intent="info" onClick={() => setShowEditorMenu((v) => !v)}>
            <FolderOpen size={12} />
          </IconButton>
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
        <IconButton title="Fechar terminal" intent="danger" onClick={handleClose}>
          <X size={12} />
        </IconButton>
      </div>

      {/* Terminal body (escondido em zoom baixo — LOD) */}
      <div
        ref={termDivRef}
        className="flex-1 min-h-0 nodrag nowheel"
        style={{
          padding: "4px 2px 2px 4px",
          background: TERMINAL_BG,
          display: lod ? "none" : undefined,
        }}
        onMouseDown={(e) => e.stopPropagation()}
      />

      {/* LOD: card legível no overview (o terminal continua vivo, só sem DOM visível) */}
      {lod && (
        <div
          className="flex-1 min-h-0 flex flex-col items-center justify-center gap-3 select-none"
          style={{ background: TERMINAL_BG }}
        >
          <span style={{ color: accentColor }}>
            {agentType === "shell" ? <Terminal size={56} /> : agentType === "claude" ? <Bot size={56} /> : agentType === "codex" ? <Code2 size={56} /> : <Wrench size={56} />}
          </span>
          <span className="text-3xl font-medium text-ink truncate max-w-[90%]">{data.label}</span>
          <span className="flex items-center gap-2 text-xl" style={{ color: statusDot.color }}>
            <span className="w-3 h-3 rounded-full" style={{ background: statusDot.color }} />
            {statusDot.label}
          </span>
        </div>
      )}

      {/* Processo encerrado: overlay elegante com reinício no lugar */}
      {sessionStatus === "exited" && (
        <div className="absolute inset-x-0 top-9 bottom-0 z-10 flex flex-col items-center justify-center gap-3 bg-black/60 nodrag">
          <span className="text-xs text-ink-muted">
            Processo encerrado{typeof exitCode === "number" ? ` · código ${exitCode}` : ""}
          </span>
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => { e.stopPropagation(); respawnTerminal(id); }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs bg-canvas-tile border border-canvas-border text-ink hover:border-accent hover:text-accent transition-colors"
          >
            <RotateCcw size={12} /> Reiniciar
          </button>
        </div>
      )}

      <Handle type="target" position={Position.Left} style={{ background: accentColor, border: "none", width: 8, height: 8 }} />
      <Handle type="source" position={Position.Right} style={{ background: accentColor, border: "none", width: 8, height: 8 }} />
    </div>
  );
}

// React Flow re-renderiza nós custom a cada mudança de qualquer nó; com memo,
// arrastar um tile não re-renderiza os demais.
export default memo(TerminalTile);

import { memo, useEffect, useRef, useCallback, useState } from "react";
import NodeHandles from "@/components/ui/NodeHandles";
import { NodeResizer, useStore } from "@xyflow/react";
import { X, FolderOpen, Clock, Plug, RotateCcw, Crown, MessageSquare, SquarePen } from "lucide-react";
import { useComposerStore } from "@/stores/composer";
import { useChatStore } from "@/stores/chat";
import { attachTerminal, detachTerminal, ensureTerminal, fitTerminal, respawnTerminal, spawnOptsFor } from "@/lib/terminalManager";
import { presetOf } from "@/lib/agents";
import { useCanvasStore } from "@/stores/canvas";
import { useTerminalsStore } from "@/stores/terminals";
import { useRoutinesStore } from "@/stores/routines";
import type { SessionStatus } from "@/stores/terminals";
import StatusDot, { STATUS_DOT } from "@/components/ui/StatusDot";
import { openInEditor, ptyQueueCancel } from "@/lib/tauri";
import type { QueueItem } from "@/lib/tauri";
import IconButton from "@/components/ui/IconButton";
import { ContextRing } from "@/components/UsageMeter";
import { useUsageStore, formatReset } from "@/stores/usage";
import { toast } from "@/stores/toasts";
import { EDITORS } from "@/lib/editors";
import type { TerminalNodeData } from "@/stores/canvas";
import type { Node, NodeProps } from "@xyflow/react";
import "@xterm/xterm/css/xterm.css";

type TerminalNode = Node<TerminalNodeData, "terminal">;

const TERMINAL_BG = "var(--term)";

// Below this zoom the terminal content is unreadable — swap for a card (LOD)
const LOD_ZOOM = 0.35;

// Stable reference for the queue selector (avoids identity re-renders)
const NO_QUEUE: QueueItem[] = [];

function TerminalTile({ id, data, selected }: NodeProps<TerminalNode>) {
  const termDivRef = useRef<HTMLDivElement>(null);
  const cwdRef = useRef<string>(data.cwd || "~");
  cwdRef.current = data.cwd || "~";
  // Only re-renders when crossing the threshold, not on every zoom tick
  const lod = useStore((s) => s.transform[2] < LOD_ZOOM);
  const removeNode = useCanvasStore((s) => s.removeNode);
  const updateNodeData = useCanvasStore((s) => s.updateNodeData);
  const chatUnread = useChatStore((s) => s.unread[id] ?? 0);
  const chatOpen = useChatStore((s) => s.openTerminal === id);
  const pipeCount = useCanvasStore((s) =>
    s.edges.filter((e) => (e.source === id || e.target === id) && e.type === "agent-pipe").length
  );
  const agentType = data.agentType ?? "shell";
  const preset = presetOf(agentType);
  const accentColor = preset.color;
  const sessionStatus = useTerminalsStore((s) => s.sessions[id]?.status ?? "spawning");
  const exitCode = useTerminalsStore((s) => s.sessions[id]?.exitCode);
  const statusDot = STATUS_DOT[sessionStatus];
  const queueItems = useTerminalsStore((s) => s.queues[id] ?? NO_QUEUE);
  const queuePending = queueItems.length;
  const routines = useRoutinesStore((s) => s.routines);
  const myRoutines = routines.filter((r) => r.target_id === id && r.status === "enabled");
  const sessionUsage = useUsageStore((s) => s.sessions[id]);
  const claudePlan = useUsageStore((s) => (agentType === "claude" ? s.providers.claude : undefined));
  const [showEditorMenu, setShowEditorMenu] = useState(false);
  const [showQueueMenu, setShowQueueMenu] = useState(false);

  // Attaches the managed terminal (xterm/PTY live outside React — unmounting
  // the tile via culling/LOD doesn't kill the process or lose the scrollback)
  useEffect(() => {
    const parent = termDivRef.current;
    if (!parent) return;

    // Normally already started by the canvas store; this covers a tile
    // that mounts first (same opts, idempotent).
    ensureTerminal(id, spawnOptsFor(data));
    attachTerminal(id, parent);

    const observer = new ResizeObserver(() => fitTerminal(id));
    if (parent.parentElement) observer.observe(parent.parentElement);

    return () => {
      observer.disconnect();
      detachTerminal(id);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, data.sessionGen]); // opts are read per session — a swap bumps sessionGen

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
        toast.error(`Could not open the editor (${editorCmd})`);
      }
    },
    []
  );

  return (
    <div
      className="group relative flex flex-col w-full h-full rounded-xl overflow-hidden bg-surface border border-line transition-shadow duration-150"
      style={{ boxShadow: selected ? "var(--ring-accent), var(--shadow-tile)" : "var(--shadow-tile)" }}
    >
      <NodeResizer
        minWidth={360}
        minHeight={220}
        isVisible={selected}
        lineStyle={{ borderColor: "transparent" }}
        handleStyle={{ width: 8, height: 8, borderRadius: 2, border: "1.5px solid var(--accent)", background: "var(--surface)" }}
      />

      {/* Identity thread: 1px in the agent's colour, to scan the canvas */}
      <div
        className="h-px shrink-0 opacity-80"
        style={{ background: `linear-gradient(90deg, transparent 4%, ${accentColor} 50%, transparent 96%)` }}
      />

      {/* Header — integrated with the tile; state always visible, actions on hover */}
      <div className="flex items-center gap-2 h-[34px] pl-3 pr-2 shrink-0 cursor-grab active:cursor-grabbing select-none border-b border-line">
        <StatusDot status={sessionStatus} />

        <span title={preset.label} className="shrink-0 flex" style={{ color: accentColor }}>
          <preset.Icon size={13} />
        </span>

        <span className="text-[12px] font-medium text-ink truncate min-w-0" title={`${data.label} · ${preset.label}`}>
          {data.label}
        </span>

        {/* Role: neutral chip with the role's colour as a dot */}
        {data.roleName && (
          <span className="shrink-0 flex items-center gap-1 h-[18px] px-1.5 rounded-full bg-hover text-2xs text-ink-2">
            <span className="w-1.5 h-1.5 rounded-full" style={{ background: data.roleColor ?? "var(--text-3)" }} />
            {data.roleName}
          </span>
        )}

        <span className="flex-1" />

        {/* Maestro mode: this agent may recruit/dismiss teammates and manage roles */}
        {agentType !== "shell" && (
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => { e.stopPropagation(); updateNodeData(id, { maestro: !data.maestro }); }}
            title={data.maestro
              ? "Maestro — can recruit and dismiss teammates and manage roles (click to turn off)"
              : "Turn on Maestro mode (lets this agent recruit teammates)"}
            className={`nodrag shrink-0 flex items-center gap-1 h-[18px] px-1.5 rounded-full text-2xs font-medium transition-all duration-150 ${
              data.maestro
                ? "text-queue bg-queue/10 border border-queue/25"
                : "text-ink-muted hover:text-ink border border-transparent opacity-0 group-hover:opacity-100"
            }`}
          >
            <Crown size={10} />
            {data.maestro && "Maestro"}
          </button>
        )}

        {/* Queued messages — click shows the queue and allows cancelling */}
        {queuePending > 0 && (
          <div className="relative nodrag shrink-0">
            <button
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => { e.stopPropagation(); setShowQueueMenu((v) => !v); }}
              title={`${queuePending} message${queuePending > 1 ? "s" : ""} awaiting delivery — click to view`}
              className="flex items-center gap-1 h-[18px] px-1.5 rounded-full text-2xs font-medium tabular-nums text-queue bg-queue/10 border border-queue/25"
            >
              <Clock size={10} />
              {queuePending}
            </button>
            {showQueueMenu && (
              <div
                className="absolute top-7 right-0 bg-raised border border-line rounded-lg shadow-float py-1 z-50 w-[280px]"
                onMouseDown={(e) => e.stopPropagation()}
              >
                <div className="px-3 pt-1 pb-1.5 text-2xs uppercase tracking-[0.06em] text-ink-muted select-none">
                  Message queue
                </div>
                {queueItems.map((item, i) => (
                  <div
                    key={`${i}-${item.msg.slice(0, 12)}`}
                    className="flex items-start gap-2 px-3 py-1.5 text-[12px] hover:bg-hover"
                  >
                    <div className="flex-1 min-w-0">
                      <span className="text-ink font-medium">{item.from_label}</span>
                      <div className="truncate text-ink-2" title={item.msg}>{item.msg}</div>
                    </div>
                    <button
                      onClick={() => ptyQueueCancel(id, i).catch(console.error)}
                      title="Cancel message"
                      className="text-ink-muted hover:text-danger-text transition-colors shrink-0 mt-0.5"
                    >
                      <X size={12} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Routines firing into this terminal (Routines panel in the sidebar) */}
        {myRoutines.length > 0 && (
          <span
            title={myRoutines.map((r) => `${r.name} — ${r.schedule}${r.next_fire ? ` · next ${r.next_fire}` : ""}`).join("\n")}
            className="flex items-center gap-0.5 text-2xs tabular-nums text-ink-muted shrink-0"
          >
            <Clock size={11} />
            {myRoutines.length}
          </span>
        )}

        {/* Context window of this session (Claude status line → usage.rs) */}
        {sessionUsage?.context_percent != null && (
          <ContextRing
            pct={sessionUsage.context_percent}
            title={[
              `Context ${Math.round(sessionUsage.context_percent)}% used`,
              sessionUsage.model,
              sessionUsage.cost_usd != null ? `$${sessionUsage.cost_usd.toFixed(2)} this session (estimate)` : null,
              claudePlan?.five_hour
                ? `Plan · 5h ${Math.round(claudePlan.five_hour.used_percent)}%${claudePlan.five_hour.resets_at ? ` (resets ${formatReset(claudePlan.five_hour.resets_at)})` : ""}`
                : null,
              claudePlan?.seven_day ? `Plan · 7d ${Math.round(claudePlan.seven_day.used_percent)}%` : null,
            ].filter(Boolean).join("\n")}
          />
        )}

        {/* NarraTer Chat (agents answer with `narrater say`) — unread stays visible */}
        {agentType !== "shell" && (
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              const chat = useChatStore.getState();
              if (chatOpen) chat.close();
              else chat.open(id);
            }}
            title={chatUnread ? `${chatUnread} new chat repl${chatUnread === 1 ? "y" : "ies"}` : "Chat with this agent"}
            className={`relative nodrag shrink-0 p-1 rounded-md transition-all duration-150 hover:bg-hover ${
              chatOpen ? "text-accent-text" : chatUnread ? "text-ink" : "text-ink-muted hover:text-ink opacity-0 group-hover:opacity-100"
            }`}
          >
            <MessageSquare size={13} />
            {chatUnread > 0 && (
              <span className="absolute -top-0.5 -right-0.5 min-w-[14px] h-[14px] px-1 rounded-full bg-accent text-[10px] leading-[14px] font-medium text-white text-center tabular-nums">
                {chatUnread}
              </span>
            )}
          </button>
        )}

        {/* Actions — quiet until the tile is hovered */}
        <div className="relative nodrag flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity duration-150">
          {pipeCount > 0 && (
            <span
              title={`Connected to ${pipeCount} agent${pipeCount > 1 ? "s" : ""}`}
              className="flex items-center gap-0.5 px-1 text-2xs tabular-nums text-ink-muted"
            >
              <Plug size={11} />
              {pipeCount}
            </span>
          )}
          <IconButton
            title="Write a prompt (Ctrl+Shift+P) — @ mentions, pasted screenshots, a draft per terminal"
            onClick={() => useComposerStore.getState().open(id)}
          >
            <SquarePen size={13} />
          </IconButton>
          <IconButton title="Open in editor" onClick={() => setShowEditorMenu((v) => !v)}>
            <FolderOpen size={13} />
          </IconButton>
          {showEditorMenu && (
            <div
              className="absolute top-7 right-0 bg-raised border border-line rounded-lg shadow-float py-1 z-50 min-w-[130px]"
              onMouseDown={(e) => e.stopPropagation()}
            >
              {EDITORS.map((ed) => (
                <button
                  key={ed.cmd}
                  onClick={() => handleOpenEditor(ed.cmd)}
                  className="flex items-center gap-2 w-full px-3 py-1.5 text-[13px] text-ink-2 hover:bg-hover hover:text-ink transition-colors whitespace-nowrap"
                >
                  {ed.label}
                </button>
              ))}
            </div>
          )}
          <IconButton title="Close terminal" intent="danger" onClick={handleClose}>
            <X size={13} />
          </IconButton>
        </div>
      </div>

      {/* Terminal body (hidden at low zoom — LOD) */}
      <div
        ref={termDivRef}
        className="flex-1 min-h-0 nodrag nowheel"
        style={{
          padding: "8px 4px 4px 10px",
          background: TERMINAL_BG,
          display: lod ? "none" : undefined,
        }}
        onMouseDown={(e) => e.stopPropagation()}
      />

      {/* LOD: readable card in the overview (the terminal stays alive, just without visible DOM) */}
      {lod && (
        <div
          className="flex-1 min-h-0 flex flex-col items-center justify-center gap-3 select-none"
          style={{ background: TERMINAL_BG }}
        >
          <span style={{ color: accentColor }}>
            <preset.Icon size={56} />
          </span>
          <span className="text-3xl font-semibold tracking-tight text-ink truncate max-w-[90%]">{data.label}</span>
          <span className="flex items-center gap-2 text-xl" style={{ color: statusDot.color }}>
            <span className="w-3 h-3 rounded-full" style={{ background: statusDot.color }} />
            {statusDot.label}
          </span>
        </div>
      )}

      {/* Process exited: elegant overlay with in-place restart */}
      {sessionStatus === "exited" && (
        <div className="absolute inset-x-0 top-[35px] bottom-0 z-10 flex flex-col items-center justify-center gap-3 bg-bg/70 backdrop-blur-sm nodrag">
          <span className="text-xs text-ink-muted">
            Process exited{typeof exitCode === "number" ? ` · code ${exitCode}` : ""}
          </span>
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => { e.stopPropagation(); respawnTerminal(id); }}
            className="flex items-center gap-1.5 h-8 px-3.5 rounded-lg text-[13px] font-medium bg-surface-2 border border-line text-ink hover:bg-raised hover:border-line-strong transition-colors"
          >
            <RotateCcw size={12} /> Restart
          </button>
        </div>
      )}
      {/* Handles on all four sides, hidden until hover (D18) */}
      <NodeHandles />
    </div>
  );
}

// React Flow re-renders custom nodes on every change to any node; with memo,
// dragging one tile doesn't re-render the others.
export default memo(TerminalTile);

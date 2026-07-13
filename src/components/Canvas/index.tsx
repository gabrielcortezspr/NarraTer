import { lazy, Suspense, useCallback, useState, useEffect, useRef } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  useReactFlow,
  type OnConnect,
  type NodeTypes,
  type EdgeTypes,
  type Connection,
  type XYPosition,
} from "@xyflow/react";
import { useShallow } from "zustand/react/shallow";
import { onPtyOutput } from "@/lib/ptyBus";
import { scheduleTerminalZoomSync } from "@/lib/terminalManager";
import "@xyflow/react/dist/style.css";
import TerminalTile from "@/components/TerminalTile";
import NoteTile from "@/components/NoteTile";
import TextTile from "@/components/TextTile";
import FileTreeTile from "@/components/FileTreeTile";
import AttachmentTile from "@/components/AttachmentTile";
import PortalTile from "@/components/PortalTile";
import AgentNoteEdge from "@/components/AgentNoteEdge";
import AgentPipeEdge from "@/components/AgentPipeEdge";
import CommandPalette from "@/components/CommandPalette";
import EdgeHistoryPanel from "@/components/EdgeHistoryPanel";
import EmptyState from "@/components/EmptyState";

const AgentPicker = lazy(() => import("@/components/AgentPicker"));
import SketchLayer from "@/components/SketchLayer";
import Toolbar from "@/components/Toolbar";
import { useCanvasStore } from "@/stores/canvas";
import { useLedgerStore } from "@/stores/ledger";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useToolStore, PLACEMENT_TOOLS } from "@/stores/tool";
import { saveNow } from "@/hooks/useAutoSave";
import { useSketchStore } from "@/stores/sketch";
import { stripAnsi, cleanLines } from "@/lib/ansi";
import { pickFile } from "@/lib/tauri";
import { printToTerminal } from "@/lib/terminalManager";
import type { AgentPickerResult } from "@/components/AgentPicker";
import type { AppEdge, TerminalNodeData } from "@/stores/canvas";
import type { Tool } from "@/stores/tool";

const nodeTypes = {
  terminal: TerminalTile,
  note: NoteTile,
  text: TextTile,
  filetree: FileTreeTile,
  attachment: AttachmentTile,
  portal: PortalTile,
} satisfies NodeTypes;

const edgeTypes = {
  "agent-note": AgentNoteEdge,
  "agent-pipe": AgentPipeEdge,
} satisfies EdgeTypes;

// Modifier-free tool shortcuts, Figma style.
const TOOL_SHORTCUTS: Record<string, Tool> = {
  v: "select",
  t: "terminal",
  n: "note",
  x: "text",
  f: "files",
  a: "attachment",
  w: "portal",
  d: "draw",
};

export default function Canvas() {
  return (
    <ReactFlowProvider>
      <CanvasInner />
    </ReactFlowProvider>
  );
}

function CanvasInner() {
  const { nodes, edges, onNodesChange, onEdgesChange, addEdge: addStoreEdge, addTerminalNode, addNoteNode, addTextNode, addFileTreeNode, addAttachmentNode, addPortalNode } =
    useCanvasStore(
      useShallow((s) => ({
        nodes: s.nodes,
        edges: s.edges,
        onNodesChange: s.onNodesChange,
        onEdgesChange: s.onEdgesChange,
        addEdge: s.addEdge,
        addTerminalNode: s.addTerminalNode,
        addNoteNode: s.addNoteNode,
        addTextNode: s.addTextNode,
        addFileTreeNode: s.addFileTreeNode,
        addAttachmentNode: s.addAttachmentNode,
        addPortalNode: s.addPortalNode,
      }))
    );
  const hydrated = useCanvasStore((s) => s.hydrated);
  const { current: currentWorkspace } = useWorkspacesStore();
  const { active: activeTool, setTool } = useToolStore();
  const undoSketch = useSketchStore((s) => s.undo);
  const { screenToFlowPosition, fitView } = useReactFlow();

  const drawMode = activeTool === "draw";
  const placementMode = PLACEMENT_TOOLS.has(activeTool);

  const wrapperRef = useRef<HTMLDivElement>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);

  // Moves are undoable: snapshot the state when the drag starts
  const onNodeDragStart = useCallback(() => {
    useCanvasStore.getState().snapshot();
  }, []);

  // Frame the canvas once per scene load — a permanent fitView prop would
  // re-frame on every structural node change instead.
  const fitDoneFor = useRef<string | null>(null);
  useEffect(() => {
    if (hydrated && fitDoneFor.current !== currentWorkspace) {
      fitDoneFor.current = currentWorkspace;
      requestAnimationFrame(() => fitView({ padding: 0.2 }));
    }
  }, [hydrated, currentWorkspace, fitView]);

  // New nodes spawn at the center of the current viewport, slightly jittered
  // so consecutive spawns don't stack exactly.
  const viewportCenter = useCallback((): XYPosition => {
    const rect = wrapperRef.current?.getBoundingClientRect();
    const center = rect
      ? { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
      : { x: window.innerWidth / 2, y: window.innerHeight / 2 };
    const pos = screenToFlowPosition(center);
    return { x: pos.x + (Math.random() - 0.5) * 80, y: pos.y + (Math.random() - 0.5) * 80 };
  }, [screenToFlowPosition]);

  // Placement tools: next click on the pane creates the node there.
  const onPaneClick = useCallback(
    (e: React.MouseEvent) => {
      const tool = useToolStore.getState().active;
      if (!PLACEMENT_TOOLS.has(tool)) return;
      const position = screenToFlowPosition({ x: e.clientX, y: e.clientY });
      switch (tool) {
        case "note":
          addNoteNode(position);
          break;
        case "text":
          addTextNode(position);
          break;
        case "files":
          addFileTreeNode(position);
          break;
        case "portal":
          addPortalNode(position);
          break;
      }
      // Shift keeps the tool active for serial creation
      if (!e.shiftKey) setTool("select");
    },
    [screenToFlowPosition, addNoteNode, addTextNode, addFileTreeNode, addPortalNode, setTool]
  );

  // Clicking an agent-pipe edge opens that conversation's history
  const onEdgeClick = useCallback((_: React.MouseEvent, edge: AppEdge) => {
    if (edge.type === "agent-pipe") {
      useLedgerStore.getState().setOpenEdge({ source: edge.source, target: edge.target });
    }
  }, []);

  // Agent → Note pipe: mirrors terminal output into connected notes.
  // Chunks are buffered per note and applied in batches (~100ms) — a verbose
  // agent emits dozens of chunks per second, and each appendNoteContent is a
  // global setState (item 1.4 of the frontend plan).
  useEffect(() => {
    const buffers = new Map<string, string>();
    let flushTimer: number | undefined;

    const flush = () => {
      flushTimer = undefined;
      const { appendNoteContent } = useCanvasStore.getState();
      buffers.forEach((text, noteId) => appendNoteContent(noteId, text));
      buffers.clear();
    };

    const unlisten = onPtyOutput("*", ({ id: termId, data: rawData }) => {
      const currentEdges = useCanvasStore.getState().edges;
      const agentNoteEdges = currentEdges.filter(
        (e) => e.type === "agent-note" && (e.source === termId || e.target === termId)
      );
      if (!agentNoteEdges.length) return;

      const clean = cleanLines(stripAnsi(rawData));
      if (!clean) return;

      agentNoteEdges.forEach((edge) => {
        const noteId = edge.source === termId ? edge.target : edge.source;
        const pending = buffers.get(noteId);
        buffers.set(noteId, pending ? `${pending}\n${clean}` : clean);
      });
      flushTimer ??= window.setTimeout(flush, 100);
    });

    return () => {
      unlisten();
      if (flushTimer !== undefined) window.clearTimeout(flushTimer);
    };
  }, []);

  const onConnect: OnConnect = useCallback(
    (connection: Connection) => {
      // Same guards as the MCP connect_nodes: no self-loops, and an existing
      // route is not duplicated (each duplicate snapshots + resyncs for nothing).
      if (!connection.source || !connection.target || connection.source === connection.target) return;
      const { nodes: currentNodes, edges: currentEdges } = useCanvasStore.getState();
      if (currentEdges.some((e) => e.source === connection.source && e.target === connection.target)) return;

      const sourceNode = currentNodes.find((n) => n.id === connection.source);
      const targetNode = currentNodes.find((n) => n.id === connection.target);

      const isAgentNote =
        (sourceNode?.type === "terminal" && targetNode?.type === "note") ||
        (sourceNode?.type === "note" && targetNode?.type === "terminal");

      const isAgentPipe =
        sourceNode?.type === "terminal" && targetNode?.type === "terminal";

      let edgeType = "default";
      if (isAgentNote) edgeType = "agent-note";
      else if (isAgentPipe) edgeType = "agent-pipe";

      const newEdge: AppEdge = {
        id: `edge-${crypto.randomUUID().slice(0, 8)}`,
        source: connection.source,
        target: connection.target,
        sourceHandle: connection.sourceHandle ?? undefined,
        targetHandle: connection.targetHandle ?? undefined,
        type: edgeType,
        animated: edgeType === "default",
        style: edgeType === "default" ? { stroke: "#4a4a4a", strokeWidth: 1.5 } : undefined,
      };

      // Store is the source of truth: it mirrors the route to the backend
      addStoreEdge(newEdge);

      // Creating the route only updates the backend table (syncConnections,
      // token-free). AI agents are deliberately NOT notified: a queued system
      // message auto-submits and burns a turn on each endpoint per edge. They
      // verify routes when a prompt actually flows — list_peers on the sender
      // side, the [narrater from X] frame on the receiver side. Shells get a
      // hint on screen only (never stdin — the shell would execute it).
      if (isAgentPipe) {
        const srcLabel = (sourceNode?.data as TerminalNodeData | undefined)?.label ?? connection.source;
        const tgtLabel = (targetNode?.data as TerminalNodeData | undefined)?.label ?? connection.target;
        const srcType = (sourceNode?.data as TerminalNodeData | undefined)?.agentType ?? "shell";
        const tgtType = (targetNode?.data as TerminalNodeData | undefined)?.agentType ?? "shell";
        const isAi = (t: string) => t === "claude" || t === "codex";

        if (!isAi(srcType)) {
          printToTerminal(
            connection.source,
            `\r\n\x1b[35m[NarraTer]\x1b[0m Connected \x1b[1m→ "${tgtLabel}"\x1b[0m\r\n` +
              `Use: \x1b[36mnarrater send "${tgtLabel}" "message"\x1b[0m or \x1b[36mnarrater ask "${tgtLabel}" "question"\x1b[0m\r\n\r\n`
          );
        }

        if (!isAi(tgtType)) {
          printToTerminal(
            connection.target,
            `\r\n\x1b[35m[NarraTer]\x1b[0m Agent \x1b[1m"${srcLabel}" →\x1b[0m connected to you.\r\n\r\n`
          );
        }
      }
    },
    [addStoreEdge]
  );

  const handleAgentPicked = useCallback(
    (result: AgentPickerResult) => {
      addTerminalNode(result, viewportCenter());
      setPickerOpen(false);
    },
    [addTerminalNode, viewportCenter]
  );

  const openTerminalPicker = useCallback(() => setPickerOpen(true), []);

  // Attachment: native file picker → node at the viewport center
  const handleAttachment = useCallback(() => {
    pickFile()
      .then((path) => {
        if (path) addAttachmentNode(viewportCenter(), path);
      })
      .catch(console.error);
  }, [addAttachmentNode, viewportCenter]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable ||
        target.closest?.(".xterm")
      )
        return;

      if (e.ctrlKey || e.metaKey) {
        const key = e.key.toLowerCase();
        if (key === "s") { e.preventDefault(); saveNow().catch(console.error); }
        else if (key === "k") { e.preventDefault(); setPaletteOpen((v) => !v); }
        else if (key === "z" && useToolStore.getState().active === "draw") { e.preventDefault(); undoSketch(); }
        else if (key === "z" && e.shiftKey) { e.preventDefault(); useCanvasStore.getState().redo(); }
        else if (key === "z") { e.preventDefault(); useCanvasStore.getState().undo(); }
        else if (key === "y") { e.preventDefault(); useCanvasStore.getState().redo(); }
        return;
      }

      if (e.key === "Escape") {
        setTool("select");
        setPickerOpen(false);
        return;
      }

      const tool = TOOL_SHORTCUTS[e.key.toLowerCase()];
      if (!tool) return;
      e.preventDefault();
      if (tool === "terminal") setPickerOpen(true);
      else if (tool === "attachment") handleAttachment();
      else if (tool === "draw") setTool(useToolStore.getState().active === "draw" ? "select" : "draw");
      else setTool(tool);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [setTool, undoSketch, handleAttachment]);

  return (
    <div
      ref={wrapperRef}
      className={`w-full h-full relative ${placementMode ? "placement-mode" : ""}`}
    >
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onPaneClick={onPaneClick}
        onEdgeClick={onEdgeClick}
        onNodeDragStart={onNodeDragStart}
        onMove={(_, viewport) => scheduleTerminalZoomSync(viewport.zoom)}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        minZoom={0.05}
        maxZoom={3}
        onlyRenderVisibleElements
        deleteKeyCode={drawMode ? null : "Delete"}
        panOnDrag={!drawMode}
        zoomOnScroll={!drawMode}
        nodesDraggable={!drawMode}
        nodesConnectable={!drawMode}
        className="bg-canvas-bg"
        proOptions={{ hideAttribution: true }}
      >
        <Background variant={BackgroundVariant.Dots} gap={24} size={1} color="#222" />
        <Controls showInteractive={false} style={{ left: 16, bottom: 16 }} />
        <MiniMap
          nodeColor={(node) => {
            if (node.type === "note") return "#fbbf2440";
            if (node.type === "text") return "#e5e7eb30";
            if (node.type === "filetree") return "#60a5fa40";
            if (node.type === "attachment") return "#f472b640";
            if (node.type === "portal") return "#22d3ee40";
            const t = (node.data as { agentType?: string })?.agentType ?? "shell";
            return { shell: "#6b728050", claude: "#8b5cf650", codex: "#3b82f650", custom: "#14b8a650" }[t] ?? "#6b728050";
          }}
          maskColor="rgba(0,0,0,0.6)"
          style={{ right: 16, bottom: 16 }}
        />

        {/* Sketch layer — inside ReactFlow context so useViewport() works */}
        <SketchLayer active={drawMode} />
      </ReactFlow>

      <Toolbar onTerminal={openTerminalPicker} onAttachment={handleAttachment} />

      {/* Conversation history for the clicked agent-pipe edge */}
      <EdgeHistoryPanel />

      {/* Welcome screen when the canvas is empty */}
      <EmptyState onTerminal={openTerminalPicker} />

      <div className="absolute top-0 left-0 h-10 flex items-center px-4 z-10 pointer-events-none">
        <span className="text-[#2a2a2a] text-xs select-none">{currentWorkspace}</span>
      </div>

      {drawMode && (
        <div className="absolute bottom-20 left-1/2 -translate-x-1/2 z-30 pointer-events-none">
          <div className="bg-[#1a1a1a] border border-[#8b5cf640] text-[#8b5cf6] text-xs px-3 py-1.5 rounded-full">
            Draw mode active — Esc to exit
          </div>
        </div>
      )}

      {placementMode && (
        <div className="absolute bottom-20 left-1/2 -translate-x-1/2 z-30 pointer-events-none">
          <div className="bg-[#1a1a1a] border border-[#8b5cf640] text-[#8b5cf6] text-xs px-3 py-1.5 rounded-full">
            Click the canvas to place — Shift keeps the tool, Esc cancels
          </div>
        </div>
      )}

      {/* Lazy modals: only enter the bundle/DOM when opened */}
      {pickerOpen && (
        <Suspense fallback={null}>
          <AgentPicker open={pickerOpen} onConfirm={handleAgentPicked} onClose={() => setPickerOpen(false)} />
        </Suspense>
      )}

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onNewTerminal={() => setPickerOpen(true)}
      />
    </div>
  );
}

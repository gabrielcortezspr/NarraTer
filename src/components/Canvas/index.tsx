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
import { pickFile } from "@/lib/tauri";
import { presetOf } from "@/lib/agents";
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

  const onConnect: OnConnect = useCallback(
    (connection: Connection) => {
      // Same guards as `narrater connect`: no self-loops, and an existing
      // connection (either direction) is not duplicated.
      if (!connection.source || !connection.target || connection.source === connection.target) return;
      const { nodes: currentNodes, edges: currentEdges } = useCanvasStore.getState();
      if (currentEdges.some((e) =>
        (e.source === connection.source && e.target === connection.target) ||
        (e.source === connection.target && e.target === connection.source)
      )) return;

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

      // Creating the connection only updates the backend table — agents are
      // not prompted (that would burn a turn on each side). They find their
      // teammates with `narrater list` when a prompt actually flows. Shells
      // get a hint on screen only (never stdin — the shell would execute it).
      if (isAgentPipe) {
        const label = (n: typeof sourceNode, fallback: string) =>
          (n?.data as TerminalNodeData | undefined)?.label ?? fallback;
        const isShell = (n: typeof sourceNode) =>
          ((n?.data as TerminalNodeData | undefined)?.agentType ?? "shell") === "shell";
        const ends: Array<[string, string]> = [
          [connection.source, label(targetNode, connection.target)],
          [connection.target, label(sourceNode, connection.source)],
        ];
        ends.forEach(([termId, peer], i) => {
          if (!isShell(i === 0 ? sourceNode : targetNode)) return;
          printToTerminal(
            termId,
            `\r\n\x1b[35m[NarraTer]\x1b[0m Connected to \x1b[1m"${peer}"\x1b[0m — ` +
              `\x1b[36mnarrater ask "${peer}" "..."\x1b[0m\r\n\r\n`
          );
        });
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
        target.closest?.(".xterm") ||
        target.closest?.(".portal-surface")
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
            return `${presetOf((node.data as { agentType?: string })?.agentType).color}50`;
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

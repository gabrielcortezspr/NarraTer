import { useCallback, useState, useEffect, useRef } from "react";
import {
  ReactFlow,
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  addEdge,
  useNodesState,
  useEdgesState,
  type OnConnect,
  type NodeTypes,
  type EdgeTypes,
  type NodeChange,
  type EdgeChange,
  type Connection,
} from "@xyflow/react";
import { listen } from "@tauri-apps/api/event";
import "@xyflow/react/dist/style.css";
import { Plus, StickyNote, Save, Pencil, Eraser, Undo2 } from "lucide-react";
import { motion } from "framer-motion";
import TerminalTile from "@/components/TerminalTile";
import NoteTile from "@/components/NoteTile";
import AgentNoteEdge from "@/components/AgentNoteEdge";
import AgentPipeEdge from "@/components/AgentPipeEdge";
import AgentPicker from "@/components/AgentPicker";
import SketchLayer from "@/components/SketchLayer";
import { useCanvasStore } from "@/stores/canvas";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useSketchStore } from "@/stores/sketch";
import { stripAnsi, cleanLines } from "@/lib/ansi";
import { ptyWrite, ptyNotify } from "@/lib/tauri";
import type { AgentType, HistoriaEdge as _HistoriaEdge } from "@/lib/tauri";
import type { AppNode, AppEdge, NoteNodeData, TerminalNodeData } from "@/stores/canvas";

const nodeTypes = {
  terminal: TerminalTile,
  note: NoteTile,
} satisfies NodeTypes;

const edgeTypes = {
  "agent-note": AgentNoteEdge,
  "agent-pipe": AgentPipeEdge,
} satisfies EdgeTypes;

const SKETCH_COLORS = ["#8b5cf6", "#f87171", "#4ade80", "#fbbf24", "#60a5fa", "#f472b6", "#ffffff"];

let edgeIdCounter = 0;

export default function Canvas() {
  const {
    nodes: storeNodes,
    edges: storeEdges,
    setNodes: setStoreNodes,
    setEdges: setStoreEdges,
    addEdge: addStoreEdge,
    removeEdges: removeStoreEdges,
    addTerminalNode,
    addNoteNode,
    saveHistoria,
  } = useCanvasStore();
  const { current: currentWorkspace } = useWorkspacesStore();
  const { undo: undoSketch, clear: clearSketch, color, setColor, size, setSize } = useSketchStore();

  const [nodes, setNodes, onNodesChange] = useNodesState<AppNode>(storeNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<AppEdge>(storeEdges);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [drawMode, setDrawMode] = useState(false);
  const [showColorPicker, setShowColorPicker] = useState(false);
  const prevWorkspace = useRef(currentWorkspace);

  // Refs for use in event callbacks without re-subscribing
  const nodesRef = useRef(nodes);
  const edgesRef = useRef(edges);
  useEffect(() => { nodesRef.current = nodes; }, [nodes]);
  useEffect(() => { edgesRef.current = edges; }, [edges]);

  // Sync store → local when workspace changes
  useEffect(() => {
    if (prevWorkspace.current !== currentWorkspace) {
      prevWorkspace.current = currentWorkspace;
      setNodes(useCanvasStore.getState().nodes as AppNode[]);
      setEdges(useCanvasStore.getState().edges as AppEdge[]);
      clearSketch();
    }
  }, [currentWorkspace, setNodes, setEdges, clearSketch]);

  useEffect(() => { setNodes(storeNodes as AppNode[]); }, [storeNodes, setNodes]);
  useEffect(() => { setEdges(storeEdges as AppEdge[]); }, [storeEdges, setEdges]);

  // Agent → Note pipe: listen to all PTY output and forward to connected notes
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    listen<{ id: string; data: string }>("pty_output", (event) => {
      const { id: termId, data: rawData } = event.payload;
      const currentEdges = edgesRef.current;
      const currentNodes = nodesRef.current;

      // Find agent-note edges where this terminal is source or target
      const agentNoteEdges = currentEdges.filter(
        (e) => e.type === "agent-note" && (e.source === termId || e.target === termId)
      );
      if (!agentNoteEdges.length) return;

      const clean = cleanLines(stripAnsi(rawData));
      if (!clean) return;

      agentNoteEdges.forEach((edge) => {
        const noteId = edge.source === termId ? edge.target : edge.source;
        setNodes((nds) =>
          nds.map((n) => {
            if (n.id !== noteId || n.type !== "note") return n;
            const current = (n.data as NoteNodeData).content ?? "";
            const separator = current.length > 0 ? "\n" : "";
            return {
              ...n,
              data: {
                ...n.data,
                content: current + separator + clean,
                isAgentLive: true,
              },
            };
          })
        );
      });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [setNodes]);

  const handleNodesChange = useCallback((changes: NodeChange<AppNode>[]) => onNodesChange(changes), [onNodesChange]);

  const handleEdgesChange = useCallback(
    (changes: EdgeChange<AppEdge>[]) => {
      onEdgesChange(changes);
      // Mirror deletions to the store so the backend routing table updates
      const removedIds = changes.filter((c) => c.type === "remove").map((c) => c.id);
      if (removedIds.length > 0) removeStoreEdges(removedIds);
    },
    [onEdgesChange, removeStoreEdges]
  );

  const onConnect: OnConnect = useCallback(
    (connection: Connection) => {
      const sourceNode = nodesRef.current.find((n) => n.id === connection.source);
      const targetNode = nodesRef.current.find((n) => n.id === connection.target);

      const isAgentNote =
        (sourceNode?.type === "terminal" && targetNode?.type === "note") ||
        (sourceNode?.type === "note" && targetNode?.type === "terminal");

      const isAgentPipe =
        sourceNode?.type === "terminal" && targetNode?.type === "terminal";

      let edgeType = "default";
      if (isAgentNote) edgeType = "agent-note";
      else if (isAgentPipe) edgeType = "agent-pipe";

      const newEdge: AppEdge = {
        id: `edge-${Date.now()}-${edgeIdCounter++}`,
        source: connection.source,
        target: connection.target,
        sourceHandle: connection.sourceHandle ?? undefined,
        targetHandle: connection.targetHandle ?? undefined,
        type: edgeType,
        animated: edgeType === "default",
        style: edgeType === "default" ? { stroke: "#4a4a4a", strokeWidth: 1.5 } : undefined,
      };

      // Store is the source of truth: it mirrors the route to the backend and
      // the store→local effect brings the edge into React Flow state
      addStoreEdge(newEdge);

      // Tell both endpoints about the new route. AI agents get a queued
      // system message (auto-submitted, becomes part of the conversation);
      // shells/custom get a visual hint only — queued delivery would execute
      // the text as a command. The route is directed: source → target.
      if (isAgentPipe && connection.source && connection.target) {
        const srcLabel = (sourceNode?.data as TerminalNodeData | undefined)?.label ?? connection.source;
        const tgtLabel = (targetNode?.data as TerminalNodeData | undefined)?.label ?? connection.target;
        const srcType = (sourceNode?.data as TerminalNodeData | undefined)?.agentType ?? "shell";
        const tgtType = (targetNode?.data as TerminalNodeData | undefined)?.agentType ?? "shell";
        const isAi = (t: string) => t === "claude" || t === "codex";

        if (isAi(srcType)) {
          ptyNotify(
            connection.source,
            `Novo agente conectado: você → "${tgtLabel}". Você pode enviar mensagens a ele com send_message/ask_agent (to: "${tgtLabel}").`
          ).catch(console.error);
        } else {
          ptyWrite(
            connection.source,
            `\r\n\x1b[35m[NarraTer]\x1b[0m Conectado \x1b[1m→ "${tgtLabel}"\x1b[0m\r\n` +
              `Use: \x1b[36mnarrater send "${tgtLabel}" "mensagem"\x1b[0m ou \x1b[36mnarrater ask "${tgtLabel}" "pergunta"\x1b[0m\r\n\r\n`
          ).catch(console.error);
        }

        if (isAi(tgtType)) {
          ptyNotify(
            connection.target,
            `Novo agente conectado: "${srcLabel}" → você. Mensagens dele chegarão como [narrater de ${srcLabel}].`
          ).catch(console.error);
        } else {
          ptyWrite(
            connection.target,
            `\r\n\x1b[35m[NarraTer]\x1b[0m Agente \x1b[1m"${srcLabel}" →\x1b[0m conectado a você.\r\n\r\n`
          ).catch(console.error);
        }
      }
    },
    [addStoreEdge]
  );

  const handleAgentPicked = useCallback(
    (
      agentType: AgentType,
      command?: string,
      instructions?: string,
      scheduleCommand?: string,
      scheduleIntervalSecs?: number,
      roleId?: string,
      roleName?: string,
      roleColor?: string,
    ) => {
      addTerminalNode(agentType, command, instructions, scheduleCommand, scheduleIntervalSecs, roleId, roleName, roleColor);
      setNodes(useCanvasStore.getState().nodes as AppNode[]);
      setPickerOpen(false);
    },
    [addTerminalNode, setNodes]
  );

  const handleAddNote = useCallback(() => {
    addNoteNode();
    setNodes(useCanvasStore.getState().nodes as AppNode[]);
  }, [addNoteNode, setNodes]);

  const handleSave = useCallback(async () => {
    setSaving(true);
    setStoreNodes(nodes);
    setStoreEdges(edges);
    await saveHistoria(currentWorkspace);
    setTimeout(() => setSaving(false), 800);
  }, [nodes, edges, setStoreNodes, setStoreEdges, saveHistoria, currentWorkspace]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.ctrlKey || e.metaKey) {
        if (e.key === "s") { e.preventDefault(); handleSave(); }
        if (e.key === "t") { e.preventDefault(); setPickerOpen(true); }
        if (e.key === "n") { e.preventDefault(); handleAddNote(); }
        if (e.key === "d") { e.preventDefault(); setDrawMode((v) => !v); }
        if (e.key === "z" && drawMode) { e.preventDefault(); undoSketch(); }
      }
      if (e.key === "Escape") {
        setDrawMode(false);
        setPickerOpen(false);
        setShowColorPicker(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [handleSave, handleAddNote, drawMode, undoSketch]);

  return (
    <div className="w-full h-full relative">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={onConnect}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.05}
        maxZoom={3}
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
            const t = (node.data as { agentType?: string })?.agentType ?? "shell";
            return { shell: "#6b728050", claude: "#8b5cf650", codex: "#3b82f650", custom: "#14b8a650" }[t] ?? "#6b728050";
          }}
          maskColor="rgba(0,0,0,0.6)"
          style={{ right: 16, bottom: 16 }}
        />

        {/* Sketch layer — inside ReactFlow context so useViewport() works */}
        <SketchLayer active={drawMode} />
      </ReactFlow>

      {/* Toolbar */}
      <div className="absolute top-4 right-4 flex items-center gap-2 z-30">
        <motion.button
          onClick={handleSave}
          whileTap={{ scale: 0.95 }}
          title="Ctrl+S"
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all border"
          style={{
            background: saving ? "#4ade8020" : "#1a1a1a",
            borderColor: saving ? "#4ade80" : "#2a2a2a",
            color: saving ? "#4ade80" : "#6b7280",
          }}
        >
          <Save size={13} />
          {saving ? "Salvo" : "Salvar"}
        </motion.button>

        <motion.button
          onClick={handleAddNote}
          whileTap={{ scale: 0.95 }}
          title="Ctrl+N"
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium
            bg-[#1e1a0e] border border-[#2e2810] text-[#fbbf24] hover:border-[#fbbf2460] transition-all"
        >
          <StickyNote size={13} />
          Nota
        </motion.button>

        <div className="flex items-center gap-1">
          {drawMode && (
            <motion.div
              initial={{ opacity: 0, x: 8 }}
              animate={{ opacity: 1, x: 0 }}
              className="flex items-center gap-1"
            >
              <div className="relative">
                <button
                  onClick={() => setShowColorPicker((v) => !v)}
                  className="w-7 h-7 rounded-lg border-2 border-[#333] transition-colors hover:border-[#555]"
                  style={{ background: color }}
                  title="Cor"
                />
                {showColorPicker && (
                  <div className="absolute top-9 right-0 bg-[#1a1a1a] border border-[#2a2a2a] rounded-lg p-2 flex gap-1.5 z-50 shadow-xl">
                    {SKETCH_COLORS.map((c) => (
                      <button
                        key={c}
                        onClick={() => { setColor(c); setShowColorPicker(false); }}
                        className="w-5 h-5 rounded-full border-2 transition-all hover:scale-110"
                        style={{ background: c, borderColor: c === color ? "#fff" : "transparent" }}
                      />
                    ))}
                  </div>
                )}
              </div>
              <button onClick={() => setSize(Math.max(2, size - 2))} className="w-6 h-6 flex items-center justify-center rounded bg-[#1a1a1a] border border-[#2a2a2a] text-[#888] hover:text-white text-xs transition-colors">−</button>
              <span className="text-[10px] text-[#666] w-3 text-center">{size}</span>
              <button onClick={() => setSize(Math.min(20, size + 2))} className="w-6 h-6 flex items-center justify-center rounded bg-[#1a1a1a] border border-[#2a2a2a] text-[#888] hover:text-white text-xs transition-colors">+</button>
              <button onClick={undoSketch} title="Ctrl+Z" className="p-1.5 rounded bg-[#1a1a1a] border border-[#2a2a2a] text-[#888] hover:text-white transition-colors"><Undo2 size={12} /></button>
              <button onClick={clearSketch} className="p-1.5 rounded bg-[#1a1a1a] border border-[#2a2a2a] text-[#f87171] hover:text-white transition-colors" title="Limpar"><Eraser size={12} /></button>
            </motion.div>
          )}

          <motion.button
            onClick={() => { setDrawMode((v) => !v); setShowColorPicker(false); }}
            whileTap={{ scale: 0.95 }}
            title="Ctrl+D"
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all border"
            style={{
              background: drawMode ? "#8b5cf620" : "#1a1a1a",
              borderColor: drawMode ? "#8b5cf6" : "#2a2a2a",
              color: drawMode ? "#8b5cf6" : "#6b7280",
            }}
          >
            <Pencil size={13} />
            {drawMode ? "Desenhando" : "Desenho"}
          </motion.button>
        </div>

        <motion.button
          onClick={() => setPickerOpen(true)}
          whileTap={{ scale: 0.95 }}
          title="Ctrl+T"
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium
            bg-[#8b5cf6] hover:bg-[#7c3aed] text-white transition-all"
          style={{ boxShadow: "0 0 20px rgba(139,92,246,0.25)" }}
        >
          <Plus size={15} />
          Terminal
        </motion.button>
      </div>

      <div className="absolute top-0 left-0 h-10 flex items-center px-4 z-10 pointer-events-none">
        <span className="text-[#2a2a2a] text-xs select-none">{currentWorkspace}</span>
      </div>

      {drawMode && (
        <div className="absolute bottom-20 left-1/2 -translate-x-1/2 z-30 pointer-events-none">
          <div className="bg-[#1a1a1a] border border-[#8b5cf640] text-[#8b5cf6] text-xs px-3 py-1.5 rounded-full">
            Modo desenho ativo — Esc para sair
          </div>
        </div>
      )}

      <AgentPicker
        open={pickerOpen}
        onConfirm={handleAgentPicked}
        onClose={() => setPickerOpen(false)}
      />
    </div>
  );
}

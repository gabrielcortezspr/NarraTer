import { useCallback, useState } from "react";
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
  type NodeChange,
  type EdgeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Plus, StickyNote, Save } from "lucide-react";
import { motion } from "framer-motion";
import TerminalTile from "@/components/TerminalTile";
import NoteTile from "@/components/NoteTile";
import AgentPicker from "@/components/AgentPicker";
import { useCanvasStore } from "@/stores/canvas";
import type { AgentType } from "@/lib/tauri";
import type { AppNode, AppEdge } from "@/stores/canvas";

const nodeTypes = {
  terminal: TerminalTile,
  note: NoteTile,
} satisfies NodeTypes;

export default function Canvas() {
  const {
    nodes: storeNodes,
    edges: storeEdges,
    setNodes: setStoreNodes,
    setEdges: setStoreEdges,
    addTerminalNode,
    addNoteNode,
    saveHistoria,
  } = useCanvasStore();

  const [nodes, setNodes, onNodesChange] = useNodesState<AppNode>(storeNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<AppEdge>(storeEdges);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [saving, setSaving] = useState(false);

  // Sync React Flow state back to store on changes
  const handleNodesChange = useCallback(
    (changes: NodeChange<AppNode>[]) => {
      onNodesChange(changes);
      setStoreNodes(nodes);
    },
    [onNodesChange, nodes, setStoreNodes]
  );

  const handleEdgesChange = useCallback(
    (changes: EdgeChange<AppEdge>[]) => {
      onEdgesChange(changes);
      setStoreEdges(edges);
    },
    [onEdgesChange, edges, setStoreEdges]
  );

  const onConnect: OnConnect = useCallback(
    (connection) => setEdges((eds) => addEdge(connection, eds)),
    [setEdges]
  );

  // Keep local React Flow state in sync when store adds nodes
  const handleAgentPicked = useCallback(
    (agentType: AgentType, command?: string) => {
      addTerminalNode(agentType, command);
      const updatedNodes = useCanvasStore.getState().nodes;
      setNodes(updatedNodes as AppNode[]);
      setPickerOpen(false);
    },
    [addTerminalNode, setNodes]
  );

  const handleAddNote = useCallback(() => {
    addNoteNode();
    const updatedNodes = useCanvasStore.getState().nodes;
    setNodes(updatedNodes as AppNode[]);
  }, [addNoteNode, setNodes]);

  const handleSave = useCallback(async () => {
    setSaving(true);
    setStoreNodes(nodes);
    setStoreEdges(edges);
    await saveHistoria("default");
    setTimeout(() => setSaving(false), 800);
  }, [nodes, edges, setStoreNodes, setStoreEdges, saveHistoria]);

  return (
    <div className="w-full h-full relative">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={onConnect}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.1}
        maxZoom={2}
        deleteKeyCode={null}
        className="bg-canvas-bg"
        proOptions={{ hideAttribution: true }}
      >
        <Background
          variant={BackgroundVariant.Dots}
          gap={24}
          size={1}
          color="#2a2a2a"
        />
        <Controls
          showInteractive={false}
          style={{ left: 16, bottom: 16 }}
        />
        <MiniMap
          nodeColor={(node) => {
            if (node.type === "note") return "#fbbf2440";
            const agentType = (node.data as { agentType?: string })?.agentType ?? "shell";
            const colors: Record<string, string> = {
              shell: "#6b728060",
              claude: "#8b5cf660",
              codex: "#3b82f660",
              custom: "#14b8a660",
            };
            return colors[agentType] ?? "#6b728060";
          }}
          maskColor="rgba(0,0,0,0.6)"
          style={{ right: 16, bottom: 16 }}
        />
      </ReactFlow>

      {/* Floating toolbar */}
      <div className="absolute top-4 right-4 flex items-center gap-2 z-10">
        {/* Save button */}
        <motion.button
          onClick={handleSave}
          whileTap={{ scale: 0.95 }}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium
            transition-all border"
          style={{
            background: saving ? "#4ade8020" : "#1a1a1a",
            borderColor: saving ? "#4ade80" : "#2a2a2a",
            color: saving ? "#4ade80" : "#9ca3af",
          }}
        >
          <Save size={13} />
          {saving ? "Salvo" : "Salvar"}
        </motion.button>

        {/* New note */}
        <motion.button
          onClick={handleAddNote}
          whileTap={{ scale: 0.95 }}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium
            bg-[#1e1a0e] border border-[#2e2810] text-[#fbbf24] hover:border-[#fbbf2460]
            transition-all"
        >
          <StickyNote size={13} />
          Nota
        </motion.button>

        {/* New terminal */}
        <motion.button
          onClick={() => setPickerOpen(true)}
          whileTap={{ scale: 0.95 }}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium
            bg-[#8b5cf6] hover:bg-[#7c3aed] text-white transition-all shadow-lg"
          style={{ boxShadow: "0 0 20px rgba(139,92,246,0.3)" }}
        >
          <Plus size={15} />
          Terminal
        </motion.button>
      </div>

      {/* Header bar */}
      <div className="absolute top-0 left-0 right-0 h-10 flex items-center px-4 z-10 pointer-events-none">
        <span className="text-[#333] text-xs font-medium tracking-widest uppercase select-none">
          NarraTer
        </span>
      </div>

      <AgentPicker
        open={pickerOpen}
        onConfirm={handleAgentPicked}
        onClose={() => setPickerOpen(false)}
      />
    </div>
  );
}

import { create } from "zustand";
import type { Node, Edge } from "@xyflow/react";
import { loadHistoria, saveHistoria } from "@/lib/tauri";
import type { AgentType } from "@/lib/tauri";

export interface TerminalNodeData extends Record<string, unknown> {
  agentType: AgentType;
  command?: string;
  label: string;
  instructions?: string;
  scheduleCommand?: string;
  scheduleIntervalSecs?: number;
  roleId?: string;
  roleName?: string;
  roleColor?: string;
}

export interface NoteNodeData extends Record<string, unknown> {
  content: string;
  label: string;
}

export type AppNode = Node<TerminalNodeData, "terminal"> | Node<NoteNodeData, "note">;
export type AppEdge = Edge;

interface CanvasStore {
  nodes: AppNode[];
  edges: AppEdge[];
  setNodes: (nodes: AppNode[]) => void;
  setEdges: (edges: AppEdge[]) => void;
  addTerminalNode: (agentType: AgentType, command?: string, instructions?: string, scheduleCommand?: string, scheduleIntervalSecs?: number, roleId?: string, roleName?: string, roleColor?: string) => string;
  updateNodeData: (id: string, patch: Record<string, unknown>) => void;
  addNoteNode: () => void;
  removeNode: (id: string) => void;
  loadHistoria: (name: string) => Promise<void>;
  saveHistoria: (name: string) => Promise<void>;
}

let nodeCounter = 0;

export const useCanvasStore = create<CanvasStore>((set, get) => ({
  nodes: [],
  edges: [],

  setNodes: (nodes) => set({ nodes }),
  setEdges: (edges) => set({ edges }),

  addTerminalNode: (agentType, command, instructions, scheduleCommand, scheduleIntervalSecs, roleId, roleName, roleColor) => {
    const id = `terminal-${Date.now()}-${nodeCounter++}`;
    const label = roleName ?? (agentType === "custom" ? (command ?? "Terminal") : agentType);
    const newNode: AppNode = {
      id,
      type: "terminal",
      position: {
        x: 80 + (nodeCounter % 5) * 60,
        y: 80 + (nodeCounter % 4) * 60,
      },
      data: { agentType, command, label, instructions, scheduleCommand, scheduleIntervalSecs, roleId, roleName, roleColor },
      style: { width: 640, height: 420 },
    };
    set((s) => ({ nodes: [...s.nodes, newNode] }));
    return id;
  },

  updateNodeData: (id, patch) => {
    set((s) => ({
      nodes: s.nodes.map((n) =>
        n.id === id ? ({ ...n, data: { ...n.data, ...patch } } as AppNode) : n
      ),
    }));
  },

  addNoteNode: () => {
    const id = `note-${Date.now()}-${nodeCounter++}`;
    const newNode: AppNode = {
      id,
      type: "note",
      position: { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { content: "", label: "Note" },
      style: { width: 280, height: 200 },
    };
    set((s) => ({ nodes: [...s.nodes, newNode] }));
  },

  removeNode: (id) => {
    set((s) => ({
      nodes: s.nodes.filter((n) => n.id !== id),
      edges: s.edges.filter((e) => e.source !== id && e.target !== id),
    }));
  },

  loadHistoria: async (name) => {
    try {
      const data = await loadHistoria(name);
      const nodes: AppNode[] = data.nodes.map((n): AppNode => {
        const base = {
          id: n.id,
          position: { x: n.x, y: n.y },
          style: { width: n.width, height: n.height },
        };
        if (n.node_type === "note") {
          return { ...base, type: "note" as const, data: { content: n.content ?? "", label: n.label ?? "Note" } };
        }
        return {
          ...base,
          type: "terminal" as const,
          data: {
            agentType: (n.agent_type ?? "shell") as AgentType,
            command: n.command,
            label: n.label ?? n.agent_type ?? "terminal",
            instructions: n.instructions,
            scheduleCommand: n.schedule_command,
            scheduleIntervalSecs: n.schedule_interval_secs,
            roleId: n.role_id,
            roleName: n.role_name,
            roleColor: n.role_color,
          },
        };
      });
      const edges: AppEdge[] = data.edges.map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        type: e.edge_type ?? "default",
        animated: e.edge_type === "agent-pipe",
        style: e.edge_type !== "agent-pipe" && e.edge_type !== "agent-note"
          ? { stroke: "#4a4a4a", strokeWidth: 1.5 }
          : undefined,
      }));
      set({ nodes, edges });
    } catch {
      // No saved historia yet — start empty
    }
  },

  saveHistoria: async (name) => {
    const { nodes, edges } = get();
    const data = {
      nodes: nodes.map((n) => {
        const tdata = n.type === "terminal" ? (n.data as TerminalNodeData) : undefined;
        return {
          id: n.id,
          node_type: n.type ?? "terminal",
          x: n.position.x,
          y: n.position.y,
          width: (n.style?.width as number) ?? 640,
          height: (n.style?.height as number) ?? 420,
          agent_type: tdata?.agentType,
          command: tdata?.command,
          label: n.data.label as string,
          content: n.type === "note" ? (n.data as NoteNodeData).content : undefined,
          instructions: tdata?.instructions,
          schedule_command: tdata?.scheduleCommand,
          schedule_interval_secs: tdata?.scheduleIntervalSecs,
          role_id: tdata?.roleId,
          role_name: tdata?.roleName,
          role_color: tdata?.roleColor,
        };
      }),
      edges: edges.map((e) => ({ id: e.id, source: e.source, target: e.target, edge_type: e.type })),
    };
    await saveHistoria(name, data);
  },
}));

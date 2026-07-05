import { create } from "zustand";
import type { Node, Edge } from "@xyflow/react";
import { loadHistoria, saveHistoria } from "@/lib/tauri";
import type { AgentType } from "@/lib/tauri";

export interface TerminalNodeData extends Record<string, unknown> {
  agentType: AgentType;
  command?: string;
  label: string;
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
  addTerminalNode: (agentType: AgentType, command?: string) => string;
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

  addTerminalNode: (agentType, command) => {
    const id = `terminal-${Date.now()}-${nodeCounter++}`;
    const label = agentType === "custom" ? (command ?? "Terminal") : agentType;
    const newNode: AppNode = {
      id,
      type: "terminal",
      position: {
        x: 100 + (nodeCounter % 4) * 50,
        y: 100 + (nodeCounter % 3) * 50,
      },
      data: { agentType, command, label },
      style: { width: 640, height: 420 },
    };
    set((s) => ({ nodes: [...s.nodes, newNode] }));
    return id;
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
          data: { agentType: (n.agent_type ?? "shell") as AgentType, command: n.command, label: n.label ?? n.agent_type ?? "terminal" },
        };
      });
      const edges: AppEdge[] = data.edges.map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
      }));
      set({ nodes, edges });
    } catch {
      // No saved historia yet — start empty
    }
  },

  saveHistoria: async (name) => {
    const { nodes, edges } = get();
    const data = {
      nodes: nodes.map((n) => ({
        id: n.id,
        node_type: n.type ?? "terminal",
        x: n.position.x,
        y: n.position.y,
        width: (n.style?.width as number) ?? 640,
        height: (n.style?.height as number) ?? 420,
        agent_type: n.type === "terminal" ? (n.data as TerminalNodeData).agentType : undefined,
        command: n.type === "terminal" ? (n.data as TerminalNodeData).command : undefined,
        label: n.data.label as string,
        content: n.type === "note" ? (n.data as NoteNodeData).content : undefined,
      })),
      edges: edges.map((e) => ({ id: e.id, source: e.source, target: e.target })),
    };
    await saveHistoria(name, data);
  },
}));

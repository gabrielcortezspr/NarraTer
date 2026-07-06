import { create } from "zustand";
import { applyNodeChanges, applyEdgeChanges } from "@xyflow/react";
import type { Node, Edge, NodeChange, EdgeChange, XYPosition } from "@xyflow/react";
import { loadHistoria, saveHistoria, connectionsSync } from "@/lib/tauri";
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

export interface TextNodeData extends Record<string, unknown> {
  text: string;
}

export interface FileTreeNodeData extends Record<string, unknown> {
  rootPath: string;
  expandedPaths: string[];
}

export interface AttachmentNodeData extends Record<string, unknown> {
  path: string;
  fileName: string;
}

export interface PortalNodeData extends Record<string, unknown> {
  url: string;
}

export type AppNode =
  | Node<TerminalNodeData, "terminal">
  | Node<NoteNodeData, "note">
  | Node<TextNodeData, "text">
  | Node<FileTreeNodeData, "filetree">
  | Node<AttachmentNodeData, "attachment">
  | Node<PortalNodeData, "portal">;
export type AppEdge = Edge;

export interface AddTerminalOpts {
  agentType: AgentType;
  command?: string;
  instructions?: string;
  scheduleCommand?: string;
  scheduleIntervalSecs?: number;
  roleId?: string;
  roleName?: string;
  roleColor?: string;
}

interface CanvasStore {
  nodes: AppNode[];
  edges: AppEdge[];
  // True once a historia finished loading — gates auto-save and fitView so
  // neither runs against a canvas that is mid-hydration.
  hydrated: boolean;
  onNodesChange: (changes: NodeChange<AppNode>[]) => void;
  onEdgesChange: (changes: EdgeChange<AppEdge>[]) => void;
  addEdge: (edge: AppEdge) => void;
  addTerminalNode: (opts: AddTerminalOpts, position?: XYPosition) => string;
  updateNodeData: (id: string, patch: Record<string, unknown>) => void;
  addNoteNode: (position?: XYPosition, initial?: Partial<NoteNodeData>) => string;
  addTextNode: (position?: XYPosition, initialText?: string) => string;
  moveNode: (id: string, position: XYPosition) => void;
  addFileTreeNode: (position?: XYPosition, rootPath?: string) => string;
  addAttachmentNode: (position: XYPosition, path: string) => string;
  addPortalNode: (position?: XYPosition, url?: string) => string;
  appendNoteContent: (noteId: string, text: string) => void;
  removeNode: (id: string) => void;
  loadHistoria: (name: string) => Promise<void>;
  saveHistoria: (name: string) => Promise<void>;
}

let nodeCounter = 0;

// Mirror agent-pipe edges into the backend routing table — narrater send/ask
// is only allowed along these directed routes.
function syncConnections(edges: AppEdge[]) {
  const pairs = edges
    .filter((e) => e.type === "agent-pipe")
    .map((e) => [e.source, e.target] as [string, string]);
  connectionsSync(pairs).catch(console.error);
}

export const useCanvasStore = create<CanvasStore>((set, get) => ({
  nodes: [],
  edges: [],
  hydrated: false,

  onNodesChange: (changes) => {
    set({ nodes: applyNodeChanges(changes, get().nodes) });
  },

  onEdgesChange: (changes) => {
    const edges = applyEdgeChanges(changes, get().edges);
    set({ edges });
    if (changes.some((c) => c.type === "remove")) syncConnections(edges);
  },

  addEdge: (edge) => {
    const edges = [...get().edges, edge];
    set({ edges });
    syncConnections(edges);
  },

  addTerminalNode: (opts, position) => {
    const id = `terminal-${Date.now()}-${nodeCounter++}`;
    const { agentType, command, roleName } = opts;
    const label = roleName ?? (agentType === "custom" ? (command ?? "Terminal") : agentType);
    const newNode: AppNode = {
      id,
      type: "terminal",
      position: position ?? {
        x: 80 + (nodeCounter % 5) * 60,
        y: 80 + (nodeCounter % 4) * 60,
      },
      data: { ...opts, label },
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

  addNoteNode: (position, initial) => {
    const id = `note-${Date.now()}-${nodeCounter++}`;
    const newNode: AppNode = {
      id,
      type: "note",
      position: position ?? { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { content: "", label: "Note", ...initial },
      style: { width: 280, height: 200 },
    };
    set((s) => ({ nodes: [...s.nodes, newNode] }));
    return id;
  },

  addTextNode: (position, initialText) => {
    const id = `text-${Date.now()}-${nodeCounter++}`;
    const newNode: AppNode = {
      id,
      type: "text",
      position: position ?? { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { text: initialText ?? "" },
      style: { width: 240, height: 80 },
    };
    set((s) => ({ nodes: [...s.nodes, newNode] }));
    return id;
  },

  moveNode: (id, position) => {
    set((s) => ({
      nodes: s.nodes.map((n) => (n.id === id ? ({ ...n, position } as AppNode) : n)),
    }));
  },

  addFileTreeNode: (position, rootPath) => {
    const id = `filetree-${Date.now()}-${nodeCounter++}`;
    const newNode: AppNode = {
      id,
      type: "filetree",
      position: position ?? { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { rootPath: rootPath ?? "~", expandedPaths: [] },
      style: { width: 280, height: 360 },
    };
    set((s) => ({ nodes: [...s.nodes, newNode] }));
    return id;
  },

  addAttachmentNode: (position, path) => {
    const id = `attachment-${Date.now()}-${nodeCounter++}`;
    const fileName = path.split("/").pop() ?? path;
    const newNode: AppNode = {
      id,
      type: "attachment",
      position,
      data: { path, fileName },
      style: { width: 260, height: 220 },
    };
    set((s) => ({ nodes: [...s.nodes, newNode] }));
    return id;
  },

  addPortalNode: (position, url) => {
    const id = `portal-${Date.now()}-${nodeCounter++}`;
    const newNode: AppNode = {
      id,
      type: "portal",
      position: position ?? { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { url: url ?? "" },
      style: { width: 720, height: 520 },
    };
    set((s) => ({ nodes: [...s.nodes, newNode] }));
    return id;
  },

  appendNoteContent: (noteId, text) => {
    set((s) => ({
      nodes: s.nodes.map((n) => {
        if (n.id !== noteId || n.type !== "note") return n;
        const current = (n.data as NoteNodeData).content ?? "";
        const separator = current.length > 0 ? "\n" : "";
        return {
          ...n,
          data: { ...n.data, content: current + separator + text, isAgentLive: true },
        } as AppNode;
      }),
    }));
  },

  removeNode: (id) => {
    const edges = get().edges.filter((e) => e.source !== id && e.target !== id);
    set((s) => ({
      nodes: s.nodes.filter((n) => n.id !== id),
      edges,
    }));
    syncConnections(edges);
  },

  loadHistoria: async (name) => {
    set({ hydrated: false });
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
        if (n.node_type === "text") {
          return { ...base, type: "text" as const, data: { text: n.content ?? "" } };
        }
        if (n.node_type === "filetree") {
          return {
            ...base,
            type: "filetree" as const,
            data: { rootPath: n.path ?? "~", expandedPaths: n.expanded_paths ?? [] },
          };
        }
        if (n.node_type === "attachment") {
          const path = n.path ?? "";
          return {
            ...base,
            type: "attachment" as const,
            data: { path, fileName: path.split("/").pop() ?? path },
          };
        }
        if (n.node_type === "portal") {
          return { ...base, type: "portal" as const, data: { url: n.url ?? "" } };
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
        sourceHandle: e.source_handle,
        targetHandle: e.target_handle,
        type: e.edge_type ?? "default",
        animated: e.edge_type === "agent-pipe",
        style: e.edge_type !== "agent-pipe" && e.edge_type !== "agent-note"
          ? { stroke: "#4a4a4a", strokeWidth: 1.5 }
          : undefined,
      }));
      set({ nodes, edges, hydrated: true });
      syncConnections(edges);
    } catch {
      // No saved historia yet — start empty
      set({ nodes: [], edges: [], hydrated: true });
      syncConnections([]);
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
          width: n.width ?? (n.style?.width as number) ?? 640,
          height: n.height ?? (n.style?.height as number) ?? 420,
          agent_type: tdata?.agentType,
          command: tdata?.command,
          label: n.data.label as string | undefined,
          content:
            n.type === "note" ? (n.data as NoteNodeData).content
            : n.type === "text" ? (n.data as TextNodeData).text
            : undefined,
          path:
            n.type === "filetree" ? (n.data as FileTreeNodeData).rootPath
            : n.type === "attachment" ? (n.data as AttachmentNodeData).path
            : undefined,
          url: n.type === "portal" ? (n.data as PortalNodeData).url : undefined,
          expanded_paths: n.type === "filetree" ? (n.data as FileTreeNodeData).expandedPaths : undefined,
          instructions: tdata?.instructions,
          schedule_command: tdata?.scheduleCommand,
          schedule_interval_secs: tdata?.scheduleIntervalSecs,
          role_id: tdata?.roleId,
          role_name: tdata?.roleName,
          role_color: tdata?.roleColor,
        };
      }),
      edges: edges.map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        edge_type: e.type,
        source_handle: e.sourceHandle ?? undefined,
        target_handle: e.targetHandle ?? undefined,
      })),
    };
    await saveHistoria(name, data);
  },
}));

import { listen } from "@tauri-apps/api/event";
import { useCanvasStore } from "@/stores/canvas";
import { canvasRespond } from "@/lib/tauri";
import type { AppEdge, AppNode, NoteNodeData } from "@/stores/canvas";

// Agent → canvas bridge (MCP): the backend emits `canvas_request` for each
// canvas_* tool called by an agent; we apply the action on the store (single
// source of truth — auto-save persists it) and return the result via
// canvas_respond, which wakes the socket handler waiting with a timeout.
export interface CanvasRequest {
  req_id: string;
  from: string;
  from_label: string;
  action: string;
  params: Record<string, unknown> | null;
}

// Resolves by exact id or, failing that, by label (case-insensitive). Readable
// error when the label matches more than one node — the agent must use the id.
function resolveNode(nodes: AppNode[], ref: string, kind: string): AppNode | string {
  const byId = nodes.find((n) => n.id === ref);
  if (byId) return byId;
  const byLabel = nodes.filter(
    (n) => ((n.data as { label?: string }).label ?? "").toLowerCase() === ref.toLowerCase()
  );
  if (byLabel.length > 1) {
    return `Error: label '${ref}' is ambiguous (${byLabel.length} ${kind}s) — use the id (canvas_list_nodes)`;
  }
  if (byLabel.length === 0) return `Error: no ${kind} with id or label '${ref}'`;
  return byLabel[0];
}

// Default position for agent-created nodes: to the right of the caller's terminal.
function defaultPosition(nodes: AppNode[], fromId: string): { x: number; y: number } {
  const fromNode = nodes.find((n) => n.id === fromId);
  return fromNode
    ? {
        x: fromNode.position.x + (fromNode.width ?? (fromNode.style?.width as number) ?? 520) + 60,
        y: fromNode.position.y,
      }
    : { x: 200, y: 200 };
}

function handleRequest(req: CanvasRequest): string {
  const store = useCanvasStore.getState();
  const params = req.params ?? {};

  switch (req.action) {
    case "list_nodes": {
      const list = store.nodes.map((n) => ({
        id: n.id,
        type: n.type,
        label: (n.data as { label?: string }).label ?? null,
        x: Math.round(n.position.x),
        y: Math.round(n.position.y),
      }));
      return JSON.stringify(list, null, 2);
    }

    case "create_note": {
      const content = typeof params.content === "string" ? params.content : "";
      if (!content) return "Error: empty content";
      const label = typeof params.label === "string" && params.label ? params.label : "Note";

      const position =
        typeof params.x === "number" && typeof params.y === "number"
          ? { x: params.x, y: params.y }
          : defaultPosition(store.nodes, req.from);

      const id = store.addNoteNode(position, { content, label });
      return `ok: note created (id: ${id})`;
    }

    case "read_note": {
      const ref = typeof params.id === "string" ? params.id : "";
      if (!ref) return "Error: provide the note's id or label";
      const resolved = resolveNode(store.nodes.filter((n) => n.type === "note"), ref, "note");
      if (typeof resolved === "string") return resolved;
      const content = (resolved.data as NoteNodeData).content ?? "";
      return content.length > 0 ? content : "(empty note)";
    }

    case "create_text": {
      const text = typeof params.text === "string" ? params.text : "";
      if (!text) return "Error: empty text";
      const position =
        typeof params.x === "number" && typeof params.y === "number"
          ? { x: params.x, y: params.y }
          : defaultPosition(store.nodes, req.from);
      const id = store.addTextNode(position, text);
      return `ok: text created (id: ${id})`;
    }

    case "move_node": {
      const ref = typeof params.id === "string" ? params.id : "";
      if (!ref) return "Error: provide the node's id or label";
      if (typeof params.x !== "number" || typeof params.y !== "number") {
        return "Error: x and y are required (numbers)";
      }
      const resolved = resolveNode(store.nodes, ref, "node");
      if (typeof resolved === "string") return resolved;
      store.moveNode(resolved.id, { x: params.x, y: params.y });
      return `ok: node moved (id: ${resolved.id}) to (${Math.round(params.x)}, ${Math.round(params.y)})`;
    }

    case "connect_nodes": {
      const sourceRef = typeof params.source === "string" ? params.source : "";
      const targetRef = typeof params.target === "string" ? params.target : "";
      if (!sourceRef || !targetRef) return "Error: provide source and target (id or label)";

      const source = resolveNode(store.nodes, sourceRef, "node");
      if (typeof source === "string") return source;
      const target = resolveNode(store.nodes, targetRef, "node");
      if (typeof target === "string") return target;
      if (source.id === target.id) return "Error: source and target are the same node";

      const existing = store.edges.find((e) => e.source === source.id && e.target === target.id);
      if (existing) return `ok: connection already existed (id: ${existing.id}, type: ${existing.type})`;

      // Same classification as the Canvas onConnect — agent-pipe becomes a
      // communication route in the backend via syncConnections (inside addEdge).
      const isAgentNote =
        (source.type === "terminal" && target.type === "note") ||
        (source.type === "note" && target.type === "terminal");
      const isAgentPipe = source.type === "terminal" && target.type === "terminal";
      const edgeType = isAgentNote ? "agent-note" : isAgentPipe ? "agent-pipe" : "default";

      const edge: AppEdge = {
        id: `edge-mcp-${crypto.randomUUID().slice(0, 8)}`,
        source: source.id,
        target: target.id,
        type: edgeType,
        animated: edgeType === "default",
        style: edgeType === "default" ? { stroke: "#4a4a4a", strokeWidth: 1.5 } : undefined,
      };
      store.addEdge(edge);
      return `ok: connection created (id: ${edge.id}, type: ${edgeType}, route: ${source.id} → ${target.id})`;
    }

    case "update_note": {
      const ref = typeof params.id === "string" ? params.id : "";
      const content = typeof params.content === "string" ? params.content : "";
      if (!ref) return "Error: provide the note's id or label";
      if (!content) return "Error: empty content";

      const resolved = resolveNode(store.nodes.filter((n) => n.type === "note"), ref, "note");
      if (typeof resolved === "string") return resolved;

      if (params.mode === "replace") store.updateNodeData(resolved.id, { content });
      else store.appendNoteContent(resolved.id, content);
      return `ok: note updated (id: ${resolved.id})`;
    }

    default:
      return `Error: unknown canvas action '${req.action}'`;
  }
}

export function initCanvasBridge(): Promise<() => void> {
  return listen<CanvasRequest>("canvas_request", (event) => {
    const req = event.payload;
    let result: string;
    try {
      result = handleRequest(req);
    } catch (e) {
      result = `Error: ${e}`;
    }
    // Always respond — including errors — so the agent never hits the timeout
    canvasRespond(req.req_id, result).catch(console.error);
  });
}

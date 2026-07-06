import { listen } from "@tauri-apps/api/event";
import { useCanvasStore } from "@/stores/canvas";
import { canvasRespond } from "@/lib/tauri";
import type { AppEdge, AppNode, NoteNodeData } from "@/stores/canvas";

// Ponte agente → canvas (MCP): o backend emite `canvas_request` para cada
// tool canvas_* chamada por um agente; aplicamos a ação no store (única fonte
// de verdade — o auto-save persiste) e devolvemos o resultado via
// canvas_respond, que acorda o handler do socket aguardando com timeout.
export interface CanvasRequest {
  req_id: string;
  from: string;
  from_label: string;
  action: string;
  params: Record<string, unknown> | null;
}

// Resolve por id exato ou, na falta, por label (case-insensitive). Erro
// legível quando o label bate em mais de um nó — o agente deve usar o id.
function resolveNode(nodes: AppNode[], ref: string, kind: string): AppNode | string {
  const byId = nodes.find((n) => n.id === ref);
  if (byId) return byId;
  const byLabel = nodes.filter(
    (n) => ((n.data as { label?: string }).label ?? "").toLowerCase() === ref.toLowerCase()
  );
  if (byLabel.length > 1) {
    return `Erro: label '${ref}' é ambíguo (${byLabel.length} ${kind}s) — use o id (canvas_list_nodes)`;
  }
  if (byLabel.length === 0) return `Erro: nenhum(a) ${kind} com id ou label '${ref}'`;
  return byLabel[0];
}

// Default de posição para nós criados por agente: à direita do terminal de quem pediu.
function defaultPosition(nodes: AppNode[], fromId: string): { x: number; y: number } {
  const fromNode = nodes.find((n) => n.id === fromId);
  return fromNode
    ? {
        x: fromNode.position.x + (fromNode.width ?? (fromNode.style?.width as number) ?? 520) + 60,
        y: fromNode.position.y,
      }
    : { x: 200, y: 200 };
}

let edgeIdCounter = 0;

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
      if (!content) return "Erro: content vazio";
      const label = typeof params.label === "string" && params.label ? params.label : "Note";

      const position =
        typeof params.x === "number" && typeof params.y === "number"
          ? { x: params.x, y: params.y }
          : defaultPosition(store.nodes, req.from);

      const id = store.addNoteNode(position, { content, label });
      return `ok: nota criada (id: ${id})`;
    }

    case "read_note": {
      const ref = typeof params.id === "string" ? params.id : "";
      if (!ref) return "Erro: informe o id ou label da nota";
      const resolved = resolveNode(store.nodes.filter((n) => n.type === "note"), ref, "nota");
      if (typeof resolved === "string") return resolved;
      const content = (resolved.data as NoteNodeData).content ?? "";
      return content.length > 0 ? content : "(nota vazia)";
    }

    case "create_text": {
      const text = typeof params.text === "string" ? params.text : "";
      if (!text) return "Erro: text vazio";
      const position =
        typeof params.x === "number" && typeof params.y === "number"
          ? { x: params.x, y: params.y }
          : defaultPosition(store.nodes, req.from);
      const id = store.addTextNode(position, text);
      return `ok: texto criado (id: ${id})`;
    }

    case "move_node": {
      const ref = typeof params.id === "string" ? params.id : "";
      if (!ref) return "Erro: informe o id ou label do nó";
      if (typeof params.x !== "number" || typeof params.y !== "number") {
        return "Erro: x e y são obrigatórios (números)";
      }
      const resolved = resolveNode(store.nodes, ref, "nó");
      if (typeof resolved === "string") return resolved;
      store.moveNode(resolved.id, { x: params.x, y: params.y });
      return `ok: nó movido (id: ${resolved.id}) para (${Math.round(params.x)}, ${Math.round(params.y)})`;
    }

    case "connect_nodes": {
      const sourceRef = typeof params.source === "string" ? params.source : "";
      const targetRef = typeof params.target === "string" ? params.target : "";
      if (!sourceRef || !targetRef) return "Erro: informe source e target (id ou label)";

      const source = resolveNode(store.nodes, sourceRef, "nó");
      if (typeof source === "string") return source;
      const target = resolveNode(store.nodes, targetRef, "nó");
      if (typeof target === "string") return target;
      if (source.id === target.id) return "Erro: source e target são o mesmo nó";

      const existing = store.edges.find((e) => e.source === source.id && e.target === target.id);
      if (existing) return `ok: conexão já existia (id: ${existing.id}, tipo: ${existing.type})`;

      // Mesma classificação do onConnect do Canvas — agent-pipe vira rota de
      // comunicação no backend via syncConnections (dentro de addEdge).
      const isAgentNote =
        (source.type === "terminal" && target.type === "note") ||
        (source.type === "note" && target.type === "terminal");
      const isAgentPipe = source.type === "terminal" && target.type === "terminal";
      const edgeType = isAgentNote ? "agent-note" : isAgentPipe ? "agent-pipe" : "default";

      const edge: AppEdge = {
        id: `edge-${Date.now()}-mcp-${edgeIdCounter++}`,
        source: source.id,
        target: target.id,
        type: edgeType,
        animated: edgeType === "default",
        style: edgeType === "default" ? { stroke: "#4a4a4a", strokeWidth: 1.5 } : undefined,
      };
      store.addEdge(edge);
      return `ok: conexão criada (id: ${edge.id}, tipo: ${edgeType}, rota: ${source.id} → ${target.id})`;
    }

    case "update_note": {
      const ref = typeof params.id === "string" ? params.id : "";
      const content = typeof params.content === "string" ? params.content : "";
      if (!ref) return "Erro: informe o id ou label da nota";
      if (!content) return "Erro: content vazio";

      const resolved = resolveNode(store.nodes.filter((n) => n.type === "note"), ref, "nota");
      if (typeof resolved === "string") return resolved;

      if (params.mode === "replace") store.updateNodeData(resolved.id, { content });
      else store.appendNoteContent(resolved.id, content);
      return `ok: nota atualizada (id: ${resolved.id})`;
    }

    default:
      return `Erro: ação de canvas desconhecida '${req.action}'`;
  }
}

export function initCanvasBridge(): Promise<() => void> {
  return listen<CanvasRequest>("canvas_request", (event) => {
    const req = event.payload;
    let result: string;
    try {
      result = handleRequest(req);
    } catch (e) {
      result = `Erro: ${e}`;
    }
    // Sempre responder — inclusive erro — para não deixar o agente no timeout
    canvasRespond(req.req_id, result).catch(console.error);
  });
}

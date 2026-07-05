import { listen } from "@tauri-apps/api/event";
import { useCanvasStore } from "@/stores/canvas";
import { canvasRespond } from "@/lib/tauri";
import type { NoteNodeData } from "@/stores/canvas";

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

      let position: { x: number; y: number };
      if (typeof params.x === "number" && typeof params.y === "number") {
        position = { x: params.x, y: params.y };
      } else {
        // Default: ao lado direito do terminal do agente que pediu
        const fromNode = store.nodes.find((n) => n.id === req.from);
        position = fromNode
          ? {
              x: fromNode.position.x + (fromNode.width ?? (fromNode.style?.width as number) ?? 640) + 60,
              y: fromNode.position.y,
            }
          : { x: 200, y: 200 };
      }

      const id = store.addNoteNode(position, { content, label });
      return `ok: nota criada (id: ${id})`;
    }

    case "update_note": {
      const ref = typeof params.id === "string" ? params.id : "";
      const content = typeof params.content === "string" ? params.content : "";
      if (!ref) return "Erro: informe o id ou label da nota";
      if (!content) return "Erro: content vazio";

      const notes = store.nodes.filter((n) => n.type === "note");
      let target = notes.find((n) => n.id === ref);
      if (!target) {
        const byLabel = notes.filter(
          (n) => ((n.data as NoteNodeData).label ?? "").toLowerCase() === ref.toLowerCase()
        );
        if (byLabel.length > 1) {
          return `Erro: label '${ref}' é ambíguo (${byLabel.length} notas) — use o id (canvas_list_nodes)`;
        }
        target = byLabel[0];
      }
      if (!target) return `Erro: nenhuma nota com id ou label '${ref}'`;

      if (params.mode === "replace") store.updateNodeData(target.id, { content });
      else store.appendNoteContent(target.id, content);
      return `ok: nota atualizada (id: ${target.id})`;
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

import { useState, useCallback, useRef, useEffect } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import { X, Globe, RotateCw, ExternalLink, ArrowRight } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { PortalNodeData } from "@/stores/canvas";
import { openUrl } from "@/lib/tauri";
import type { Node, NodeProps } from "@xyflow/react";

type PortalNode = Node<PortalNodeData, "portal">;

const ACCENT = "#22d3ee";

function normalizeUrl(input: string): string {
  const trimmed = input.trim();
  if (!trimmed) return "";
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
}

// Mini navegador via iframe: escala e move junto com o canvas, mas sites com
// X-Frame-Options/frame-ancestors não carregam — daí o aviso fixo e o botão
// de abrir no navegador externo. Upgrade futuro: child webview do Tauri.
export default function PortalTile({ id, data, selected, dragging }: NodeProps<PortalNode>) {
  const updateNodeData = useCanvasStore((s) => s.updateNodeData);
  const removeNode = useCanvasStore((s) => s.removeNode);

  const [urlInput, setUrlInput] = useState(data.url ?? "");
  const [reloadKey, setReloadKey] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Recém-criado sem URL: foco direto na barra de navegação
  useEffect(() => {
    if (!data.url) inputRef.current?.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const navigate = useCallback(() => {
    const url = normalizeUrl(urlInput);
    setUrlInput(url);
    updateNodeData(id, { url });
    setReloadKey((k) => k + 1);
  }, [id, urlInput, updateNodeData]);

  return (
    <div
      className="flex flex-col w-full h-full rounded-xl overflow-hidden"
      style={{
        background: "#0c1a1d",
        boxShadow: selected
          ? `0 0 0 1px ${ACCENT}, 0 8px 32px rgba(0,0,0,0.5)`
          : "0 4px 24px rgba(0,0,0,0.4), 0 0 0 1px rgba(34,211,238,0.15)",
        transition: "box-shadow 0.15s ease",
      }}
    >
      <NodeResizer
        minWidth={320}
        minHeight={240}
        isVisible={selected}
        lineStyle={{ borderColor: ACCENT }}
        handleStyle={{ borderColor: ACCENT, background: "#0c1a1d" }}
      />

      {/* Barra de navegação (também é o drag handle) */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab select-none"
        style={{ background: "#0f2226", borderBottom: "1px solid #163238" }}
      >
        <Globe size={12} style={{ color: ACCENT }} className="shrink-0" />
        <input
          ref={inputRef}
          value={urlInput}
          onChange={(e) => setUrlInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && navigate()}
          onMouseDown={(e) => e.stopPropagation()}
          placeholder="exemplo.com"
          spellCheck={false}
          className="flex-1 min-w-0 bg-[#0c1a1d] border border-[#163238] rounded px-2 py-0.5
            text-[11px] text-[#9adbe5] placeholder-[#2a4a52] outline-none focus:border-[#22d3ee60] nodrag"
        />
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={navigate}
          className="text-[#557] hover:text-white transition-colors p-0.5 nodrag"
          title="Ir"
        >
          <ArrowRight size={11} />
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => setReloadKey((k) => k + 1)}
          className="text-[#557] hover:text-white transition-colors p-0.5 nodrag"
          title="Recarregar"
        >
          <RotateCw size={11} />
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => data.url && openUrl(data.url).catch(console.error)}
          className="text-[#557] hover:text-white transition-colors p-0.5 nodrag"
          title="Abrir no navegador externo"
        >
          <ExternalLink size={11} />
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); removeNode(id); }}
          className="text-[#666] hover:text-[#f87171] transition-colors p-0.5 rounded nodrag"
        >
          <X size={12} />
        </button>
      </div>

      {/* Página */}
      <div className="flex-1 min-h-0 relative nodrag nowheel" onMouseDown={(e) => e.stopPropagation()}>
        {data.url ? (
          <iframe
            key={reloadKey}
            src={data.url}
            title={data.url}
            className="w-full h-full border-0 bg-white"
            // Durante o drag o iframe engoliria os eventos do mouse
            style={{ pointerEvents: dragging ? "none" : "auto" }}
            sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center">
            <span className="text-[11px] text-[#2a4a52]">Digite uma URL acima</span>
          </div>
        )}
      </div>

      {/* Expectativa: detecção de bloqueio é impossível cross-origin */}
      <div
        className="shrink-0 px-3 py-1 text-[9px] text-[#2a4a52] select-none"
        style={{ background: "#0f2226", borderTop: "1px solid #163238" }}
      >
        Alguns sites bloqueiam embed — use ↗ para abrir no navegador
      </div>

      <Handle type="target" position={Position.Left} style={{ background: ACCENT, border: "none", width: 8, height: 8 }} />
      <Handle type="source" position={Position.Right} style={{ background: ACCENT, border: "none", width: 8, height: 8 }} />
    </div>
  );
}

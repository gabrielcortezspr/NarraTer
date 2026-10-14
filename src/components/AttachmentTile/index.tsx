import { memo, useState, useEffect } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import { X, Paperclip, FileText, ExternalLink } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { AttachmentNodeData } from "@/stores/canvas";
import { fsReadFileBase64, openInEditor } from "@/lib/tauri";
import { EDITORS } from "@/lib/editors";
import type { Node, NodeProps } from "@xyflow/react";

type AttachmentNode = Node<AttachmentNodeData, "attachment">;

// Identity of this node kind: icon and top thread only (docs/design.md)
const ACCENT = "#ff7eb6";
const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"]);

function isImage(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTS.has(ext);
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1_048_576) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1_048_576).toFixed(1)} MB`;
}

function AttachmentTile({ id, data, selected }: NodeProps<AttachmentNode>) {
  const removeNode = useCanvasStore((s) => s.removeNode);
  const [imageSrc, setImageSrc] = useState<string | null>(null);
  const [size, setSize] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editorMenu, setEditorMenu] = useState(false);

  useEffect(() => {
    let cancelled = false;
    if (!data.path) return;
    fsReadFileBase64(data.path)
      .then((blob) => {
        if (cancelled) return;
        setSize(blob.size);
        if (isImage(data.path)) setImageSrc(`data:${blob.mime};base64,${blob.base64}`);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [data.path]);

  return (
    <div
      className="flex flex-col w-full h-full rounded-xl overflow-hidden bg-surface border border-line transition-shadow duration-150"
      style={{ boxShadow: selected ? "var(--ring-accent), var(--shadow-tile)" : "var(--shadow-tile)" }}
    >
      <div className="h-px shrink-0 opacity-80" style={{ background: `linear-gradient(90deg, transparent 4%, ${ACCENT} 50%, transparent 96%)` }} />
      <NodeResizer
        minWidth={160}
        minHeight={120}
        isVisible={selected}
        lineStyle={{ borderColor: "transparent" }}
        handleStyle={{ width: 8, height: 8, borderRadius: 2, border: "1.5px solid var(--accent)", background: "var(--surface)" }}
      />

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab select-none"
        
      >
        <Paperclip size={12} style={{ color: ACCENT }} className="shrink-0" />
        <span className="text-[11px] truncate flex-1" style={{ color: ACCENT }} title={data.path}>
          {data.fileName}
        </span>
        {size !== null && <span className="text-[9px] text-ink-2 shrink-0">{formatSize(size)}</span>}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); removeNode(id); }}
          className="text-ink-muted hover:text-danger-text transition-colors p-0.5 rounded nodrag"
        >
          <X size={12} />
        </button>
      </div>

      {/* Content */}
      <div
        className="flex-1 min-h-0 flex items-center justify-center overflow-hidden nodrag nowheel relative"
        onMouseDown={(e) => e.stopPropagation()}
      >
        {imageSrc ? (
          <img src={imageSrc} alt={data.fileName} className="max-w-full max-h-full object-contain" draggable={false} />
        ) : (
          <div className="flex flex-col items-center gap-2 p-3 text-center">
            <FileText size={28} className="text-ink-muted" />
            <span className="text-[11px] text-ink-2 break-all">{data.fileName}</span>
            {error && <span className="text-[10px] text-danger-text break-all">{error}</span>}
            <div className="relative">
              <button
                onClick={() => setEditorMenu((v) => !v)}
                className="flex items-center gap-1 text-[10px] px-2 py-1 rounded border border-line text-ink-2 hover:text-ink hover:border-line-strong transition-colors"
              >
                <ExternalLink size={10} /> Open in editor
              </button>
              {editorMenu && (
                <div className="absolute left-1/2 -translate-x-1/2 top-7 z-20 bg-raised border border-line rounded-lg shadow-xl py-1 min-w-[100px]">
                  {EDITORS.map((ed) => (
                    <button
                      key={ed.cmd}
                      onClick={() => {
                        openInEditor(ed.cmd, data.path).catch(console.error);
                        setEditorMenu(false);
                      }}
                      className="block w-full text-left px-3 py-1 text-[11px] text-ink hover:bg-hover hover:text-ink"
                    >
                      {ed.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      <Handle type="target" position={Position.Left} className="narrater-handle" />
      <Handle type="source" position={Position.Right} className="narrater-handle" />
    </div>
  );
}

export default memo(AttachmentTile);

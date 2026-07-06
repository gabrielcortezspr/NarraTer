import { memo, useState, useCallback, useEffect, useRef } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import { X, StickyNote, Radio, Check, Copy } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { NoteNodeData } from "@/stores/canvas";
import type { Node, NodeProps } from "@xyflow/react";

type NoteNode = Node<NoteNodeData, "note">;

function NoteTile({ id, data, selected }: NodeProps<NoteNode>) {
  const [content, setContent] = useState(data.content ?? "");
  const updateNodeData = useCanvasStore((s) => s.updateNodeData);
  const removeNode = useCanvasStore((s) => s.removeNode);
  const prevExternalContent = useRef(data.content);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const isLive = data.isAgentLive as boolean | undefined;
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleValue, setTitleValue] = useState(data.label ?? "Nota");
  const [copied, setCopied] = useState(false);

  // Sync when external agent writes to this note
  useEffect(() => {
    if (data.content !== prevExternalContent.current) {
      prevExternalContent.current = data.content;
      setContent(data.content ?? "");
    }
  }, [data.content]);

  // Agente escrevendo → scroll acompanha o fim, como um tail -f
  useEffect(() => {
    if (isLive && textareaRef.current) {
      textareaRef.current.scrollTop = textareaRef.current.scrollHeight;
    }
  }, [content, isLive]);

  const commitTitle = useCallback(() => {
    setEditingTitle(false);
    const label = titleValue.trim() || "Nota";
    setTitleValue(label);
    updateNodeData(id, { label });
  }, [id, titleValue, updateNodeData]);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(content).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    });
  }, [content]);

  // Write local edits back to the store so they're included on save
  const handleChange = useCallback(
    (value: string) => {
      setContent(value);
      updateNodeData(id, { content: value });
    },
    [id, updateNodeData]
  );

  const handleClose = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      removeNode(id);
    },
    [id, removeNode]
  );

  return (
    <div
      className="flex flex-col w-full h-full rounded-xl overflow-hidden"
      style={{
        background: "#1e1a0e",
        boxShadow: isLive
          ? "0 0 0 1.5px #fbbf24, 0 8px 32px rgba(251,191,36,0.15)"
          : selected
          ? "0 0 0 1px #fbbf24, 0 8px 32px rgba(0,0,0,0.5)"
          : "0 4px 24px rgba(0,0,0,0.4), 0 0 0 1px rgba(251,191,36,0.15)",
        transition: "box-shadow 0.15s ease",
      }}
    >
      <NodeResizer
        minWidth={180}
        minHeight={120}
        isVisible={selected}
        lineStyle={{ borderColor: "#fbbf24" }}
        handleStyle={{ borderColor: "#fbbf24", background: "#1e1a0e" }}
      />

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab select-none"
        style={{ background: "#231e0e", borderBottom: "1px solid #2e2810" }}
      >
        <StickyNote size={12} className="text-[#fbbf24] shrink-0" />
        {editingTitle ? (
          <input
            autoFocus
            value={titleValue}
            onChange={(e) => setTitleValue(e.target.value)}
            onBlur={commitTitle}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitTitle();
              if (e.key === "Escape") { setTitleValue(data.label ?? "Nota"); setEditingTitle(false); }
            }}
            onMouseDown={(e) => e.stopPropagation()}
            className="flex-1 min-w-0 bg-transparent text-[#fbbf24] text-xs outline-none border-b border-[#fbbf2450] nodrag"
            aria-label="Título da nota"
          />
        ) : (
          <span
            className="text-[#fbbf24] text-xs flex-1 truncate"
            title="Duplo clique para renomear"
            onDoubleClick={(e) => { e.stopPropagation(); setEditingTitle(true); }}
          >
            {data.label ?? "Nota"}
          </span>
        )}

        {/* Live indicator — pulsing when agent is writing */}
        {isLive && (
          <span className="flex items-center gap-1 text-[9px] text-[#fbbf24] animate-pulse">
            <Radio size={9} /> ao vivo
          </span>
        )}

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); handleCopy(); }}
          title="Copiar conteúdo"
          aria-label="Copiar conteúdo"
          className="text-[#666] hover:text-[#fbbf24] transition-colors p-0.5 rounded nodrag"
        >
          {copied ? <Check size={12} className="text-[#4ade80]" /> : <Copy size={12} />}
        </button>

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleClose}
          title="Fechar nota"
          aria-label="Fechar nota"
          className="text-[#666] hover:text-[#f87171] transition-colors p-0.5 rounded nodrag"
        >
          <X size={12} />
        </button>
      </div>

      {/* Content */}
      <textarea
        ref={textareaRef}
        value={content}
        onChange={(e) => handleChange(e.target.value)}
        placeholder="Escreva aqui..."
        className="flex-1 resize-none bg-transparent text-[#d4c899] text-sm p-3 outline-none
          placeholder-[#4a4020] nodrag nowheel"
        style={{ fontFamily: "inherit", lineHeight: 1.6 }}
        onMouseDown={(e) => e.stopPropagation()}
      />

      <Handle type="target" position={Position.Left} style={{ background: "#fbbf24", border: "none", width: 8, height: 8 }} />
      <Handle type="source" position={Position.Right} style={{ background: "#fbbf24", border: "none", width: 8, height: 8 }} />
    </div>
  );
}

export default memo(NoteTile);

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
  const [titleValue, setTitleValue] = useState(data.label ?? "Note");
  const [copied, setCopied] = useState(false);

  // Sync when external agent writes to this note
  useEffect(() => {
    if (data.content !== prevExternalContent.current) {
      prevExternalContent.current = data.content;
      setContent(data.content ?? "");
    }
  }, [data.content]);

  // Agent writing → scroll follows the tail, like tail -f
  useEffect(() => {
    if (isLive && textareaRef.current) {
      textareaRef.current.scrollTop = textareaRef.current.scrollHeight;
    }
  }, [content, isLive]);

  const commitTitle = useCallback(() => {
    setEditingTitle(false);
    const label = titleValue.trim() || "Note";
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
      className="flex flex-col w-full h-full rounded-xl overflow-hidden transition-shadow duration-150"
      style={{
        // Sticky note: warm paper (the one tinted surface — a metaphor everyone reads)
        background: "color-mix(in srgb, var(--queue) 6%, var(--surface))",
        border: "1px solid rgb(var(--queue-rgb) / 0.16)",
        boxShadow: isLive
          ? "0 0 0 1.5px rgb(var(--queue-rgb) / 0.8), 0 0 0 5px rgb(var(--queue-rgb) / 0.15), var(--shadow-tile)"
          : selected
          ? "var(--ring-accent), var(--shadow-tile)"
          : "var(--shadow-tile)",
      }}
    >
      <NodeResizer
        minWidth={180}
        minHeight={120}
        isVisible={selected}
lineStyle={{ borderColor: "transparent" }}
        handleStyle={{ width: 8, height: 8, borderRadius: 2, border: "1.5px solid var(--accent)", background: "var(--surface)" }}
      />

      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2 shrink-0 cursor-grab select-none"
        style={{ borderBottom: "1px solid rgb(var(--queue-rgb) / 0.12)" }}
      >
        <StickyNote size={12} className="text-queue shrink-0" />
        {editingTitle ? (
          <input
            autoFocus
            value={titleValue}
            onChange={(e) => setTitleValue(e.target.value)}
            onBlur={commitTitle}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitTitle();
              if (e.key === "Escape") { setTitleValue(data.label ?? "Note"); setEditingTitle(false); }
            }}
            onMouseDown={(e) => e.stopPropagation()}
            className="flex-1 min-w-0 bg-transparent text-queue text-xs outline-none border-b border-queue/30 nodrag"
            aria-label="Note title"
          />
        ) : (
          <span
            className="text-queue text-xs flex-1 truncate"
            title="Double-click to rename"
            onDoubleClick={(e) => { e.stopPropagation(); setEditingTitle(true); }}
          >
            {data.label ?? "Note"}
          </span>
        )}

        {/* Live indicator — pulsing when agent is writing */}
        {isLive && (
          <span className="flex items-center gap-1 text-[9px] text-queue animate-pulse">
            <Radio size={9} /> live
          </span>
        )}

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); handleCopy(); }}
          title="Copy content"
          aria-label="Copy content"
          className="text-ink-muted hover:text-queue transition-colors p-0.5 rounded nodrag"
        >
          {copied ? <Check size={12} className="text-ok" /> : <Copy size={12} />}
        </button>

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleClose}
          title="Close note"
          aria-label="Close note"
          className="text-ink-muted hover:text-danger-text transition-colors p-0.5 rounded nodrag"
        >
          <X size={12} />
        </button>
      </div>

      {/* Content */}
      <textarea
        ref={textareaRef}
        value={content}
        onChange={(e) => handleChange(e.target.value)}
        placeholder="Write here..."
        className="flex-1 resize-none bg-transparent text-ink-2 text-sm p-3 outline-none
          placeholder-ink-faint nodrag nowheel"
        style={{ fontFamily: "inherit", lineHeight: 1.6 }}
        onMouseDown={(e) => e.stopPropagation()}
      />

      <Handle type="target" position={Position.Left} className="narrater-handle" />
      <Handle type="source" position={Position.Right} className="narrater-handle" />
    </div>
  );
}

export default memo(NoteTile);

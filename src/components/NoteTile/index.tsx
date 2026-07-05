import { useState, useCallback, useEffect, useRef } from "react";
import { Handle, Position, NodeResizer, useReactFlow } from "@xyflow/react";
import { X, StickyNote, Radio } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { NoteNodeData } from "@/stores/canvas";
import type { Node, NodeProps } from "@xyflow/react";

type NoteNode = Node<NoteNodeData, "note">;

export default function NoteTile({ id, data, selected }: NodeProps<NoteNode>) {
  const [content, setContent] = useState(data.content ?? "");
  const { updateNodeData } = useReactFlow();
  const removeNode = useCanvasStore((s) => s.removeNode);
  const prevExternalContent = useRef(data.content);
  const isLive = data.isAgentLive as boolean | undefined;

  // Sync when external agent writes to this note
  useEffect(() => {
    if (data.content !== prevExternalContent.current) {
      prevExternalContent.current = data.content;
      setContent(data.content ?? "");
    }
  }, [data.content]);

  // Write local edits back to React Flow so they're included on save
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
        <StickyNote size={12} className="text-[#fbbf24]" />
        <span className="text-[#fbbf24] text-xs flex-1">Nota</span>

        {/* Live indicator — pulsing when agent is writing */}
        {isLive && (
          <span className="flex items-center gap-1 text-[9px] text-[#fbbf24] animate-pulse">
            <Radio size={9} /> ao vivo
          </span>
        )}

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleClose}
          className="text-[#666] hover:text-[#f87171] transition-colors p-0.5 rounded nodrag"
        >
          <X size={12} />
        </button>
      </div>

      {/* Content */}
      <textarea
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

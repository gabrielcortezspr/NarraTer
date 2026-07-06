import { memo, useState, useCallback } from "react";
import { Handle, Position, NodeResizer } from "@xyflow/react";
import { X } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { TextNodeData } from "@/stores/canvas";
import type { Node, NodeProps } from "@xyflow/react";

type TextNode = Node<TextNodeData, "text">;

// Bloco de texto leve — anotação direta no canvas, sem o peso visual da nota.
function TextTile({ id, data, selected }: NodeProps<TextNode>) {
  const [text, setText] = useState(data.text ?? "");
  const [hovered, setHovered] = useState(false);
  const updateNodeData = useCanvasStore((s) => s.updateNodeData);
  const removeNode = useCanvasStore((s) => s.removeNode);

  const handleChange = useCallback(
    (value: string) => {
      setText(value);
      updateNodeData(id, { text: value });
    },
    [id, updateNodeData]
  );

  return (
    <div
      className="relative w-full h-full rounded-lg"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        background: "transparent",
        boxShadow: selected
          ? "0 0 0 1px #8b5cf6"
          : hovered
          ? "0 0 0 1px #2a2a2a"
          : "none",
        transition: "box-shadow 0.15s ease",
      }}
    >
      <NodeResizer
        minWidth={120}
        minHeight={40}
        isVisible={selected}
        lineStyle={{ borderColor: "#8b5cf6" }}
        handleStyle={{ borderColor: "#8b5cf6", background: "#0d0d0d" }}
      />

      {(hovered || selected) && (
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); removeNode(id); }}
          className="absolute -top-2 -right-2 z-10 w-5 h-5 flex items-center justify-center rounded-full
            bg-[#1a1a1a] border border-[#2a2a2a] text-[#666] hover:text-[#f87171] transition-colors nodrag"
        >
          <X size={10} />
        </button>
      )}

      <textarea
        value={text}
        onChange={(e) => handleChange(e.target.value)}
        placeholder="Texto…"
        className="w-full h-full resize-none bg-transparent text-[#e5e7eb] p-2 outline-none
          placeholder-[#3a3a3a] nodrag nowheel"
        style={{ fontSize: 17, lineHeight: 1.5, fontFamily: "inherit" }}
        onMouseDown={(e) => e.stopPropagation()}
      />

      <Handle type="target" position={Position.Left} style={{ background: "#4a4a4a", border: "none", width: 6, height: 6 }} />
      <Handle type="source" position={Position.Right} style={{ background: "#4a4a4a", border: "none", width: 6, height: 6 }} />
    </div>
  );
}

export default memo(TextTile);

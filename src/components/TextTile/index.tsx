import { memo, useState, useCallback } from "react";
import NodeHandles from "@/components/ui/NodeHandles";
import { NodeResizer } from "@xyflow/react";
import { X } from "lucide-react";
import { useCanvasStore } from "@/stores/canvas";
import type { TextNodeData } from "@/stores/canvas";
import type { Node, NodeProps } from "@xyflow/react";

type TextNode = Node<TextNodeData, "text">;

// Lightweight text block — direct annotation on the canvas, without the note's visual weight.
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
          ? "var(--ring-accent)"
          : hovered
          ? "0 0 0 1px var(--line-strong)"
          : "none",
        transition: "box-shadow 0.15s ease",
      }}
    >
      <NodeResizer
        minWidth={120}
        minHeight={40}
        isVisible={selected}
lineStyle={{ borderColor: "transparent" }}
        handleStyle={{ width: 8, height: 8, borderRadius: 2, border: "1.5px solid var(--accent)", background: "var(--surface)" }}
      />

      {(hovered || selected) && (
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); removeNode(id); }}
          className="absolute -top-2 -right-2 z-10 w-5 h-5 flex items-center justify-center rounded-full
            bg-surface border border-line text-ink-muted hover:text-danger-text transition-colors nodrag"
        >
          <X size={10} />
        </button>
      )}

      <textarea
        value={text}
        onChange={(e) => handleChange(e.target.value)}
        placeholder="Text…"
        className="w-full h-full resize-none bg-transparent text-ink p-2 outline-none
          placeholder-ink-faint nodrag nowheel"
        style={{ fontSize: 17, lineHeight: 1.5, fontFamily: "inherit" }}
        onMouseDown={(e) => e.stopPropagation()}
      />

      {/* Handles on all four sides, hidden until hover (D18) */}

      <NodeHandles />
    </div>
  );
}

export default memo(TextTile);

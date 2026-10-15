import { memo } from "react";
import { getBezierPath, type EdgeProps } from "@xyflow/react";

// Agent → note: a quiet dashed thread (docs/design.md, "O grafo"). It is a
// reference, not a conversation, so it never animates; it brightens on hover.
function AgentNoteEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  selected,
}: EdgeProps) {
  const [edgePath] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  return (
    <g>
      <path d={edgePath} stroke="transparent" strokeWidth={14} fill="none" />
      <path
        id={id}
        className="narrater-edge-line"
        d={edgePath}
        stroke={selected ? "var(--accent)" : "rgb(var(--text-rgb))"}
        strokeWidth={1.25}
        strokeDasharray="3 5"
        strokeLinecap="round"
        fill="none"
        style={{ opacity: selected ? 0.7 : 0.14, transition: "opacity 200ms var(--ease)" }}
      />
    </g>
  );
}

export default memo(AgentNoteEdge);

import { memo } from "react";
import type { EdgeProps } from "@xyflow/react";
import { useRope } from "@/lib/useRope";

// Agent → note: a quiet dashed thread (docs/design.md, "O grafo"). It is a
// reference, not a conversation, so it never animates; it brightens on hover.
function AgentNoteEdge({ id, source, target, selected }: EdgeProps) {
  // A rope too (D18): anchored on the facing sides, around other nodes
  const { ready, d: edgePath, pathRef } = useRope(source, target);
  if (!ready) return null;

  return (
    <g>
      <path ref={pathRef} d={edgePath} stroke="transparent" strokeWidth={14} fill="none" />
      <path
        ref={pathRef}
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

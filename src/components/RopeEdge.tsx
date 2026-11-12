import { memo } from "react";
import type { EdgeProps } from "@xyflow/react";
import { useRope } from "@/lib/useRope";

// Plain connection (terminal ↔ portal, files, attachment…): a quiet solid rope
// (D18), the same physics as the agent edges, never animated.
function RopeEdge({ id, source, target, selected }: EdgeProps) {
  const { ready, d, pathRef } = useRope(source, target);
  if (!ready) return null;
  return (
    <g>
      <path ref={pathRef} d={d} stroke="transparent" strokeWidth={14} fill="none" />
      <path
        ref={pathRef}
        id={id}
        className="narrater-edge-line"
        d={d}
        stroke={selected ? "var(--accent)" : "rgb(var(--text-rgb))"}
        strokeWidth={1.25}
        strokeLinecap="round"
        fill="none"
        style={{ opacity: selected ? 0.7 : 0.16, transition: "opacity 200ms var(--ease)" }}
      />
    </g>
  );
}

export default memo(RopeEdge);

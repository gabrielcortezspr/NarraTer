import { memo } from "react";
import { getBezierPath, type EdgeProps } from "@xyflow/react";

function AgentNoteEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
}: EdgeProps) {
  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  return (
    <g>
      {/* Glow */}
      <path
        d={edgePath}
        stroke="#fbbf24"
        strokeWidth={6}
        fill="none"
        opacity={0.08}
      />
      {/* Main line */}
      <path
        id={id}
        d={edgePath}
        stroke="#fbbf24"
        strokeWidth={1.5}
        strokeDasharray="6 4"
        fill="none"
        opacity={0.7}
      />
      {/* Label */}
      <g transform={`translate(${labelX},${labelY})`}>
        <rect x={-32} y={-10} width={64} height={20} rx={10} fill="#1e1a0e" stroke="#fbbf2440" strokeWidth={1} />
        <text
          textAnchor="middle"
          dominantBaseline="middle"
          style={{ fontSize: 9, fill: "#fbbf24", fontFamily: "inherit", opacity: 0.9 }}
        >
          ✍ agent→nota
        </text>
      </g>
    </g>
  );
}

export default memo(AgentNoteEdge);

import { getBezierPath, type EdgeProps } from "@xyflow/react";

export default function AgentPipeEdge({
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

  const gradientId = `pipe-gradient-${id}`;
  const glowId = `pipe-glow-${id}`;

  return (
    <g>
      <defs>
        <linearGradient id={gradientId} x1="0%" y1="0%" x2="100%" y2="0%">
          <stop offset="0%" stopColor="#8b5cf6" />
          <stop offset="50%" stopColor="#a855f7" />
          <stop offset="100%" stopColor="#8b5cf6" />
        </linearGradient>
        <filter id={glowId}>
          <feGaussianBlur stdDeviation="2" result="coloredBlur" />
          <feMerge>
            <feMergeNode in="coloredBlur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      {/* Outer glow */}
      <path
        d={edgePath}
        stroke="#8b5cf6"
        strokeWidth={8}
        fill="none"
        opacity={0.12}
      />

      {/* Animated flow line */}
      <path
        id={id}
        d={edgePath}
        stroke={`url(#${gradientId})`}
        strokeWidth={2}
        fill="none"
        strokeDasharray="8 4"
        opacity={0.85}
        filter={`url(#${glowId})`}
        style={{
          animation: "narrater-pipe-flow 1.2s linear infinite",
        }}
      />

      {/* Label */}
      <g transform={`translate(${labelX},${labelY})`}>
        <rect x={-38} y={-11} width={76} height={22} rx={11} fill="#110d1e" stroke="#8b5cf640" strokeWidth={1} />
        <text
          textAnchor="middle"
          dominantBaseline="middle"
          style={{ fontSize: 9, fill: "#a78bfa", fontFamily: "inherit", fontWeight: 500, letterSpacing: "0.03em" }}
        >
          ⟶ agent pipe
        </text>
      </g>

      <style>{`
        @keyframes narrater-pipe-flow {
          from { stroke-dashoffset: 0; }
          to { stroke-dashoffset: -24; }
        }
      `}</style>
    </g>
  );
}

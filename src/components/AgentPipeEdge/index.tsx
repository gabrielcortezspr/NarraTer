import { memo } from "react";
import { getBezierPath, type EdgeProps } from "@xyflow/react";
import { useTerminalsStore } from "@/stores/terminals";

function AgentPipeEdge({
  id,
  source,
  target,
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

  const pending = useTerminalsStore((s) => s.queues[target] ?? 0);
  const targetStatus = useTerminalsStore((s) => s.sessions[target]?.status);
  const sourceStatus = useTerminalsStore((s) => s.sessions[source]?.status);
  // Flow animation when a message is queued or either endpoint is working
  const active = pending > 0 || targetStatus === "running" || sourceStatus === "running";

  const gradientId = `pipe-gradient-${id}`;
  const glowId = `pipe-glow-${id}`;
  const label = pending > 0 ? `⧗ ${pending} na fila` : "⟶ agent pipe";
  const labelColor = pending > 0 ? "#fbbf24" : "#a78bfa";
  const labelStroke = pending > 0 ? "#fbbf2440" : "#8b5cf640";

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
        opacity={active ? 0.18 : 0.08}
      />

      {/* Flow line — animated only while there is activity on the route */}
      <path
        id={id}
        d={edgePath}
        stroke={`url(#${gradientId})`}
        strokeWidth={2}
        fill="none"
        strokeDasharray="8 4"
        opacity={active ? 0.9 : 0.5}
        filter={active ? `url(#${glowId})` : undefined}
        style={active ? { animation: "narrater-pipe-flow 1.2s linear infinite" } : undefined}
      />

      {/* Label */}
      <g transform={`translate(${labelX},${labelY})`}>
        <rect x={-38} y={-11} width={76} height={22} rx={11} fill="#110d1e" stroke={labelStroke} strokeWidth={1} />
        <text
          textAnchor="middle"
          dominantBaseline="middle"
          style={{ fontSize: 9, fill: labelColor, fontFamily: "inherit", fontWeight: 500, letterSpacing: "0.03em" }}
        >
          {label}
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

export default memo(AgentPipeEdge);

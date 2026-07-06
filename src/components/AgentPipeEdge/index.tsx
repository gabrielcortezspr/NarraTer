import { memo, useEffect, useState } from "react";
import { getBezierPath, type EdgeProps } from "@xyflow/react";
import { useTerminalsStore } from "@/stores/terminals";
import { pairKey, useLedgerStore } from "@/stores/ledger";

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

  const pending = useTerminalsStore((s) => (s.queues[target] ?? []).length);
  const targetStatus = useTerminalsStore((s) => s.sessions[target]?.status);
  const sourceStatus = useTerminalsStore((s) => s.sessions[source]?.status);
  // Flow animation when a message is queued or either endpoint is working
  const active = pending > 0 || targetStatus === "running" || sourceStatus === "running";

  // Pulso quando uma mensagem passa pela rota (ledger → lastActivity do par)
  const activity = useLedgerStore((s) => s.lastActivity[pairKey(source, target)]);
  const [pulse, setPulse] = useState(false);
  useEffect(() => {
    if (!activity) return;
    setPulse(true);
    const t = setTimeout(() => setPulse(false), 1500);
    return () => clearTimeout(t);
  }, [activity]);

  const gradientId = `pipe-gradient-${id}`;
  const glowId = `pipe-glow-${id}`;
  const label = pulse ? "✉ mensagem" : pending > 0 ? `⧗ ${pending} na fila` : "⟶ agent pipe";
  const labelColor = pulse ? "#4ade80" : pending > 0 ? "#fbbf24" : "#a78bfa";
  const labelStroke = pulse ? "#4ade8040" : pending > 0 ? "#fbbf2440" : "#8b5cf640";

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
        className="narrater-edge-line"
        d={edgePath}
        stroke={pulse ? "#4ade80" : `url(#${gradientId})`}
        strokeWidth={pulse ? 3 : 2}
        fill="none"
        strokeDasharray="8 4"
        opacity={active || pulse ? 0.9 : 0.5}
        filter={active || pulse ? `url(#${glowId})` : undefined}
        style={
          active || pulse
            ? { animation: "narrater-pipe-flow 1.2s linear infinite", transition: "stroke-width 0.2s" }
            : undefined
        }
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

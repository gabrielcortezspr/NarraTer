import { memo, useEffect, useState } from "react";
import { getBezierPath, type EdgeProps } from "@xyflow/react";
import { useTerminalsStore } from "@/stores/terminals";
import { useCanvasStore } from "@/stores/canvas";
import { pairKey, useLedgerStore } from "@/stores/ledger";

// A "line of dialogue" between two agents (docs/design.md, "O grafo"):
// - at rest: a thin, calm curve with no label;
// - an endpoint working: the curve brightens and a slow accent flow runs on it;
// - a message travels: a comet of light crosses the curve;
// - messages queued: a yellow chip with the count at the middle;
// - hover: the names of both ends ("A ↔ B").
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
  selected,
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
  const working = targetStatus === "running" || sourceStatus === "running";
  const names = useCanvasStore((s) => {
    const label = (nid: string) => {
      const n = s.nodes.find((x) => x.id === nid);
      return (n?.data as { label?: string } | undefined)?.label ?? "";
    };
    return `${label(source)} ↔ ${label(target)}`;
  });

  // A message travelled the route (ledger → the pair's lastActivity)
  const activity = useLedgerStore((s) => s.lastActivity[pairKey(source, target)]);
  const [pulse, setPulse] = useState(0);
  useEffect(() => {
    if (!activity) return;
    setPulse((p) => p + 1);
  }, [activity]);
  const [pulsing, setPulsing] = useState(false);
  useEffect(() => {
    if (!pulse) return;
    setPulsing(true);
    const t = setTimeout(() => setPulsing(false), 1100);
    return () => clearTimeout(t);
  }, [pulse]);

  const glowId = `pipe-glow-${id}`;
  const baseOpacity = selected ? 0.7 : pulsing ? 0.6 : working ? 0.35 : 0.18;
  const chipW = 26 + String(pending).length * 7;

  return (
    <g className="narrater-pipe">
      <defs>
        <filter id={glowId} x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="2.5" result="blur" />
          <feMerge>
            <feMergeNode in="blur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      {/* Wide invisible path: easy to hover and click */}
      <path d={edgePath} stroke="transparent" strokeWidth={16} fill="none" />

      {/* The line itself */}
      <path
        id={id}
        className="narrater-edge-line"
        d={edgePath}
        stroke={selected ? "var(--accent)" : "rgb(var(--text-rgb))"}
        strokeWidth={1.5}
        strokeLinecap="round"
        fill="none"
        style={{ opacity: baseOpacity, transition: "opacity 300ms var(--ease), stroke 150ms" }}
      />

      {/* An endpoint is working: slow accent flow */}
      {working && !pulsing && (
        <path
          d={edgePath}
          pathLength={1}
          stroke="var(--accent)"
          strokeWidth={1.5}
          strokeLinecap="round"
          strokeDasharray="0.02 0.06"
          fill="none"
          className="narrater-pipe-flow"
          style={{ opacity: 0.7 }}
        />
      )}

      {/* A message travels: comet of light along the curve */}
      {pulsing && (
        <path
          key={pulse}
          d={edgePath}
          pathLength={1}
          stroke="var(--accent)"
          strokeWidth={3}
          strokeLinecap="round"
          strokeDasharray="0.18 1"
          fill="none"
          filter={`url(#${glowId})`}
          className="narrater-pipe-comet"
        />
      )}

      {/* Queued messages: count chip at the middle */}
      {pending > 0 && (
        <g transform={`translate(${labelX},${labelY})`}>
          <title>{`${pending} message${pending > 1 ? "s" : ""} waiting in the queue`}</title>
          <rect
            x={-chipW / 2}
            y={-10}
            width={chipW}
            height={20}
            rx={10}
            fill="rgb(var(--raised-rgb))"
            stroke="rgb(var(--queue-rgb) / 0.45)"
          />
          <circle cx={-chipW / 2 + 9} cy={0} r={3} fill="var(--queue)" />
          <text
            x={5}
            textAnchor="middle"
            dominantBaseline="central"
            style={{ fontSize: 11, fill: "var(--queue)", fontWeight: 600, fontVariantNumeric: "tabular-nums" }}
          >
            {pending}
          </text>
        </g>
      )}

      {/* Hover: who talks to whom */}
      {pending === 0 && names.trim() !== "↔" && (
        <g className="narrater-edge-label" transform={`translate(${labelX},${labelY})`}>
          <rect
            x={-(names.length * 3.3 + 12)}
            y={-11}
            width={names.length * 6.6 + 24}
            height={22}
            rx={11}
            fill="rgb(var(--raised-rgb))"
            stroke="var(--line-strong)"
          />
          <text
            textAnchor="middle"
            dominantBaseline="central"
            style={{ fontSize: 11, fill: "var(--text-2)", fontWeight: 500 }}
          >
            {names}
          </text>
        </g>
      )}
    </g>
  );
}

export default memo(AgentPipeEdge);

import { useCallback, useRef } from "react";
import { useViewport } from "@xyflow/react";
import { getStroke } from "perfect-freehand";
import { useSketchStore, type Stroke } from "@/stores/sketch";

function strokeToPath(points: number[][], size: number): string {
  const outline = getStroke(points, {
    size,
    thinning: 0.5,
    smoothing: 0.5,
    streamline: 0.5,
  });
  if (outline.length < 2) return "";
  const [first, ...rest] = outline;
  const d = [`M ${first[0].toFixed(2)} ${first[1].toFixed(2)}`];
  for (let i = 0; i < rest.length - 1; i++) {
    const [x0, y0] = rest[i];
    const [x1, y1] = rest[i + 1];
    d.push(`Q ${x0.toFixed(2)} ${y0.toFixed(2)} ${((x0 + x1) / 2).toFixed(2)} ${((y0 + y1) / 2).toFixed(2)}`);
  }
  d.push("Z");
  return d.join(" ");
}

interface Props {
  active: boolean;
}

export default function SketchLayer({ active }: Props) {
  const { x: vpX, y: vpY, zoom } = useViewport();
  const { strokes, currentStroke, color, size, addPoint, commitStroke, cancelStroke } =
    useSketchStore();
  const svgRef = useRef<SVGSVGElement>(null);
  const isPointerDown = useRef(false);

  const toCanvas = useCallback(
    (ex: number, ey: number): number[] => {
      if (!svgRef.current) return [0, 0];
      const rect = svgRef.current.getBoundingClientRect();
      return [(ex - rect.left - vpX) / zoom, (ey - rect.top - vpY) / zoom];
    },
    [vpX, vpY, zoom]
  );

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (!active) return;
      e.preventDefault();
      isPointerDown.current = true;
      (e.target as SVGElement).setPointerCapture(e.pointerId);
      addPoint([...toCanvas(e.clientX, e.clientY), e.pressure]);
    },
    [active, toCanvas, addPoint]
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!active || !isPointerDown.current) return;
      e.preventDefault();
      addPoint([...toCanvas(e.clientX, e.clientY), e.pressure]);
    },
    [active, toCanvas, addPoint]
  );

  const handlePointerUp = useCallback(
    (e: React.PointerEvent) => {
      if (!active) return;
      e.preventDefault();
      isPointerDown.current = false;
      commitStroke();
    },
    [active, commitStroke]
  );

  const handlePointerLeave = useCallback(() => {
    if (isPointerDown.current) {
      isPointerDown.current = false;
      cancelStroke();
    }
  }, [cancelStroke]);

  return (
    <svg
      ref={svgRef}
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        pointerEvents: active ? "all" : "none",
        zIndex: active ? 20 : 5,
        cursor: active ? "crosshair" : "default",
        touchAction: "none",
      }}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerLeave={handlePointerLeave}
    >
      <g transform={`translate(${vpX},${vpY}) scale(${zoom})`}>
        {strokes.map((stroke: Stroke, i: number) => {
          const d = strokeToPath(stroke.points, stroke.size);
          return d ? (
            <path key={i} d={d} fill={stroke.color} opacity={0.85} />
          ) : null;
        })}
        {currentStroke.length > 1 && (
          <path
            d={strokeToPath(currentStroke, size)}
            fill={color}
            opacity={0.85}
          />
        )}
      </g>
    </svg>
  );
}

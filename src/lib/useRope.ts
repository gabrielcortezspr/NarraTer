import { useCallback, useEffect, useMemo, useRef } from "react";
import { useInternalNode, useStore, type InternalNode } from "@xyflow/react";
import {
  anchors,
  createRope,
  midpoint,
  pointCount,
  resample,
  ropePath,
  stepRope,
  ROPE,
  type Rect,
  type Rope,
  type Vec,
} from "@/lib/rope";

// Drives the rope of one connection (src/lib/rope.ts, D18): reads both nodes'
// boxes and every other node as an obstacle, simulates only while something
// moves, and writes the path straight into the SVG elements registered with
// `pathRef` (no React render per frame).

const rectOf = (n: InternalNode | undefined): Rect | null => {
  if (!n) return null;
  const w = n.measured?.width ?? n.width ?? 0;
  const h = n.measured?.height ?? n.height ?? 0;
  if (!w || !h) return null;
  const p = n.internals.positionAbsolute;
  return { x: p.x, y: p.y, w, h };
};

const reducedMotion = () =>
  typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

/** Pre-solve steps when a rope is created or the motion is reduced. */
const SOLVE_STEPS = 160;

export function useRope(source: string, target: string) {
  const src = rectOf(useInternalNode(source));
  const tgt = rectOf(useInternalNode(target));

  // Every node's box, as a stable string key (re-renders only when one moves)
  const obstacleKey = useStore((s) => {
    let key = "";
    s.nodeLookup.forEach((n) => {
      if (n.hidden) return;
      const r = rectOf(n);
      if (r) key += `${n.id}:${Math.round(r.x)},${Math.round(r.y)},${Math.round(r.w)},${Math.round(r.h)};`;
    });
    return key;
  });
  const obstacles = useMemo<Rect[]>(
    () =>
      obstacleKey
        .split(";")
        .filter(Boolean)
        .map((entry) => {
          const [, nums] = entry.split(":");
          const [x, y, w, h] = nums.split(",").map(Number);
          return { x, y, w, h };
        }),
    [obstacleKey]
  );

  const rope = useRef<Rope | null>(null);
  const paths = useRef(new Set<SVGPathElement>());
  const mids = useRef(new Set<SVGGElement>());
  const raf = useRef<number | null>(null);
  const inputs = useRef<{ a: Vec; b: Vec; obstacles: Rect[] } | null>(null);

  const draw = useCallback(() => {
    const r = rope.current;
    if (!r) return;
    const d = ropePath(r.pts);
    paths.current.forEach((el) => (el.isConnected ? el.setAttribute("d", d) : paths.current.delete(el)));
    const m = midpoint(r.pts);
    mids.current.forEach((el) =>
      el.isConnected ? el.setAttribute("transform", `translate(${m.x.toFixed(1)},${m.y.toFixed(1)})`) : mids.current.delete(el)
    );
  }, []);

  // Anchors and obstacles for this render; (re)start the simulation on change
  if (src && tgt) {
    const [a, b] = anchors(src, tgt);
    inputs.current = { a, b, obstacles };
    if (!rope.current) {
      rope.current = createRope(a, b);
      for (let i = 0; i < SOLVE_STEPS; i++) stepRope(rope.current, a, b, obstacles);
    } else {
      const n = pointCount(a, b);
      if (Math.abs(n - rope.current.pts.length) > 2) rope.current = resample(rope.current, n);
    }
  }
  const changeKey = src && tgt ? `${src.x},${src.y},${src.w},${src.h}|${tgt.x},${tgt.y},${tgt.w},${tgt.h}|${obstacleKey}` : "";

  useEffect(() => {
    if (!rope.current || !inputs.current) return;
    if (reducedMotion()) {
      const { a, b, obstacles: obs } = inputs.current;
      for (let i = 0; i < SOLVE_STEPS; i++) stepRope(rope.current, a, b, obs);
      draw();
      return;
    }
    let calm = 0;
    const tick = () => {
      const r = rope.current;
      const cur = inputs.current;
      if (!r || !cur) return;
      const moved = stepRope(r, cur.a, cur.b, cur.obstacles);
      draw();
      calm = moved < ROPE.restEpsilon ? calm + 1 : 0;
      raf.current = calm < ROPE.settleFrames ? requestAnimationFrame(tick) : null;
    };
    if (raf.current == null) raf.current = requestAnimationFrame(tick);
  }, [changeKey, draw]);

  useEffect(
    () => () => {
      if (raf.current != null) cancelAnimationFrame(raf.current);
    },
    []
  );

  const pathRef = useCallback((el: SVGPathElement | null) => {
    if (el) paths.current.add(el);
  }, []);
  const midRef = useCallback((el: SVGGElement | null) => {
    if (el) mids.current.add(el);
  }, []);

  const d = rope.current ? ropePath(rope.current.pts) : "";
  const mid = rope.current ? midpoint(rope.current.pts) : { x: 0, y: 0 };
  return { ready: !!(src && tgt), d, mid, pathRef, midRef };
}

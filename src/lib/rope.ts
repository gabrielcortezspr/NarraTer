// Rope physics for the connections between nodes (docs/design.md, "O grafo"; D18).
//
// A connection is a rope: its ends are pinned to the nodes' borders at the
// point facing the other node (any side), it hangs with a little slack, it
// sways when a node is dragged, and it is pushed out of other nodes' boxes so
// it goes around a terminal instead of disappearing under it.
//
// Verlet integration with distance constraints, solved in the canvas'
// coordinate space. Pure functions: the edge components own the state and
// only step while something moves.

export interface Vec {
  x: number;
  y: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Rope {
  pts: Vec[];
  prev: Vec[];
  /** Rest length of each segment. */
  seg: number;
}

/** Tunables (canvas units per frame at ~60 fps). */
export const ROPE = {
  /** Rope length over the straight distance: the sag. */
  slack: 1.05,
  gravity: 0.22,
  damping: 0.9,
  iterations: 12,
  /** Clearance kept around obstacles. */
  pad: 16,
  /** Points per canvas unit of distance, clamped. */
  spacing: 26,
  minPoints: 6,
  maxPoints: 36,
  /** Below this max movement per frame, for SETTLE_FRAMES frames, the rope sleeps. */
  restEpsilon: 0.04,
  settleFrames: 18,
};

export const center = (r: Rect): Vec => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });

/**
 * Where a line from the rect's centre towards `to` leaves the rect: the
 * rope's anchor, on whichever side faces the other node.
 */
export function borderPoint(r: Rect, to: Vec): Vec {
  const c = center(r);
  const dx = to.x - c.x;
  const dy = to.y - c.y;
  if (dx === 0 && dy === 0) return { x: c.x, y: r.y };
  const sx = dx !== 0 ? r.w / 2 / Math.abs(dx) : Infinity;
  const sy = dy !== 0 ? r.h / 2 / Math.abs(dy) : Infinity;
  const s = Math.min(sx, sy);
  return { x: c.x + dx * s, y: c.y + dy * s };
}

/** Both anchors of a connection between two rects. */
export function anchors(a: Rect, b: Rect): [Vec, Vec] {
  return [borderPoint(a, center(b)), borderPoint(b, center(a))];
}

const dist = (a: Vec, b: Vec) => Math.hypot(b.x - a.x, b.y - a.y);

export function pointCount(a: Vec, b: Vec): number {
  return Math.max(ROPE.minPoints, Math.min(ROPE.maxPoints, Math.round(dist(a, b) / ROPE.spacing)));
}

/** A rope laid straight between two points. */
export function createRope(a: Vec, b: Vec): Rope {
  const n = pointCount(a, b);
  const pts = Array.from({ length: n }, (_, i) => ({
    x: a.x + ((b.x - a.x) * i) / (n - 1),
    y: a.y + ((b.y - a.y) * i) / (n - 1),
  }));
  return { pts, prev: pts.map((p) => ({ ...p })), seg: (dist(a, b) * ROPE.slack) / (n - 1) };
}

/** Pushes `p` out of `r` (inflated by pad) through the nearest side. */
function pushOut(p: Vec, r: Rect, pad: number): boolean {
  const x0 = r.x - pad;
  const y0 = r.y - pad;
  const x1 = r.x + r.w + pad;
  const y1 = r.y + r.h + pad;
  if (p.x <= x0 || p.x >= x1 || p.y <= y0 || p.y >= y1) return false;
  const dl = p.x - x0;
  const dr = x1 - p.x;
  const dt = p.y - y0;
  const db = y1 - p.y;
  const m = Math.min(dl, dr, dt, db);
  if (m === dl) p.x = x0;
  else if (m === dr) p.x = x1;
  else if (m === dt) p.y = y0;
  else p.y = y1;
  return true;
}

/**
 * One simulation frame. Ends are pinned to `a` and `b`; `obstacles` are the
 * boxes the rope must stay out of (the two end nodes included: the rope
 * leaves its node instead of sagging back over it). Returns the largest
 * movement of any point, to know when the rope is at rest.
 */
export function stepRope(rope: Rope, a: Vec, b: Vec, obstacles: Rect[]): number {
  const { pts, prev } = rope;
  const n = pts.length;
  // Keep the slack proportional to the current span (nodes move)
  rope.seg = (dist(a, b) * ROPE.slack) / (n - 1);

  let moved = 0;
  for (let i = 1; i < n - 1; i++) {
    const p = pts[i];
    const vx = (p.x - prev[i].x) * ROPE.damping;
    const vy = (p.y - prev[i].y) * ROPE.damping;
    prev[i] = { x: p.x, y: p.y };
    p.x += vx;
    p.y += vy + ROPE.gravity;
  }
  pts[0] = { ...a };
  pts[n - 1] = { ...b };

  for (let k = 0; k < ROPE.iterations; k++) {
    for (let i = 0; i < n - 1; i++) {
      const p = pts[i];
      const q = pts[i + 1];
      const dx = q.x - p.x;
      const dy = q.y - p.y;
      const d = Math.hypot(dx, dy) || 1e-6;
      const diff = (d - rope.seg) / d;
      const pinnedP = i === 0;
      const pinnedQ = i + 1 === n - 1;
      if (pinnedP && pinnedQ) continue;
      const wp = pinnedP ? 0 : pinnedQ ? 1 : 0.5;
      const wq = pinnedQ ? 0 : pinnedP ? 1 : 0.5;
      p.x += dx * diff * wp;
      p.y += dy * diff * wp;
      q.x -= dx * diff * wq;
      q.y -= dy * diff * wq;
    }
    // Interior points stay out of every box (ends sit on their node's border)
    for (let i = 1; i < n - 1; i++) {
      for (const r of obstacles) pushOut(pts[i], r, ROPE.pad);
    }
  }

  for (let i = 1; i < n - 1; i++) {
    moved = Math.max(moved, Math.abs(pts[i].x - prev[i].x) + Math.abs(pts[i].y - prev[i].y));
  }
  return moved;
}

/** Re-samples a rope to a new number of points (the span changed a lot). */
export function resample(rope: Rope, n: number): Rope {
  const src = rope.pts;
  const out: Vec[] = [];
  for (let i = 0; i < n; i++) {
    const t = (i / (n - 1)) * (src.length - 1);
    const j = Math.min(src.length - 2, Math.floor(t));
    const f = t - j;
    out.push({ x: src[j].x + (src[j + 1].x - src[j].x) * f, y: src[j].y + (src[j + 1].y - src[j].y) * f });
  }
  return { pts: out, prev: out.map((p) => ({ ...p })), seg: rope.seg };
}

/** Smooth SVG path through the points (Catmull-Rom as cubic Béziers). */
export function ropePath(pts: Vec[]): string {
  if (pts.length < 2) return "";
  const f = (v: number) => v.toFixed(1);
  let d = `M${f(pts[0].x)},${f(pts[0].y)}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[Math.min(pts.length - 1, i + 2)];
    const c1 = { x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6 };
    const c2 = { x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6 };
    d += ` C${f(c1.x)},${f(c1.y)} ${f(c2.x)},${f(c2.y)} ${f(p2.x)},${f(p2.y)}`;
  }
  return d;
}

/** Point halfway along the rope (for the queue chip and the hover label). */
export function midpoint(pts: Vec[]): Vec {
  let total = 0;
  for (let i = 0; i < pts.length - 1; i++) total += dist(pts[i], pts[i + 1]);
  let acc = 0;
  for (let i = 0; i < pts.length - 1; i++) {
    const d = dist(pts[i], pts[i + 1]);
    if (acc + d >= total / 2) {
      const t = d ? (total / 2 - acc) / d : 0;
      return { x: pts[i].x + (pts[i + 1].x - pts[i].x) * t, y: pts[i].y + (pts[i + 1].y - pts[i].y) * t };
    }
    acc += d;
  }
  return pts[Math.floor(pts.length / 2)];
}

/** Does the segment from p to q cross the rect (inflated by pad)? Liang–Barsky. */
export function segmentHitsRect(p: Vec, q: Vec, r: Rect, pad = 0): boolean {
  const x0 = r.x - pad;
  const y0 = r.y - pad;
  const x1 = r.x + r.w + pad;
  const y1 = r.y + r.h + pad;
  let t0 = 0;
  let t1 = 1;
  const dx = q.x - p.x;
  const dy = q.y - p.y;
  const clip = (pp: number, qq: number) => {
    if (pp === 0) return qq >= 0;
    const t = qq / pp;
    if (pp < 0) {
      if (t > t1) return false;
      if (t > t0) t0 = t;
    } else {
      if (t < t0) return false;
      if (t < t1) t1 = t;
    }
    return true;
  };
  return clip(-dx, p.x - x0) && clip(dx, x1 - p.x) && clip(-dy, p.y - y0) && clip(dy, y1 - p.y) && t0 <= t1;
}

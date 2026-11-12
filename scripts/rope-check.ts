// Property check of the rope physics (src/lib/rope.ts), run with:
//   node --experimental-strip-types scripts/rope-check.ts
// A node sits between two connected nodes; the rope must go around it,
// rest within a bounded number of frames, and anchor on the facing sides.
import { anchors, createRope, stepRope, ROPE, segmentHitsRect, type Rect } from "../src/lib/rope.ts";

const A: Rect = { x: 0, y: 0, w: 400, h: 260 };
const B: Rect = { x: 1000, y: 0, w: 400, h: 260 };
const scenarios: Array<[string, Rect[]]> = [
  ["nada no caminho", []],
  ["terminal no meio", [{ x: 560, y: -40, w: 280, h: 340 }]],
  ["dois terminais", [{ x: 480, y: -60, w: 180, h: 300 }, { x: 740, y: 40, w: 180, h: 300 }]],
];
let failed = 0;
for (const [name, obstacles] of scenarios) {
  const [a, b] = anchors(A, B);
  const rope = createRope(a, b);
  const boxes = [A, B, ...obstacles];
  let frames = 0;
  let calm = 0;
  for (; frames < 2000 && calm < ROPE.settleFrames; frames++) {
    const moved = stepRope(rope, a, b, boxes);
    calm = moved < ROPE.restEpsilon ? calm + 1 : 0;
  }
  // Interior points outside every obstacle (with a 1-unit tolerance)
  const inside = rope.pts.slice(1, -1).filter((p) =>
    obstacles.some((r) => p.x > r.x - ROPE.pad + 1 && p.x < r.x + r.w + ROPE.pad - 1 && p.y > r.y - ROPE.pad + 1 && p.y < r.y + r.h + ROPE.pad - 1)
  ).length;
  // Segments crossing an obstacle (the drawn line, not just the points)
  let crossings = 0;
  for (let i = 0; i < rope.pts.length - 1; i++) {
    if (obstacles.some((r) => segmentHitsRect(rope.pts[i], rope.pts[i + 1], r))) crossings++;
  }
  const anchorOk = a.x === A.x + A.w && b.x === B.x;
  const ok = inside === 0 && crossings === 0 && calm >= ROPE.settleFrames && anchorOk;
  if (!ok) failed++;
  console.log(
    `${ok ? "ok  " : "FAIL"} ${name.padEnd(18)} pontos=${rope.pts.length} quadros-até-assentar=${frames} pontos-dentro=${inside} segmentos-cruzando=${crossings} âncoras-nas-bordas-certas=${anchorOk}`
  );
}
process.exit(failed ? 1 : 0);

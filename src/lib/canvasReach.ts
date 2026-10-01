import type { AppEdge, AppNode } from "@/stores/canvas";

// What a terminal can reach on the canvas, and name resolution for the
// `narrater` CLI. Like Maestri, an agent only reaches what is connected to
// it: agents wired to its terminal, and notes wired to it (or chained to such
// a note).

/** A CLI request forwarded by the backend (`canvas_request` event). */
export interface CanvasRequest {
  req_id: string;
  from: string;
  from_label: string;
  action: string;
  params: Record<string, unknown> | null;
}

export const labelOf = (n: AppNode) => ((n.data as { label?: string }).label ?? n.id).trim();
export const str = (v: unknown) => (typeof v === "string" ? v : "");

export function neighbors(edges: AppEdge[], id: string): string[] {
  return edges.flatMap((e) => (e.source === id ? [e.target] : e.target === id ? [e.source] : []));
}

export interface NoteEntry {
  node: AppNode;
  depth: number;
}

export interface Reach {
  agents: AppNode[];
  /** Browser portals wired to the terminal. */
  portals: AppNode[];
  /** Directly connected notes and the ones chained to them, in tree order. */
  notes: NoteEntry[];
}

export function reachOf(nodes: AppNode[], edges: AppEdge[], from: string): Reach {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const agents = edges
    .filter((e) => e.type === "agent-pipe" && (e.source === from || e.target === from))
    .map((e) => byId.get(e.source === from ? e.target : e.source))
    .filter((n): n is AppNode => n?.type === "terminal");

  const notes: NoteEntry[] = [];
  const seen = new Set<string>();
  const walk = (id: string, depth: number) => {
    const node = byId.get(id);
    if (!node || node.type !== "note" || seen.has(id)) return;
    seen.add(id);
    notes.push({ node, depth });
    neighbors(edges, id).forEach((next) => walk(next, depth + 1));
  };
  neighbors(edges, from).forEach((id) => walk(id, 0));
  const portals = neighbors(edges, from)
    .map((id) => byId.get(id))
    .filter((n): n is AppNode => n?.type === "portal");
  return {
    agents: [...new Map(agents.map((a) => [a.id, a])).values()],
    portals: [...new Map(portals.map((p) => [p.id, p])).values()],
    notes,
  };
}

/** Exact name first, then case-insensitive; ambiguity is an error. */
export function findByName(candidates: AppNode[], name: string, kind: string): AppNode | string {
  const wanted = name.trim();
  if (!wanted) return `Error: no ${kind} name given`;
  const exact = candidates.filter((n) => labelOf(n) === wanted);
  const hits = exact.length ? exact : candidates.filter((n) => labelOf(n).toLowerCase() === wanted.toLowerCase());
  if (hits.length === 1) return hits[0];
  if (hits.length > 1) return `Error: '${wanted}' matches ${hits.length} ${kind}s — rename one on the canvas`;
  return `Error: no connected ${kind} named '${wanted}' — run \`narrater list\` for the exact names`;
}

/** `base`, or `base (2)`, `base (3)`… — unique among nodes of `type`. */
export function uniqueName(nodes: AppNode[], type: string, base: string, selfId?: string): string {
  const taken = new Set(nodes.filter((n) => n.type === type && n.id !== selfId).map(labelOf));
  if (!taken.has(base)) return base;
  let i = 2;
  while (taken.has(`${base} (${i})`)) i++;
  return `${base} (${i})`;
}

const GAP = 60;

function sizeOf(n: AppNode): { width: number; height: number } {
  return {
    width: n.width ?? (n.style?.width as number) ?? 520,
    height: n.height ?? (n.style?.height as number) ?? 360,
  };
}

/** First spot near `anchor` (right, then below/above, moving outwards) where a node of `size` overlaps nothing. */
export function freeSpot(nodes: AppNode[], anchor: AppNode, size: { width: number; height: number }): { x: number; y: number } {
  const a = sizeOf(anchor);
  const boxes = nodes.map((n) => ({ ...n.position, ...sizeOf(n) }));
  const overlaps = (x: number, y: number) =>
    boxes.some((b) =>
      x < b.x + b.width + GAP / 2 && x + size.width + GAP / 2 > b.x &&
      y < b.y + b.height + GAP / 2 && y + size.height + GAP / 2 > b.y);
  const stepX = size.width + GAP;
  const stepY = size.height + GAP;
  for (let col = 0; col < 8; col++) {
    for (const row of [0, 1, -1, 2, -2]) {
      const x = anchor.position.x + a.width + GAP + col * stepX;
      const y = anchor.position.y + row * stepY;
      if (!overlaps(x, y)) return { x, y };
    }
  }
  return { x: anchor.position.x + a.width + GAP, y: anchor.position.y + 3 * stepY };
}

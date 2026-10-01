import { listen } from "@tauri-apps/api/event";
import { useCanvasStore } from "@/stores/canvas";
import { useTerminalsStore } from "@/stores/terminals";
import { canvasRespond } from "@/lib/tauri";
import { presetOf } from "@/lib/agents";
import type { AppEdge, AppNode, NoteNodeData, TerminalNodeData } from "@/stores/canvas";

// Agent → canvas bridge for the `narrater` CLI (list, note …, connect): the
// backend emits `canvas_request`; we apply the action on the store (single
// source of truth — auto-save persists it) and answer via canvas_respond.
// Like Maestri, an agent only reaches what is connected to it: agents wired
// to its terminal, and notes wired to it (or chained to such a note).
export interface CanvasRequest {
  req_id: string;
  from: string;
  from_label: string;
  action: string;
  params: Record<string, unknown> | null;
}

const NOTE_WIDTH = 280;
const NAME_MAX = 40;

const labelOf = (n: AppNode) => ((n.data as { label?: string }).label ?? n.id).trim();
const str = (v: unknown) => (typeof v === "string" ? v : "");

function neighbors(edges: AppEdge[], id: string): string[] {
  return edges.flatMap((e) => (e.source === id ? [e.target] : e.target === id ? [e.source] : []));
}

interface NoteEntry {
  node: AppNode;
  depth: number;
}

interface Reach {
  agents: AppNode[];
  /** Directly connected notes and the ones chained to them, in tree order. */
  notes: NoteEntry[];
}

function reachOf(nodes: AppNode[], edges: AppEdge[], from: string): Reach {
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
  return { agents: [...new Map(agents.map((a) => [a.id, a])).values()], notes };
}

/** Exact name first, then case-insensitive; ambiguity is an error. */
function findByName(candidates: AppNode[], name: string, kind: string): AppNode | string {
  const wanted = name.trim();
  if (!wanted) return `Error: no ${kind} name given`;
  const exact = candidates.filter((n) => labelOf(n) === wanted);
  const hits = exact.length ? exact : candidates.filter((n) => labelOf(n).toLowerCase() === wanted.toLowerCase());
  if (hits.length === 1) return hits[0];
  if (hits.length > 1) return `Error: '${wanted}' matches ${hits.length} ${kind}s — rename one on the canvas`;
  return `Error: no connected ${kind} named '${wanted}' — run \`narrater list\` for the exact names`;
}

/** A note's automatic name: its first line, without markdown heading marks. */
function autoName(content: string): string {
  const first = content.split("\n").find((l) => l.trim()) ?? "";
  const clean = first.replace(/^#+\s*/, "").trim();
  if (!clean) return "Note";
  return clean.length > NAME_MAX ? `${clean.slice(0, NAME_MAX - 1)}…` : clean;
}

function uniqueNoteName(nodes: AppNode[], base: string, selfId?: string): string {
  const taken = new Set(nodes.filter((n) => n.type === "note" && n.id !== selfId).map(labelOf));
  if (!taken.has(base)) return base;
  let i = 2;
  while (taken.has(`${base} (${i})`)) i++;
  return `${base} (${i})`;
}

const STATUS_TEXT: Record<string, string> = {
  spawning: "starting",
  running: "working",
  idle: "idle",
  attention: "waiting for input",
  exited: "exited",
};

function agentLine(n: AppNode): string {
  const d = n.data as TerminalNodeData;
  const status = STATUS_TEXT[useTerminalsStore.getState().sessions[n.id]?.status ?? "spawning"];
  const kind = d.agentType === "custom" && d.command ? d.command : presetOf(d.agentType).label;
  return `${labelOf(n)} (${kind}) · ${status}${d.roleName ? ` · role: ${d.roleName}` : ""}`;
}

function listFor(nodes: AppNode[], edges: AppEdge[], from: string): string {
  const me = nodes.find((n) => n.id === from);
  const { agents, notes } = reachOf(nodes, edges, from);
  const out: string[] = [];
  if (me) {
    const d = me.data as TerminalNodeData;
    out.push(`You: ${labelOf(me)} (${presetOf(d.agentType).label})${d.roleName ? ` · role: ${d.roleName}` : ""}`);
  }
  out.push("", "Agents:");
  if (agents.length === 0) out.push("  (none connected — ask the user to connect agents on the canvas)");
  agents.forEach((a) => out.push(`  - ${agentLine(a)}`));
  out.push("", "Notes:");
  if (notes.length === 0) out.push("  (none — `narrater note create` makes one)");
  notes.forEach(({ node, depth }) => out.push(`${"  ".repeat(depth + 1)}- ${labelOf(node)}`));
  return out.join("\n");
}

/** Next to the caller's terminal (left side, like Maestri), stacked by count. */
function notePosition(nodes: AppNode[], from: string, stack: number): { x: number; y: number } {
  const me = nodes.find((n) => n.id === from);
  if (!me) return { x: 200, y: 200 };
  return { x: me.position.x - NOTE_WIDTH - 60, y: me.position.y + stack * 40 };
}

function numbered(content: string, offset = 1, limit?: number): string {
  if (!content) return "(empty note)";
  const lines = content.split("\n");
  const start = Math.max(1, offset);
  const end = limit ? Math.min(lines.length, start - 1 + limit) : lines.length;
  if (start > lines.length) return `(the note has ${lines.length} lines)`;
  const width = String(end).length;
  return lines
    .slice(start - 1, end)
    .map((l, i) => `${String(start + i).padStart(width, " ")}\t${l}`)
    .join("\n");
}

/** Replaces the content; an automatically named note follows its first line. */
function setNoteContent(note: AppNode, content: string): string {
  const store = useCanvasStore.getState();
  const old = (note.data as NoteNodeData).content ?? "";
  const label = labelOf(note);
  const wasAuto = label === autoName(old) || label === uniqueNoteName(store.nodes, autoName(old), note.id);
  const patch: Record<string, unknown> = { content };
  let renamed = "";
  if (wasAuto) {
    const next = uniqueNoteName(store.nodes, autoName(content), note.id);
    if (next !== label) {
      patch.label = next;
      renamed = ` — renamed to '${next}' (its first line changed; use that name from now on)`;
    }
  }
  store.updateNodeData(note.id, patch);
  store.flashNoteLive(note.id);
  return renamed;
}

function edgeTypeFor(a: AppNode, b: AppNode): string {
  if (a.type === "terminal" && b.type === "terminal") return "agent-pipe";
  if (a.type === "terminal" || b.type === "terminal") return "agent-note";
  return "default";
}

function handleRequest(req: CanvasRequest): string {
  const store = useCanvasStore.getState();
  const { nodes, edges } = store;
  const params = req.params ?? {};
  const reach = () => reachOf(nodes, edges, req.from);
  const reachableNote = (name: string) => findByName(reach().notes.map((e) => e.node), name, "note");

  switch (req.action) {
    case "list":
      return listFor(nodes, edges, req.from);

    case "note_create": {
      const content = str(params.content);
      const given = str(params.name).trim();
      const name = uniqueNoteName(nodes, given || autoName(content));
      const stack = reach().notes.filter((e) => e.depth === 0).length;
      const id = store.addNoteNode(notePosition(nodes, req.from, stack), { content, label: name });
      store.addEdge({
        id: `edge-${crypto.randomUUID().slice(0, 8)}`,
        source: id,
        target: req.from,
        type: "agent-note",
      });
      return `ok: note '${name}' created and connected to you`;
    }

    case "note_read": {
      const note = reachableNote(str(params.name));
      if (typeof note === "string") return note;
      const offset = typeof params.offset === "number" ? params.offset : 1;
      const limit = typeof params.limit === "number" ? params.limit : undefined;
      return numbered((note.data as NoteNodeData).content ?? "", offset, limit);
    }

    case "note_write": {
      const note = reachableNote(str(params.name));
      if (typeof note === "string") return note;
      const renamed = setNoteContent(note, str(params.content));
      return `ok: note '${labelOf(note)}' written${renamed}`;
    }

    case "note_edit": {
      const note = reachableNote(str(params.name));
      if (typeof note === "string") return note;
      const oldText = str(params.old);
      if (!oldText) return "Error: the text to replace is empty";
      const content = (note.data as NoteNodeData).content ?? "";
      const count = content.split(oldText).length - 1;
      if (count === 0) return `Error: text not found in '${labelOf(note)}' — read it first (\`narrater note read\`)`;
      if (count > 1) return `Error: the text appears ${count} times in '${labelOf(note)}' — include more context to make it unique`;
      const renamed = setNoteContent(note, content.replace(oldText, () => str(params.new)));
      return `ok: note '${labelOf(note)}' edited${renamed}`;
    }

    case "note_delete": {
      const note = reachableNote(str(params.name));
      if (typeof note === "string") return note;
      store.removeNode(note.id);
      return `ok: note '${labelOf(note)}' deleted`;
    }

    case "connect": {
      const me = nodes.find((n) => n.id === req.from);
      const { agents, notes } = reach();
      const candidates = [...(me ? [me] : []), ...agents, ...notes.map((e) => e.node)];
      const a = findByName(candidates, str(params.from), "agent or note");
      if (typeof a === "string") return a;
      const b = findByName(candidates, str(params.to), "agent or note");
      if (typeof b === "string") return b;
      if (a.id === b.id) return "Error: can't connect something to itself";
      const exists = edges.some(
        (e) => (e.source === a.id && e.target === b.id) || (e.source === b.id && e.target === a.id)
      );
      if (exists) return `ok: '${labelOf(a)}' and '${labelOf(b)}' were already connected`;
      store.addEdge({
        id: `edge-${crypto.randomUUID().slice(0, 8)}`,
        source: a.id,
        target: b.id,
        type: edgeTypeFor(a, b),
      });
      return `ok: connected '${labelOf(a)}' ⇄ '${labelOf(b)}' — tell them about it, they are not notified`;
    }

    default:
      return `Error: unknown command '${req.action}' — run \`narrater help\``;
  }
}

export function initCanvasBridge(): Promise<() => void> {
  return listen<CanvasRequest>("canvas_request", (event) => {
    const req = event.payload;
    let result: string;
    try {
      result = handleRequest(req);
    } catch (e) {
      result = `Error: ${e}`;
    }
    // Always respond — including errors — so the agent never hits the timeout
    canvasRespond(req.req_id, result).catch(console.error);
  });
}

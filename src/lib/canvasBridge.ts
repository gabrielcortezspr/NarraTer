import { listen } from "@tauri-apps/api/event";
import { floorOf, sceneOfNode, sliceOf, syncPortals, useCanvasStore, type SceneSlice } from "@/stores/canvas";
import { useTerminalsStore } from "@/stores/terminals";
import { canvasRespond, linksList, portalOpen } from "@/lib/tauri";
import { presetOf } from "@/lib/agents";
import { findByName, freeSpot, labelOf, reachOf, str, uniqueName, type CanvasRequest } from "@/lib/canvasReach";
import { handleManager, MANAGER_ACTIONS } from "@/lib/managerCommands";
import type { AppEdge, AppNode, NoteNodeData, PortalNodeData, TerminalNodeData } from "@/stores/canvas";

// Agent → canvas bridge for the `narrater` CLI: the backend emits
// `canvas_request` for list / note … / connect (and the Maestro commands in
// managerCommands.ts); we apply the action on the store (single source of
// truth — auto-save persists it) and answer via canvas_respond.
export type { CanvasRequest };

const NOTE_WIDTH = 280;
const NAME_MAX = 40;

/** A note's automatic name: its first line, without markdown heading marks. */
function autoName(content: string): string {
  const first = content.split("\n").find((l) => l.trim()) ?? "";
  const clean = first.replace(/^#+\s*/, "").trim();
  if (!clean) return "Note";
  return clean.length > NAME_MAX ? `${clean.slice(0, NAME_MAX - 1)}…` : clean;
}

const uniqueNoteName = (nodes: AppNode[], base: string, selfId?: string) =>
  uniqueName(nodes, "note", base, selfId);

const STATUS_TEXT: Record<string, string> = {
  spawning: "starting",
  running: "working",
  idle: "idle",
  attention: "waiting for input",
  exited: "exited",
};

/** " · on floor X" when the agent lives on another floor than `from`. */
function floorNote(sl: SceneSlice, n: AppNode, fromFloor: string): string {
  const f = floorOf(n);
  if (f === fromFloor) return "";
  const name = f === "ground" ? "Ground" : sl.floors.find((x) => x.id === f)?.name ?? f;
  return ` · on floor ${name}`;
}

/** Teammates in other workspaces linked to `from` (`recruit --workspace`). */
async function remoteAgentLines(from: string): Promise<string[]> {
  const links = await linksList().catch(() => []);
  const out: string[] = [];
  for (const l of links) {
    const other = l.a.id === from ? l.b : l.b.id === from ? l.a : null;
    if (!other) continue;
    const node = sliceOf(other.scene)?.nodes.find((n) => n.id === other.id);
    const label = node ? labelOf(node) : other.label;
    const line = node ? agentLine(node).slice(labelOf(node).length) : " · not running (its workspace isn't open)";
    out.push(`  - ${label} @ ${other.scene}${line}`);
  }
  return out;
}

function agentLine(n: AppNode): string {
  const d = n.data as TerminalNodeData;
  const status = STATUS_TEXT[useTerminalsStore.getState().sessions[n.id]?.status ?? "spawning"];
  const kind = (d.agentType === "custom" && d.command ? d.command : presetOf(d.agentType).label) + (d.model ? `, ${d.model}` : "");
  return `${labelOf(n)} (${kind}) · ${status}${d.roleName ? ` · role: ${d.roleName}` : ""}`;
}

async function listFor(sl: SceneSlice, from: string): Promise<string> {
  const { nodes, edges } = sl;
  const me = nodes.find((n) => n.id === from);
  const remote = await remoteAgentLines(from);
  const maestro = (me?.data as TerminalNodeData | undefined)?.maestro === true;
  const { agents, notes, portals } = reachOf(nodes, edges, from);
  const out: string[] = [];
  if (me) {
    const d = me.data as TerminalNodeData;
    out.push(
      `You: ${labelOf(me)} (${presetOf(d.agentType).label})` +
        `${d.roleName ? ` · role: ${d.roleName}` : ""}${maestro ? " · Maestro" : ""}`
    );
  }
  out.push("", "Agents:");
  if (agents.length === 0 && remote.length === 0) out.push("  (none connected — ask the user to connect agents on the canvas)");
  agents.forEach((a) => {
    out.push(`  - ${agentLine(a)}${me ? floorNote(sl, a, floorOf(me)) : ""}`);
    // A Maestro also sees each teammate's own wiring (before a --replace or a rewire)
    if (maestro) {
      const theirs = reachOf(nodes, edges, a.id);
      theirs.agents.filter((x) => x.id !== from).forEach((x) => out.push(`      ↳ agent: ${labelOf(x)}`));
      theirs.notes.filter((e) => e.depth === 0).forEach((e) => out.push(`      ↳ note: ${labelOf(e.node)}`));
    }
  });
  out.push(...remote);
  if (portals.length) {
    out.push("", "Portals:");
    portals.forEach((p) => {
      const d = p.data as PortalNodeData;
      out.push(`  - ${labelOf(p)} · ${d.url || "(blank)"}${d.title ? ` · ${d.title}` : ""}`);
    });
  }
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
function setNoteContent(sl: SceneSlice, note: AppNode, content: string): string {
  const store = useCanvasStore.getState();
  const old = (note.data as NoteNodeData).content ?? "";
  const label = labelOf(note);
  const wasAuto = label === autoName(old) || label === uniqueNoteName(sl.nodes, autoName(old), note.id);
  const patch: Record<string, unknown> = { content };
  let renamed = "";
  if (wasAuto) {
    const next = uniqueNoteName(sl.nodes, autoName(content), note.id);
    if (next !== label) {
      patch.label = next;
      renamed = ` — renamed to '${next}' (its first line changed; use that name from now on)`;
    }
  }
  store.updateNodeData(note.id, patch);
  store.flashNoteLive(note.id);
  return renamed;
}

const PORTAL_SIZE = { width: 720, height: 520 };

/** Viewport "WxH" → numbers (between 100 and 4000 px). */
function parseSize(raw: string): { width: number; height: number } | string | undefined {
  if (!raw) return undefined;
  const m = /^(\d+)x(\d+)$/i.exec(raw.trim());
  const ok = (n: number) => n >= 100 && n <= 4000;
  if (!m || !ok(+m[1]) || !ok(+m[2])) return `Error: --size takes WxH between 100 and 4000, e.g. 390x844`;
  return { width: +m[1], height: +m[2] };
}

function normalizeUrl(raw: string): string {
  const t = raw.trim();
  if (!t || t.includes("://") || t.startsWith("about:")) return t;
  return /^(localhost|127\.)/.test(t) ? `http://${t}` : `https://${t}`;
}

async function portalCreate(
  req: CanvasRequest, scene: string | undefined, sl: SceneSlice, params: Record<string, unknown>,
): Promise<string> {
  const store = useCanvasStore.getState();
  const me = sl.nodes.find((n) => n.id === req.from);
  if (me?.type !== "terminal") return "Error: unknown terminal";
  const url = normalizeUrl(str(params.url));
  if (!url) return 'Usage: narrater portal create URL ["Name"] [--size WxH]';
  const size = parseSize(str(params.size));
  if (typeof size === "string") return size;
  const label = uniqueName(sl.nodes, "portal", str(params.name).trim() || "Portal");
  const id = store.addPortalNode(freeSpot(sl.nodes, me, PORTAL_SIZE), url, label, scene);
  store.addEdge({ id: `edge-${crypto.randomUUID().slice(0, 8)}`, source: req.from, target: id, type: "default" }, scene);
  // The backend must know the portal (and the link) before the next command
  await syncPortals();
  try {
    await portalOpen(id, url, size?.width, size?.height);
  } catch (e) {
    return `ok: portal '${label}' created and connected to you, but the browser failed: ${e}`;
  }
  return `ok: portal '${label}' created at ${url}${size ? ` (${size.width}x${size.height})` : ""}, connected to you`;
}

function edgeTypeFor(a: AppNode, b: AppNode): string {
  if (a.type === "terminal" && b.type === "terminal") return "agent-pipe";
  if (a.type === "portal" || b.type === "portal") return "default";
  if (a.type === "terminal" || b.type === "terminal") return "agent-note";
  return "default";
}

function handleRequest(req: CanvasRequest): string | Promise<string> {
  if (MANAGER_ACTIONS.has(req.action)) return handleManager(req);
  const store = useCanvasStore.getState();
  // The caller's own scene — on screen or running in the background
  const scene = sceneOfNode(req.from);
  const sl = sliceOf(scene);
  if (!sl) return "Error: unknown terminal";
  const { nodes, edges } = sl;
  const params = req.params ?? {};
  const reach = () => reachOf(nodes, edges, req.from);
  const reachableNote = (name: string) => findByName(reach().notes.map((e) => e.node), name, "note");

  switch (req.action) {
    case "list":
      return listFor(sl, req.from);

    case "note_create": {
      const content = str(params.content);
      const given = str(params.name).trim();
      const name = uniqueNoteName(nodes, given || autoName(content));
      const stack = reach().notes.filter((e) => e.depth === 0).length;
      const id = store.addNoteNode(notePosition(nodes, req.from, stack), { content, label: name }, scene);
      store.addEdge({
        id: `edge-${crypto.randomUUID().slice(0, 8)}`,
        source: id,
        target: req.from,
        type: "agent-note",
      }, scene);
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
      const renamed = setNoteContent(sl, note, str(params.content));
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
      const renamed = setNoteContent(sl, note, content.replace(oldText, () => str(params.new)));
      return `ok: note '${labelOf(note)}' edited${renamed}`;
    }

    case "note_delete": {
      const note = reachableNote(str(params.name));
      if (typeof note === "string") return note;
      store.removeNode(note.id);
      return `ok: note '${labelOf(note)}' deleted`;
    }

    case "portal_create":
      return portalCreate(req, scene, sl, params);

    case "portal_close": {
      const portal = findByName(reach().portals, str(params.name), "portal");
      if (typeof portal === "string") return portal;
      store.removeNode(portal.id);
      return `ok: portal '${labelOf(portal)}' closed`;
    }

    case "connect": {
      const me = nodes.find((n) => n.id === req.from);
      const { agents, notes, portals } = reach();
      const candidates = [...(me ? [me] : []), ...agents, ...notes.map((e) => e.node), ...portals];
      const a = findByName(candidates, str(params.from), "agent or note");
      if (typeof a === "string") return a;
      const b = findByName(candidates, str(params.to), "agent or note");
      if (typeof b === "string") return b;
      if (a.id === b.id) return "Error: can't connect something to itself";
      const portalMisuse = (x: AppNode, y: AppNode) => x.type === "portal" && y.type !== "terminal";
      if (portalMisuse(a, b) || portalMisuse(b, a)) return "Error: a portal can only be wired to an agent";
      const exists = edges.some(
        (e) => (e.source === a.id && e.target === b.id) || (e.source === b.id && e.target === a.id)
      );
      if (exists) return `ok: '${labelOf(a)}' and '${labelOf(b)}' were already connected`;
      store.addEdge({
        id: `edge-${crypto.randomUUID().slice(0, 8)}`,
        source: a.id,
        target: b.id,
        type: edgeTypeFor(a, b),
      }, scene);
      return `ok: connected '${labelOf(a)}' ⇄ '${labelOf(b)}' — tell them about it, they are not notified`;
    }

    default:
      return `Error: unknown command '${req.action}' — run \`narrater help\``;
  }
}

export function initCanvasBridge(): Promise<() => void> {
  return listen<CanvasRequest>("canvas_request", (event) => {
    const req = event.payload;
    // Always respond — including errors — so the agent never hits the timeout
    Promise.resolve()
      .then(() => handleRequest(req))
      .catch((e) => `Error: ${e}`)
      .then((result) => canvasRespond(req.req_id, result))
      .catch(console.error);
  });
}

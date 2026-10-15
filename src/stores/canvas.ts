import { create } from "zustand";
import { applyNodeChanges, applyEdgeChanges } from "@xyflow/react";
import type { Node, Edge, NodeChange, EdgeChange, XYPosition } from "@xyflow/react";
import {
  loadScene, saveScene, connectionsSync, portalsSync, maestrosSync, routinesLoad, routineUiCreate,
  routinesWake, routinesSleep, linksRemoveNode, linksScenes, listScenes,
} from "@/lib/tauri";
import type { Drawing, FloorMeta, FloorStatusLine, SceneData, SceneNode } from "@/lib/tauri";
import { disposeTerminal, replaceTerminal, startTerminal, spawnOptsFor } from "@/lib/terminalManager";
import type { AgentType } from "@/lib/agents";
import { toast } from "@/stores/toasts";

export interface TerminalNodeData extends Record<string, unknown> {
  agentType: AgentType;
  command?: string;
  label: string;
  instructions?: string;
  scheduleCommand?: string;
  scheduleIntervalSecs?: number;
  roleId?: string;
  roleName?: string;
  roleColor?: string;
  skipPermissions?: boolean;
  /** Project directory the terminal works on. */
  cwd?: string;
  /** Maestro mode: may recruit/dismiss teammates and manage roles. */
  maestro?: boolean;
  /** Bumped when the process is swapped in place — the tile re-attaches. */
  sessionGen?: number;
}

export interface NoteNodeData extends Record<string, unknown> {
  content: string;
  label: string;
}

export interface TextNodeData extends Record<string, unknown> {
  text: string;
}

export interface FileTreeNodeData extends Record<string, unknown> {
  rootPath: string;
  expandedPaths: string[];
}

export interface AttachmentNodeData extends Record<string, unknown> {
  path: string;
  fileName: string;
}

export interface PortalNodeData extends Record<string, unknown> {
  url: string;
  /** Name agents address it by (`narrater portal … "Name"`). */
  label: string;
  /** Page title, live from the browser (not persisted). */
  title?: string;
}

export type AppNode =
  | Node<TerminalNodeData, "terminal">
  | Node<NoteNodeData, "note">
  | Node<TextNodeData, "text">
  | Node<FileTreeNodeData, "filetree">
  | Node<AttachmentNodeData, "attachment">
  | Node<PortalNodeData, "portal">;
export type AppEdge = Edge;

export interface AddTerminalOpts {
  agentType: AgentType;
  command?: string;
  instructions?: string;
  scheduleCommand?: string;
  scheduleIntervalSecs?: number;
  roleId?: string;
  roleName?: string;
  roleColor?: string;
  skipPermissions?: boolean;
  cwd?: string;
  maestro?: boolean;
  /** Terminal name; defaults to the role or agent type. */
  label?: string;
  /** Floor to create it on (default: the floor on screen). */
  floor?: string;
}

/** What a scene is made of. The one on screen lives in the store's top-level
 * fields; scenes that keep running in the background live in `background`. */
export interface SceneSlice {
  nodes: AppNode[];
  edges: AppEdge[];
  floors: FloorMeta[];
  groundStatus: FloorStatusLine | null;
  /** Pen strokes, each on its floor */
  drawings: Drawing[];
}

const emptySlice = (): SceneSlice => ({ nodes: [], edges: [], floors: [], groundStatus: null, drawings: [] });

interface CanvasStore {
  /** Name of the scene on screen. */
  scene: string;
  /**
   * Scenes that keep running off screen (like Maestri's workspaces): opened
   * earlier this session, or woken by a cross-workspace link. Their
   * terminals run, their agents use the CLI, their routines fire.
   */
  background: Record<string, SceneSlice>;
  /** Every node of the scene, all floors (the canvas shows one floor). */
  nodes: AppNode[];
  edges: AppEdge[];
  /** Floors besides the ground floor. */
  floors: FloorMeta[];
  /** Floor on screen ("ground" or a floor id). */
  currentFloor: string;
  setFloor: (id: string) => void;
  /** Adds a floor (to `scene`, default: on screen); copyGround duplicates the ground layout onto it. */
  addFloor: (floor: FloorMeta, copyGround?: { retargetFrom?: string | null }, scene?: string) => void;
  /** Removes a floor and everything on it (its terminals stop). */
  removeFloor: (id: string) => void;
  /** Status line of the ground level (floors carry their own in FloorMeta). */
  groundStatus: FloorStatusLine | null;
  /** Pen strokes of the scene on screen (all floors). */
  drawings: Drawing[];
  /** Adds a stroke on the floor on screen. */
  addDrawing: (stroke: Omit<Drawing, "floor">) => void;
  /** Removes the last stroke of the floor on screen. */
  undoDrawing: () => void;
  /** Removes every stroke of the floor on screen. */
  clearDrawings: () => void;
  /** Sets (or clears, with null) the status line of the ground or a floor of `scene`. */
  setFloorStatus: (id: string, status: FloorStatusLine | null, scene?: string) => void;
  /** Starts a scene in the background (terminals, routines) unless it's live. */
  wakeScene: (name: string) => Promise<void>;
  /** Stops a background scene: saved, its terminals and routines stop. */
  sleepScene: (name: string) => Promise<void>;
  /** A scene was deleted: if it was running in the background, it stops. */
  forgetScene: (name: string) => void;
  /** A scene was renamed: a live one follows. */
  renameLiveScene: (oldName: string, newName: string) => void;
  /** Re-reads a live scene from disk (a backup was restored): terminals
   *  still in it keep running, the others stop. */
  reloadScene: (name: string) => Promise<void>;
  // True once a scene finished loading — gates auto-save and fitView so
  // neither runs against a canvas that is mid-hydration.
  hydrated: boolean;
  /** Pushes the current state onto the history (call BEFORE a structural mutation). */
  snapshot: () => void;
  undo: () => void;
  redo: () => void;
  onNodesChange: (changes: NodeChange<AppNode>[]) => void;
  onEdgesChange: (changes: EdgeChange<AppEdge>[]) => void;
  addEdge: (edge: AppEdge, scene?: string) => void;
  addTerminalNode: (opts: AddTerminalOpts, position?: XYPosition, scene?: string) => string;
  updateNodeData: (id: string, patch: Record<string, unknown>) => void;
  /** Restarts a terminal with a new agent/role/directory, keeping the node. */
  replaceTerminalAgent: (id: string, patch: Partial<TerminalNodeData>) => void;
  addNoteNode: (position?: XYPosition, initial?: Partial<NoteNodeData>, scene?: string) => string;
  addTextNode: (position?: XYPosition, initialText?: string) => string;
  moveNode: (id: string, position: XYPosition) => void;
  addFileTreeNode: (position?: XYPosition, rootPath?: string) => string;
  addAttachmentNode: (position: XYPosition, path: string) => string;
  addPortalNode: (position?: XYPosition, url?: string, label?: string, scene?: string) => string;
  /** Lights the note's "live" badge for a moment (an agent just wrote it). */
  flashNoteLive: (noteId: string) => void;
  removeNode: (id: string) => void;
  loadScene: (name: string) => Promise<void>;
  saveScene: (name: string) => Promise<void>;
}

type NodeBase = { id: string; position: XYPosition; style: { width: number; height: number } };

/** A saved scene node → canvas node (floor applied by the caller). */
function toNode(n: SceneNode, base: NodeBase): AppNode {
  if (n.node_type === "note") {
    return { ...base, type: "note" as const, data: { content: n.content ?? "", label: n.label ?? "Note" } };
  }
  if (n.node_type === "text") {
    return { ...base, type: "text" as const, data: { text: n.content ?? "" } };
  }
  if (n.node_type === "filetree") {
    return {
      ...base,
      type: "filetree" as const,
      data: { rootPath: n.path ?? "~", expandedPaths: n.expanded_paths ?? [] },
    };
  }
  if (n.node_type === "attachment") {
    const path = n.path ?? "";
    return {
      ...base,
      type: "attachment" as const,
      data: { path, fileName: path.split("/").pop() ?? path },
    };
  }
  if (n.node_type === "portal") {
    return {
      ...base,
      type: "portal" as const,
      data: { url: n.url ?? "", label: n.label ?? "Portal" },
    };
  }
  return {
    ...base,
    type: "terminal" as const,
    data: {
      agentType: (n.agent_type ?? "shell") as AgentType,
      command: n.command,
      label: n.label ?? n.agent_type ?? "terminal",
      instructions: n.instructions,
      scheduleCommand: n.schedule_command,
      scheduleIntervalSecs: n.schedule_interval_secs,
      roleId: n.role_id,
      roleName: n.role_name,
      roleColor: n.role_color,
      skipPermissions: n.skip_permissions,
      cwd: n.cwd,
      maestro: n.maestro,
    },
  };
}

export const GROUND = "ground";

/** Floor a node lives on. */
export function floorOf(n: AppNode): string {
  return (n.data as { floor?: string }).floor || GROUND;
}

/** New nodes land on the floor on screen unless they name one. */
function onFloor(n: AppNode, floor: string): AppNode {
  if ((n.data as { floor?: string }).floor || floor === GROUND) return n;
  return { ...n, data: { ...n.data, floor } } as AppNode;
}

let nodeCounter = 0;

const liveTimers = new Map<string, ReturnType<typeof setTimeout>>();

// Undo/redo via structural snapshots (nodes+edges). Lives outside the
// reactive state — changing history must not re-render the canvas.
interface Snapshot {
  nodes: AppNode[];
  edges: AppEdge[];
}
const undoStack: Snapshot[] = [];
const redoStack: Snapshot[] = [];
const HISTORY_CAP = 50;

// Restoring a snapshot may remove terminals (undoing a create) — their
// sessions must die; terminals that come back (undoing a delete) are
// restarted by startTerminals.
function terminalsToDispose(current: AppNode[], next: AppNode[]): string[] {
  const nextIds = new Set(next.filter((n) => n.type === "terminal").map((n) => n.id));
  return current.filter((n) => n.type === "terminal" && !nextIds.has(n.id)).map((n) => n.id);
}

// Every terminal of the scene runs, on screen or not — `narrater ask` must
// reach agents the camera isn't looking at. Idempotent per terminal.
function startTerminals(nodes: AppNode[], scene: string) {
  for (const n of nodes) {
    if (n.type === "terminal") startTerminal(n.id, spawnOptsFor(n.data as TerminalNodeData, scene));
  }
}

/** Appends a node to a live scene, on the floor on screen when it's that scene. */
function addToScene(scene: string | undefined, node: AppNode): void {
  const s = useCanvasStore.getState();
  const sl = sliceOf(scene);
  if (!sl) return;
  const floor = isOnScreen(scene) ? s.currentFloor : GROUND;
  writeSlice(scene, { nodes: [...sl.nodes, onFloor(node, floor)] });
}

/** Every live scene: the one on screen and those running in the background. */
export function liveSlices(): Array<[string, SceneSlice]> {
  const s = useCanvasStore.getState();
  return [[s.scene, s], ...Object.entries(s.background)];
}

/** A live scene's contents (default: the one on screen). */
export function sliceOf(scene?: string): SceneSlice | undefined {
  const s = useCanvasStore.getState();
  if (scene === undefined || scene === s.scene) return s;
  return s.background[scene];
}

/** The live scene a node belongs to. */
export function sceneOfNode(id: string): string | undefined {
  return liveSlices().find(([, sl]) => sl.nodes.some((n) => n.id === id))?.[0];
}

/** The live scene a floor belongs to. */
function sceneOfFloor(id: string): string | undefined {
  return liveSlices().find(([, sl]) => sl.floors.some((f) => f.id === id))?.[0];
}

/** Writes part of a live scene (the one on screen, or a background one). */
function writeSlice(scene: string | undefined, patch: Partial<SceneSlice>): void {
  const s = useCanvasStore.getState();
  if (scene === undefined || scene === s.scene) {
    useCanvasStore.setState(patch);
  } else if (s.background[scene]) {
    useCanvasStore.setState({ background: { ...s.background, [scene]: { ...s.background[scene], ...patch } } });
  }
}

const isOnScreen = (scene?: string) => scene === undefined || scene === useCanvasStore.getState().scene;

// Mirror agent ropes of every live scene into the backend routing table.
// Undirected: an edge between two terminals lets either side `narrater ask`
// the other. (Cross-workspace links live in the backend itself.)
function syncConnections() {
  const pairs = liveSlices().flatMap(([, sl]) =>
    sl.edges.filter((e) => e.type === "agent-pipe").map((e) => [e.source, e.target] as [string, string])
  );
  connectionsSync(pairs).catch(console.error);
}

/** Saved scene → slice. */
function hydrate(data: SceneData): SceneSlice {
  const nodes: AppNode[] = data.nodes.map((n): AppNode => {
    const base = {
      id: n.id,
      position: { x: n.x, y: n.y },
      style: { width: n.width, height: n.height },
    };
    const node = toNode(n, base);
    return n.floor ? ({ ...node, data: { ...node.data, floor: n.floor } } as AppNode) : node;
  });
  const edges: AppEdge[] = data.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    sourceHandle: e.source_handle,
    targetHandle: e.target_handle,
    type: e.edge_type ?? "default",
    // No React Flow "animated" dashes: the pipe edge animates only when something happens
    animated: false,
    style: e.edge_type !== "agent-pipe" && e.edge_type !== "agent-note"
      ? { stroke: "rgb(var(--text-rgb) / 0.2)", strokeWidth: 1.5 }
      : undefined,
  }));
  return { nodes, edges, floors: data.floors ?? [], groundStatus: data.ground_status ?? null, drawings: data.drawings ?? [] };
}

/** Slice → what goes in the scene file. */
function serialize(sl: SceneSlice): SceneData {
  return {
    nodes: sl.nodes.map((n) => {
      const tdata = n.type === "terminal" ? (n.data as TerminalNodeData) : undefined;
      return {
        id: n.id,
        node_type: n.type ?? "terminal",
        x: n.position.x,
        y: n.position.y,
        width: n.width ?? (n.style?.width as number) ?? 520,
        height: n.height ?? (n.style?.height as number) ?? 360,
        agent_type: tdata?.agentType,
        command: tdata?.command,
        label: n.data.label as string | undefined,
        content:
          n.type === "note" ? (n.data as NoteNodeData).content
          : n.type === "text" ? (n.data as TextNodeData).text
          : undefined,
        path:
          n.type === "filetree" ? (n.data as FileTreeNodeData).rootPath
          : n.type === "attachment" ? (n.data as AttachmentNodeData).path
          : undefined,
        url: n.type === "portal" ? (n.data as PortalNodeData).url : undefined,
        expanded_paths: n.type === "filetree" ? (n.data as FileTreeNodeData).expandedPaths : undefined,
        instructions: tdata?.instructions,
        schedule_command: tdata?.scheduleCommand,
        schedule_interval_secs: tdata?.scheduleIntervalSecs,
        role_id: tdata?.roleId,
        role_name: tdata?.roleName,
        role_color: tdata?.roleColor,
        skip_permissions: tdata?.skipPermissions,
        cwd: tdata?.cwd,
        maestro: tdata?.maestro,
        floor: floorOf(n) === GROUND ? undefined : floorOf(n),
      } as SceneNode;
    }),
    edges: sl.edges.map((e) => ({
      id: e.id,
      source: e.source,
      target: e.target,
      edge_type: e.type,
      source_handle: e.sourceHandle ?? undefined,
      target_handle: e.targetHandle ?? undefined,
    })),
    floors: sl.floors,
    ground_status: sl.groundStatus,
    drawings: sl.drawings,
  };
}

/**
 * Restarts every terminal of a live scene (it moved to another machine or
 * container, or its directory changed). `retarget` moves the terminals that
 * worked in the old directory (or had none) to the new one.
 */
export function restartSceneTerminals(scene: string, retarget?: { from: string | null; to: string | null }): void {
  const sl = sliceOf(scene);
  if (!sl) return;
  for (const n of sl.nodes) {
    if (n.type !== "terminal") continue;
    let cwd = (n.data as TerminalNodeData).cwd;
    const { from, to } = retarget ?? { from: null, to: null };
    if (retarget && (!cwd || (from && (cwd === from || cwd.startsWith(`${from}/`))))) {
      cwd = to ? (cwd && from ? to + cwd.slice(from.length) : to) : undefined;
    }
    useCanvasStore.getState().replaceTerminalAgent(n.id, { cwd });
  }
}

/** Scenes linked to `name` by cross-workspace teammates wake with it. */
async function wakeLinked(name: string): Promise<void> {
  try {
    const [linked, existing] = await Promise.all([linksScenes(name), listScenes()]);
    for (const other of linked) {
      if (existing.includes(other)) await useCanvasStore.getState().wakeScene(other);
    }
  } catch (e) {
    console.error("waking linked scenes:", e);
  }
}

function uniquePortalLabel(nodes: AppNode[]): string {
  const taken = new Set(nodes.filter((n) => n.type === "portal").map((n) => (n.data as PortalNodeData).label));
  if (!taken.has("Portal")) return "Portal";
  let i = 2;
  while (taken.has(`Portal ${i}`)) i++;
  return `Portal ${i}`;
}

export const useCanvasStore = create<CanvasStore>((set, get) => ({
  scene: "",
  background: {},
  nodes: [],
  edges: [],
  floors: [],
  currentFloor: GROUND,
  groundStatus: null,
  drawings: [],
  hydrated: false,

  addDrawing: (stroke) => {
    const floor = get().currentFloor;
    set({ drawings: [...get().drawings, { ...stroke, ...(floor === GROUND ? {} : { floor }) }] });
  },

  undoDrawing: () => {
    const floor = get().currentFloor;
    const all = get().drawings;
    const i = all.map((d) => d.floor ?? GROUND).lastIndexOf(floor);
    if (i >= 0) set({ drawings: [...all.slice(0, i), ...all.slice(i + 1)] });
  },

  clearDrawings: () => {
    const floor = get().currentFloor;
    set({ drawings: get().drawings.filter((d) => (d.floor ?? GROUND) !== floor) });
  },

  setFloorStatus: (id, status, scene) => {
    const where = id === GROUND ? scene : sceneOfFloor(id) ?? scene;
    const sl = sliceOf(where);
    if (!sl) return;
    if (id === GROUND) writeSlice(where, { groundStatus: status });
    else writeSlice(where, { floors: sl.floors.map((f) => (f.id === id ? { ...f, status } : f)) });
  },

  wakeScene: async (name) => {
    if (!name || sliceOf(name)) return;
    let slice: SceneSlice;
    try {
      // A missing file loads as an empty scene; an error here is a file that
      // couldn't be read — never replace it with an empty canvas
      slice = hydrate(await loadScene(name));
    } catch (e) {
      console.error(`can't start "${name}":`, e);
      toast.error(`Couldn't open workspace "${name}": ${e}`);
      return;
    }
    if (sliceOf(name)) return; // woken meanwhile
    set({ background: { ...get().background, [name]: slice } });
    syncConnections();
    startTerminals(slice.nodes, name);
    await routinesWake(name).catch(console.error);
  },

  sleepScene: async (name) => {
    const sl = get().background[name];
    if (!sl) return;
    await saveScene(name, serialize(sl)).catch(console.error);
    sl.nodes.filter((n) => n.type === "terminal").forEach((n) => disposeTerminal(n.id));
    const { [name]: _gone, ...rest } = get().background;
    set({ background: rest });
    syncConnections();
    await routinesSleep(name).catch(console.error);
  },

  forgetScene: (name) => {
    const sl = get().background[name];
    if (!sl) return;
    sl.nodes.filter((n) => n.type === "terminal").forEach((n) => disposeTerminal(n.id));
    const { [name]: _gone, ...rest } = get().background;
    set({ background: rest });
    syncConnections();
  },

  reloadScene: async (name) => {
    const sl = sliceOf(name);
    if (!sl) return; // not running: it reads the file when it opens
    const next = hydrate(await loadScene(name));
    const keep = new Set(next.nodes.filter((n) => n.type === "terminal").map((n) => n.id));
    sl.nodes.filter((n) => n.type === "terminal" && !keep.has(n.id)).forEach((n) => disposeTerminal(n.id));
    if (isOnScreen(name)) {
      undoStack.length = 0;
      redoStack.length = 0;
      set({ currentFloor: GROUND });
    }
    writeSlice(name, next);
    syncConnections();
    startTerminals(next.nodes, name);
  },

  renameLiveScene: (oldName, newName) => {
    const { scene, background } = get();
    if (scene === oldName) set({ scene: newName });
    if (background[oldName]) {
      const { [oldName]: sl, ...rest } = background;
      set({ background: { ...rest, [newName]: sl } });
    }
  },

  setFloor: (id) => {
    if (id !== GROUND && !get().floors.some((f) => f.id === id)) return;
    set({ currentFloor: id });
  },

  addFloor: (floor, copyGround, scene) => {
    const target = scene ?? get().scene;
    const sl = sliceOf(target);
    if (!sl) return;
    const { nodes, edges } = sl;
    const added: AppNode[] = [];
    const addedEdges: AppEdge[] = [];
    if (copyGround) {
      // Fresh ids; terminals that worked in the scene's directory work in the floor's
      const ids = new Map<string, string>();
      for (const n of nodes.filter((x) => floorOf(x) === GROUND)) {
        const id = `${n.type}-${crypto.randomUUID().slice(0, 8)}`;
        ids.set(n.id, id);
        const data: Record<string, unknown> = { ...n.data, floor: floor.id };
        const cwd = data.cwd as string | undefined;
        const from = copyGround.retargetFrom;
        if (n.type === "terminal" && floor.dir && (!cwd || (from && (cwd === from || cwd.startsWith(`${from}/`))))) {
          data.cwd = cwd && from ? floor.dir + cwd.slice(from.length) : floor.dir;
        }
        delete data.sessionGen;
        added.push({ ...n, id, data, selected: false } as AppNode);
      }
      for (const e of edges) {
        const source = ids.get(e.source);
        const target = ids.get(e.target);
        if (source && target) addedEdges.push({ ...e, id: `edge-${crypto.randomUUID().slice(0, 8)}`, source, target });
      }
    }
    const nextEdges = [...edges, ...addedEdges];
    writeSlice(target, { floors: [...sl.floors, floor], nodes: [...nodes, ...added], edges: nextEdges });
    if (added.length) {
      syncConnections();
      startTerminals(added, target);
    }
  },

  removeFloor: (id) => {
    const scene = sceneOfFloor(id);
    const sl = sliceOf(scene);
    if (!sl) return;
    const { nodes, edges, floors } = sl;
    const gone = new Set(nodes.filter((n) => floorOf(n) === id).map((n) => n.id));
    nodes.filter((n) => gone.has(n.id) && n.type === "terminal").forEach((n) => {
      disposeTerminal(n.id);
      linksRemoveNode(n.id).catch(console.error);
    });
    const nextEdges = edges.filter((e) => !gone.has(e.source) && !gone.has(e.target));
    writeSlice(scene, {
      floors: floors.filter((f) => f.id !== id),
      nodes: nodes.filter((n) => !gone.has(n.id)),
      edges: nextEdges,
      drawings: sl.drawings.filter((d) => d.floor !== id),
    });
    if (isOnScreen(scene) && get().currentFloor === id) set({ currentFloor: GROUND });
    syncConnections();
  },

  snapshot: () => {
    undoStack.push({ nodes: get().nodes, edges: get().edges });
    if (undoStack.length > HISTORY_CAP) undoStack.shift();
    redoStack.length = 0;
  },

  undo: () => {
    const prev = undoStack.pop();
    if (!prev) return;
    redoStack.push({ nodes: get().nodes, edges: get().edges });
    terminalsToDispose(get().nodes, prev.nodes).forEach(disposeTerminal);
    set({ nodes: prev.nodes, edges: prev.edges });
    startTerminals(prev.nodes, get().scene);
    syncConnections();
  },

  redo: () => {
    const next = redoStack.pop();
    if (!next) return;
    undoStack.push({ nodes: get().nodes, edges: get().edges });
    terminalsToDispose(get().nodes, next.nodes).forEach(disposeTerminal);
    set({ nodes: next.nodes, edges: next.edges });
    startTerminals(next.nodes, get().scene);
    syncConnections();
  },

  onNodesChange: (changes) => {
    if (changes.some((c) => c.type === "remove")) get().snapshot();
    // With xterm/PTY living outside React, removing the node (Delete) must
    // kill the session explicitly — unmounting the tile no longer does.
    for (const c of changes) {
      if (c.type === "remove" && get().nodes.find((n) => n.id === c.id)?.type === "terminal") {
        disposeTerminal(c.id);
        linksRemoveNode(c.id).catch(console.error);
      }
    }
    set({ nodes: applyNodeChanges(changes, get().nodes) });
  },

  onEdgesChange: (changes) => {
    if (changes.some((c) => c.type === "remove")) get().snapshot();
    const edges = applyEdgeChanges(changes, get().edges);
    set({ edges });
    if (changes.some((c) => c.type === "remove")) syncConnections();
  },

  addEdge: (edge, scene) => {
    const sl = sliceOf(scene);
    if (!sl) return;
    if (isOnScreen(scene)) get().snapshot();
    writeSlice(scene, { edges: [...sl.edges, edge] });
    syncConnections();
  },

  addTerminalNode: (opts, position, scene) => {
    if (isOnScreen(scene)) get().snapshot();
    const id = `terminal-${crypto.randomUUID().slice(0, 8)}`;
    nodeCounter++;
    const { agentType, command, roleName } = opts;
    const label = opts.label ?? roleName ?? (agentType === "custom" ? (command ?? "Terminal") : agentType);
    const newNode: AppNode = {
      id,
      type: "terminal",
      position: position ?? {
        x: 80 + (nodeCounter % 5) * 60,
        y: 80 + (nodeCounter % 4) * 60,
      },
      data: { ...opts, label },
      // Born compact — the NodeResizer lets it grow when needed
      style: { width: 520, height: 360 },
    };
    const target = scene ?? get().scene;
    addToScene(target, newNode);
    startTerminals([newNode], target);
    return id;
  },

  updateNodeData: (id, patch) => {
    const scene = sceneOfNode(id);
    const sl = sliceOf(scene);
    if (!sl) return;
    writeSlice(scene, {
      nodes: sl.nodes.map((n) => (n.id === id ? ({ ...n, data: { ...n.data, ...patch } } as AppNode) : n)),
    });
  },

  replaceTerminalAgent: (id, patch) => {
    const scene = sceneOfNode(id);
    const sl = sliceOf(scene);
    const node = sl?.nodes.find((n) => n.id === id);
    if (!sl || node?.type !== "terminal") return;
    const prev = node.data as TerminalNodeData;
    const data: TerminalNodeData = { ...prev, ...patch, sessionGen: (prev.sessionGen ?? 0) + 1 };
    writeSlice(scene, { nodes: sl.nodes.map((n) => (n.id === id ? ({ ...n, data } as AppNode) : n)) });
    replaceTerminal(id, spawnOptsFor(data, scene ?? get().scene));
  },

  addNoteNode: (position, initial, scene) => {
    if (isOnScreen(scene)) get().snapshot();
    const id = `note-${crypto.randomUUID().slice(0, 8)}`;
    nodeCounter++;
    const newNode: AppNode = {
      id,
      type: "note",
      position: position ?? { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { content: "", label: "Note", ...initial },
      style: { width: 280, height: 200 },
    };
    addToScene(scene, newNode);
    return id;
  },

  addTextNode: (position, initialText) => {
    get().snapshot();
    const id = `text-${crypto.randomUUID().slice(0, 8)}`;
    nodeCounter++;
    const newNode: AppNode = {
      id,
      type: "text",
      position: position ?? { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { text: initialText ?? "" },
      style: { width: 240, height: 80 },
    };
    set((s) => ({ nodes: [...s.nodes, onFloor(newNode, s.currentFloor)] }));
    return id;
  },

  moveNode: (id, position) => {
    const scene = sceneOfNode(id);
    const sl = sliceOf(scene);
    if (!sl) return;
    writeSlice(scene, { nodes: sl.nodes.map((n) => (n.id === id ? ({ ...n, position } as AppNode) : n)) });
  },

  addFileTreeNode: (position, rootPath) => {
    get().snapshot();
    const id = `filetree-${crypto.randomUUID().slice(0, 8)}`;
    nodeCounter++;
    const newNode: AppNode = {
      id,
      type: "filetree",
      position: position ?? { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { rootPath: rootPath ?? "~", expandedPaths: [] },
      style: { width: 280, height: 360 },
    };
    set((s) => ({ nodes: [...s.nodes, onFloor(newNode, s.currentFloor)] }));
    return id;
  },

  addAttachmentNode: (position, path) => {
    get().snapshot();
    const id = `attachment-${crypto.randomUUID().slice(0, 8)}`;
    nodeCounter++;
    const fileName = path.split("/").pop() ?? path;
    const newNode: AppNode = {
      id,
      type: "attachment",
      position,
      data: { path, fileName },
      style: { width: 260, height: 220 },
    };
    set((s) => ({ nodes: [...s.nodes, onFloor(newNode, s.currentFloor)] }));
    return id;
  },

  addPortalNode: (position, url, label, scene) => {
    if (isOnScreen(scene)) get().snapshot();
    const id = `portal-${crypto.randomUUID().slice(0, 8)}`;
    nodeCounter++;
    const newNode: AppNode = {
      id,
      type: "portal",
      position: position ?? { x: 200 + nodeCounter * 20, y: 200 + nodeCounter * 20 },
      data: { url: url ?? "", label: label ?? uniquePortalLabel(sliceOf(scene)?.nodes ?? []) },
      style: { width: 720, height: 520 },
    };
    addToScene(scene, newNode);
    return id;
  },

  flashNoteLive: (noteId) => {
    get().updateNodeData(noteId, { isAgentLive: true });
    const timer = liveTimers.get(noteId);
    if (timer) clearTimeout(timer);
    liveTimers.set(
      noteId,
      setTimeout(() => {
        liveTimers.delete(noteId);
        get().updateNodeData(noteId, { isAgentLive: false });
      }, 2000)
    );
  },

  removeNode: (id) => {
    const scene = sceneOfNode(id);
    const sl = sliceOf(scene);
    if (!sl) return;
    if (isOnScreen(scene)) get().snapshot();
    if (sl.nodes.find((n) => n.id === id)?.type === "terminal") {
      disposeTerminal(id);
      linksRemoveNode(id).catch(console.error);
    }
    writeSlice(scene, {
      nodes: sl.nodes.filter((n) => n.id !== id),
      edges: sl.edges.filter((e) => e.source !== id && e.target !== id),
    });
    syncConnections();
  },

  loadScene: async (name) => {
    undoStack.length = 0;
    redoStack.length = 0;
    const st = get();
    // The scene on screen keeps running in the background (like Maestri's
    // workspaces): its agents stay reachable, its routines keep firing.
    let background = st.background;
    if (st.hydrated && st.scene && st.scene !== name) {
      const { nodes, edges, floors, groundStatus, drawings } = st;
      background = { ...background, [st.scene]: { nodes, edges, floors, groundStatus, drawings } };
    }
    const live = background[name];
    if (live) {
      const { [name]: _shown, ...rest } = background;
      set({ ...live, background: rest, scene: name, currentFloor: GROUND, hydrated: true });
      syncConnections();
      await routinesLoad(name).catch(console.error);
      void wakeLinked(name);
      return;
    }
    set({ background, scene: name, ...emptySlice(), hydrated: false, currentFloor: GROUND });
    let slice: SceneSlice;
    try {
      // A missing file loads as an empty scene (backend); an error is a file
      // that couldn't be read: show nothing and don't auto-save over it
      slice = hydrate(await loadScene(name));
    } catch (e) {
      console.error(`can't load "${name}":`, e);
      toast.error(`Couldn't open workspace "${name}" — its file was left untouched; right-click it → Backups… to restore one: ${e}`);
      return;
    }
    set({ ...slice, hydrated: true });
    syncConnections();
    startTerminals(slice.nodes, name);
    await routinesLoad(name).catch(console.error);
    await migrateScheduledPrompts();
    void wakeLinked(name);
  },

  saveScene: async (name) => {
    const sl = sliceOf(name);
    if (sl) await saveScene(name, serialize(sl));
  },

}));

/**
 * The old per-tile "scheduled prompt" was a frontend timer; it's a routine
 * now (backend scheduler, idle-gated, visible in the Routines panel). Scenes
 * saved with one get it converted once.
 */
async function migrateScheduledPrompts(): Promise<void> {
  const { nodes, updateNodeData } = useCanvasStore.getState();
  for (const n of nodes) {
    if (n.type !== "terminal") continue;
    const d = n.data as TerminalNodeData;
    if (!d.scheduleCommand?.trim() || !d.scheduleIntervalSecs) continue;
    try {
      await routineUiCreate("Scheduled prompt", d.scheduleCommand.trim(), Math.max(10, d.scheduleIntervalSecs), n.id, d.label);
      updateNodeData(n.id, { scheduleCommand: undefined, scheduleIntervalSecs: undefined });
    } catch (e) {
      console.error("scheduled prompt migration failed:", e);
    }
  }
}

// Mirror portals (name, URL) and their terminal links into the backend, which
// drives the browser tabs: agents reach a portal only when it's wired to them.
let lastPortalSync = "";
export function syncPortals(): Promise<void> {
  // Portals of every live scene stay open: background agents drive theirs too
  const nodes = liveSlices().flatMap(([, sl]) => sl.nodes);
  const edges = liveSlices().flatMap(([, sl]) => sl.edges);
  const portalIds = new Set(nodes.filter((n) => n.type === "portal").map((n) => n.id));
  const terminalIds = new Set(nodes.filter((n) => n.type === "terminal").map((n) => n.id));
  const portals = nodes
    .filter((n) => n.type === "portal")
    .map((n) => {
      const d = n.data as PortalNodeData;
      return { id: n.id, label: d.label, url: d.url };
    });
  const links: Array<[string, string]> = edges.flatMap((e): Array<[string, string]> =>
    portalIds.has(e.source) && terminalIds.has(e.target) ? [[e.source, e.target]]
    : portalIds.has(e.target) && terminalIds.has(e.source) ? [[e.target, e.source]]
    : []
  );
  const key = JSON.stringify([portals, links]);
  if (key === lastPortalSync) return Promise.resolve();
  lastPortalSync = key;
  return portalsSync(portals, links).catch(console.error);
}
// Terminals in Maestro mode → backend (routine commands check it there)
let lastMaestros = "";
function syncMaestros(): void {
  const ids = liveSlices()
    .flatMap(([, sl]) => sl.nodes)
    .filter((n) => n.type === "terminal" && (n.data as TerminalNodeData).maestro)
    .map((n) => n.id)
    .sort();
  const key = ids.join(",");
  if (key === lastMaestros) return;
  lastMaestros = key;
  maestrosSync(ids).catch(console.error);
}

useCanvasStore.subscribe(() => {
  syncPortals();
  syncMaestros();
});

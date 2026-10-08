import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { onPtyOutput, onPtyExit } from "@/lib/ptyBus";
import { ptySpawn, ptyWrite, ptyResize, ptyKill } from "@/lib/tauri";
import type { RoleSpec } from "@/lib/tauri";
import { getSpawnSpec, type AgentType } from "@/lib/agents";
import { useTerminalsStore } from "@/stores/terminals";
import { useRolesStore } from "@/stores/roles";
import { toast } from "@/stores/toasts";
import type { TerminalNodeData } from "@/stores/canvas";

// xterm+PTY lifecycle outside React (item 1.2 of the frontend plan).
// The session belongs to the canvas, not to the tile: scenes start every
// terminal on load, so agents off-screen (viewport culling, LOD) are just as
// reachable by `narrater ask` as visible ones. Tiles only attach/detach the
// xterm DOM. The session only dies in disposeTerminal (node removed / scene
// switch).

export interface SpawnOpts {
  agentType: AgentType;
  command?: string;
  label?: string;
  /** Role assigned to the node; resolved against the roles store at spawn. */
  roleId?: string;
  roleName?: string;
  /** Role instructions (possibly customized for this terminal). */
  instructions?: string;
  /** Spawn the agent with its permission prompts bypassed. */
  skipPermissions?: boolean;
  /** Project directory the terminal works on. */
  cwd?: string;
  /** Workspace (scene) it belongs to. */
  scene?: string;
}

export function spawnOptsFor(data: TerminalNodeData, scene?: string): SpawnOpts {
  return {
    agentType: data.agentType ?? "shell",
    command: data.command,
    label: data.label,
    roleId: data.roleId,
    roleName: data.roleName,
    instructions: data.instructions,
    skipPermissions: data.skipPermissions,
    cwd: data.cwd,
    scene,
  };
}

interface ManagedTerminal {
  term: XTerm;
  fit: FitAddon;
  /** Lives outside React; tiles attach/detach this element. */
  container: HTMLDivElement;
  unlisteners: Array<() => void>;
  opts: SpawnOpts;
  spawned: boolean;
  /** xterm.open() done — needs a container attached to the DOM. */
  opened: boolean;
}

function openInDom(mt: ManagedTerminal): void {
  if (mt.opened || !mt.container.isConnected) return;
  mt.opened = true;
  mt.term.open(mt.container);
  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => webgl.dispose());
    mt.term.loadAddon(webgl);
  } catch (e) {
    console.warn("[NarraTer] WebGL unavailable, using DOM renderer:", e);
  }
}

const XTERM_THEME = {
  background: "#0e0e0e",
  foreground: "#e5e7eb",
  cursor: "#8b5cf6",
  cursorAccent: "#0e0e0e",
  selection: "rgba(139,92,246,0.3)",
  black: "#1a1a2e",
  red: "#f87171",
  green: "#4ade80",
  yellow: "#fbbf24",
  blue: "#60a5fa",
  magenta: "#c084fc",
  cyan: "#22d3ee",
  white: "#e5e7eb",
  brightBlack: "#374151",
  brightRed: "#fca5a5",
  brightGreen: "#86efac",
  brightYellow: "#fde68a",
  brightBlue: "#93c5fd",
  brightMagenta: "#d8b4fe",
  brightCyan: "#67e8f9",
  brightWhite: "#f9fafb",
};

const terminals = new Map<string, ManagedTerminal>();

const BASE_FONT_SIZE = 13;
// Below this a measured size is a hidden/collapsed container, not a real one
const MIN_COLS = 20;
const MIN_ROWS = 5;
// Size a terminal starts with before its tile is ever laid out
const DEFAULT_COLS = 120;
const DEFAULT_ROWS = 32;
// Crisp re-render only where the terminal is actually readable; below the
// tile's LOD threshold the body is hidden anyway, above 3 it's the maxZoom.
const CRISP_MIN_ZOOM = 0.35;
const CRISP_MAX_ZOOM = 3;

let currentZoom = 1;
let zoomTimer: number | undefined;

// React Flow zooms with a CSS transform, so the xterm canvas is rasterized at
// the node's logical size and then stretched — blurry at any zoom ≠ 1. The
// fix is counter-scaling: lay the container out `zoom`× larger, scale it back
// down with CSS, and scale the font up by `zoom`. The WebGL canvas then backs
// the screen 1:1 in device pixels (crisp), while cell metrics scale uniformly
// so cols/rows — and therefore the PTY size — stay put.
function styleContainerForZoom(container: HTMLDivElement, zoom: number): void {
  container.style.width = `${100 * zoom}%`;
  container.style.height = `${100 * zoom}%`;
  container.style.transform = `scale(${1 / zoom})`;
  container.style.transformOrigin = "top left";
}

function applyZoom(zoom: number): void {
  currentZoom = zoom;
  for (const [id, mt] of terminals) {
    styleContainerForZoom(mt.container, zoom);
    mt.term.options.fontSize = BASE_FONT_SIZE * zoom;
    if (mt.container.isConnected) fitTerminal(id);
  }
}

/**
 * Re-renders all terminals at the screen's physical resolution for the given
 * canvas zoom. Debounced: during the gesture the plain CSS scale keeps things
 * cheap (slightly soft); once the zoom settles, the atlas re-renders crisp.
 */
export function scheduleTerminalZoomSync(zoom: number): void {
  const clamped = Math.min(CRISP_MAX_ZOOM, Math.max(CRISP_MIN_ZOOM, zoom));
  if (clamped === currentZoom) return;
  if (zoomTimer !== undefined) clearTimeout(zoomTimer);
  zoomTimer = window.setTimeout(() => {
    zoomTimer = undefined;
    applyZoom(clamped);
  }, 120);
}

async function spawnInto(id: string, mt: ManagedTerminal): Promise<void> {
  const { addSession, setRunning, setExited } = useTerminalsStore.getState();
  const { agentType, command, label, roleId, roleName, instructions, skipPermissions, cwd, scene } = mt.opts;
  addSession(id);

  // Roles must be resolved before the spawn args are built — scenes can
  // hydrate terminals before the roles store finishes loading.
  if (roleId && !useRolesStore.getState().loaded) {
    await useRolesStore.getState().load().catch(console.error);
  }
  const roleDef = roleId ? useRolesStore.getState().getRole(roleId) : undefined;
  const orchestrator = roleDef?.orchestrator ?? false;

  // The role reaches the agent as instruction files (CLAUDE.md / AGENTS.md)
  // in its start directory — works for any agent CLI. Instructions edited for
  // this terminal get their own directory instead of the role's shared one.
  let role: RoleSpec | undefined;
  const text = instructions?.trim() ?? "";
  if (agentType !== "shell" && text) {
    const shared = roleDef && roleDef.instructions.trim() === text;
    role = { key: shared ? roleDef.id : id, name: roleName, instructions: text, orchestrator };
  }

  const { command: cmd, args } = getSpawnSpec(agentType, command, skipPermissions, orchestrator);
  try {
    const effectiveLabel = await ptySpawn({
      id, command: cmd, args, cols: mt.term.cols, rows: mt.term.rows, label, agentType, cwd, role, scene,
    });
    setRunning(id);
    if (effectiveLabel && effectiveLabel !== label) {
      // Late import avoids a canvas ⇄ manager cycle during module evaluation
      const { useCanvasStore } = await import("@/stores/canvas");
      useCanvasStore.getState().updateNodeData(id, { label: effectiveLabel });
    }
  } catch (err) {
    console.error("PTY spawn failed:", err);
    toast.error(`Failed to start ${label ?? agentType}: ${err}`);
    setExited(id, 1);
  }
}

/** Creates (once) the xterm + listeners for terminal `id` and schedules the spawn. */
export function ensureTerminal(id: string, opts: SpawnOpts): ManagedTerminal {
  const existing = terminals.get(id);
  if (existing) return existing;

  const term = new XTerm({
    theme: XTERM_THEME,
    fontFamily: '"JetBrains Mono", "Cascadia Code", "Fira Code", monospace',
    fontSize: BASE_FONT_SIZE * currentZoom,
    lineHeight: 1.4,
    cursorBlink: true,
    cursorStyle: "block",
    scrollback: 5000,
    cols: DEFAULT_COLS,
    rows: DEFAULT_ROWS,
    allowTransparency: true,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.loadAddon(new WebLinksAddon());

  // Opened (and given its WebGL renderer) on the first attach, once the
  // container is in the DOM — until then the buffer still records output.
  const container = document.createElement("div");
  styleContainerForZoom(container, currentZoom);

  term.onData((d) => ptyWrite(id, d).catch(console.error));

  const unlisteners = [
    onPtyOutput(id, ({ data }) => term.write(data)),
    onPtyExit(id, () => term.writeln("\r\n\x1b[2m[Process exited]\x1b[0m")),
  ];

  const mt: ManagedTerminal = { term, fit, container, unlisteners, opts, spawned: false, opened: false };
  terminals.set(id, mt);
  return mt;
}

/** Creates the terminal (if needed) and starts its process — no DOM required. */
export function startTerminal(id: string, opts: SpawnOpts): void {
  const mt = ensureTerminal(id, opts);
  if (mt.spawned) return;
  mt.spawned = true;
  spawnInto(id, mt);
}

/** Attaches the terminal to the tile's element (and starts it if it wasn't). */
/** Puts keyboard focus in a terminal (keyboard navigation between agents). */
export function focusTerminal(id: string): void {
  terminals.get(id)?.term.focus();
}

export function attachTerminal(id: string, parent: HTMLElement): void {
  const mt = terminals.get(id);
  if (!mt) return;
  parent.appendChild(mt.container);
  openInDom(mt);
  setTimeout(() => {
    fitTerminal(id);
    if (!mt.spawned) {
      mt.spawned = true;
      spawnInto(id, mt);
    }
  }, 50);
}

/** Removes the tile's DOM without killing the terminal or process (culling/LOD). */
export function detachTerminal(id: string): void {
  terminals.get(id)?.container.remove();
}

export function fitTerminal(id: string): void {
  const mt = terminals.get(id);
  if (!mt || !mt.opened || !mt.container.isConnected) return;
  try {
    // A hidden tile (LOD: body is display:none) measures 0px and the fit
    // would shrink the PTY to 2 cols × 1 row — the agent would then render
    // for that size, and its screen (what `narrater ask` reads) is garbage.
    // Keep the last real size until the tile is visible again.
    const dims = mt.fit.proposeDimensions();
    if (!dims || dims.cols < MIN_COLS || dims.rows < MIN_ROWS) return;
    const { cols, rows } = mt.term;
    mt.fit.fit();
    // Zoom re-renders keep the grid identical by design — only tell the PTY
    // when the terminal actually changed size (avoids SIGWINCH noise).
    if (mt.term.cols !== cols || mt.term.rows !== rows) {
      ptyResize(id, mt.term.cols, mt.term.rows).catch(() => {});
    }
  } catch {
    // hidden/zero-size container — the next resize fixes it
  }
}

/**
 * Display-only write to a terminal's screen (xterm). Never goes near the PTY:
 * writing "hints" to stdin types them into the child — a shell echoes and then
 * executes the text as a command (`bash: [NarraTer]: command not found`).
 */
export function printToTerminal(id: string, text: string): void {
  terminals.get(id)?.term.write(text);
}

/** Restarts the process of an exited terminal, keeping the xterm/scrollback. */
export function respawnTerminal(id: string): void {
  const mt = terminals.get(id);
  if (!mt) return;
  mt.term.writeln("");
  spawnInto(id, mt);
}

/**
 * Swaps the program running in terminal `id` (new agent, role or directory)
 * while the node keeps its id — so its connections, position and label
 * survive. The backend replaces the old process when the same id respawns;
 * a fresh xterm takes over the screen (the tile re-attaches on sessionGen).
 */
export function replaceTerminal(id: string, opts: SpawnOpts): void {
  const mt = terminals.get(id);
  if (mt) {
    terminals.delete(id);
    mt.unlisteners.forEach((fn) => fn());
    mt.term.dispose();
    mt.container.remove();
  }
  startTerminal(id, opts);
}

/** Kills the process and frees the xterm — node removed or scene switch. */
export function disposeTerminal(id: string): void {
  const mt = terminals.get(id);
  if (!mt) return;
  terminals.delete(id);
  mt.unlisteners.forEach((fn) => fn());
  ptyKill(id).catch(console.error);
  useTerminalsStore.getState().removeSession(id);
  mt.term.dispose();
  mt.container.remove();
}

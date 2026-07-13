import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { onPtyOutput, onPtyExit } from "@/lib/ptyBus";
import { ptySpawn, ptyWrite, ptyResize, ptyKill, getSpawnSpec } from "@/lib/tauri";
import type { AgentType } from "@/lib/tauri";
import { buildAgentSystemPrompt } from "@/lib/agentPrompt";
import { useTerminalsStore } from "@/stores/terminals";
import { useRolesStore } from "@/stores/roles";
import { toast } from "@/stores/toasts";

// xterm+PTY lifecycle outside React (item 1.2 of the frontend plan).
// Unmounting a TerminalTile (viewport culling, LOD switch) only detaches the
// terminal's DOM — the process and scrollback stay alive. The session only
// dies in disposeTerminal (node removed / scene switch).

export interface SpawnOpts {
  agentType: AgentType;
  command?: string;
  label?: string;
  /** Role assigned to the node; resolved against the roles store at spawn. */
  roleId?: string;
  roleName?: string;
  /** Written into the composer after spawn (non-claude agents). */
  instructions?: string;
  /** Spawn the agent with its permission prompts bypassed (claude/codex). */
  skipPermissions?: boolean;
  /** Peer labels (agent-pipe routes), for the skill message at boot. */
  pipes?: { outgoing: string[]; incoming: string[] };
}

interface ManagedTerminal {
  term: XTerm;
  fit: FitAddon;
  /** Lives outside React; tiles attach/detach this element. */
  container: HTMLDivElement;
  unlisteners: Array<() => void>;
  opts: SpawnOpts;
  spawned: boolean;
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
  const { agentType, command, label, roleId, roleName, instructions, skipPermissions, pipes } = mt.opts;
  addSession(id);

  // Delegate-only roles must be resolved before the spawn args are built —
  // scenes can hydrate terminals before the roles store finishes loading.
  let orchestrator = false;
  if (roleId) {
    if (!useRolesStore.getState().loaded) {
      await useRolesStore.getState().load().catch(console.error);
    }
    orchestrator = useRolesStore.getState().getRole(roleId)?.orchestrator ?? false;
  }

  // Claude gets identity/role/protocol as a durable system prompt (plus the
  // narrater MCP tools); other agents keep the composer injection below.
  const systemPrompt =
    agentType === "claude"
      ? buildAgentSystemPrompt({ label: label ?? "agent", roleName, instructions, orchestrator })
      : undefined;
  const { command: cmd, args } = getSpawnSpec(agentType, command, systemPrompt, skipPermissions, orchestrator);
  try {
    const effectiveLabel = await ptySpawn({
      id, command: cmd, args, cols: mt.term.cols, rows: mt.term.rows, label, agentType,
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
    return;
  }

  if (agentType === "claude") return; // protocol goes in the system prompt

  if (instructions?.trim()) {
    ptyWrite(id, instructions.trim() + "\n").catch(console.error);
  }

  // AI agents discover routes lazily (list_peers at prompt time, sender frame
  // on delivery) — injecting the peer hint would auto-submit a turn (tokens)
  // at every boot/scene load. The visual hint below is for shells/custom only.
  if (agentType === "codex") return;

  const { outgoing = [], incoming = [] } = pipes ?? {};
  if (outgoing.length > 0 || incoming.length > 0) {
    let skillMsg = "\r\n";
    if (outgoing.length > 0) {
      skillMsg +=
        `\x1b[35m[NarraTer]\x1b[0m You can send to: \x1b[1m${outgoing.join(", ")}\x1b[0m\r\n` +
        `Use: \x1b[36mnarrater send "<name>" "message"\x1b[0m or \x1b[36mnarrater ask "<name>" "question"\x1b[0m\r\n`;
    }
    if (incoming.length > 0) {
      skillMsg += `\x1b[35m[NarraTer]\x1b[0m Receives messages from: \x1b[1m${incoming.join(", ")}\x1b[0m\r\n`;
    }
    // Screen only — via stdin the shell would execute the hint as a command
    mt.term.write(skillMsg + "\r\n");
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
    allowTransparency: true,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.loadAddon(new WebLinksAddon());

  const container = document.createElement("div");
  styleContainerForZoom(container, currentZoom);
  term.open(container);

  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => webgl.dispose());
    term.loadAddon(webgl);
  } catch (e) {
    console.warn("[NarraTer] WebGL unavailable, using DOM renderer:", e);
  }

  term.onData((d) => ptyWrite(id, d).catch(console.error));

  const unlisteners = [
    onPtyOutput(id, ({ data }) => term.write(data)),
    onPtyExit(id, () => term.writeln("\r\n\x1b[2m[Process exited]\x1b[0m")),
  ];

  const mt: ManagedTerminal = { term, fit, container, unlisteners, opts, spawned: false };
  terminals.set(id, mt);
  return mt;
}

/** Attaches the terminal to the tile's element; the first attach triggers the spawn. */
export function attachTerminal(id: string, parent: HTMLElement): void {
  const mt = terminals.get(id);
  if (!mt) return;
  parent.appendChild(mt.container);
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
  if (!mt || !mt.container.isConnected) return;
  try {
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

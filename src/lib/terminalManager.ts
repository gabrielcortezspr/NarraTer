import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { onPtyOutput, onPtyExit } from "@/lib/ptyBus";
import { ptySpawn, ptyWrite, ptyResize, ptyKill, getSpawnSpec } from "@/lib/tauri";
import type { AgentType } from "@/lib/tauri";
import { useTerminalsStore } from "@/stores/terminals";
import { toast } from "@/stores/toasts";

// xterm+PTY lifecycle outside React (item 1.2 of the frontend plan).
// Unmounting a TerminalTile (viewport culling, LOD switch) only detaches the
// terminal's DOM — the process and scrollback stay alive. The session only
// dies in disposeTerminal (node removed / scene switch).

export interface SpawnOpts {
  agentType: AgentType;
  command?: string;
  label?: string;
  systemPrompt?: string;
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

async function spawnInto(id: string, mt: ManagedTerminal): Promise<void> {
  const { addSession, setRunning, setExited } = useTerminalsStore.getState();
  const { agentType, command, label, systemPrompt, instructions, skipPermissions, pipes } = mt.opts;
  addSession(id);
  const { command: cmd, args } = getSpawnSpec(agentType, command, systemPrompt, skipPermissions);
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
    ptyWrite(id, skillMsg + "\r\n").catch(console.error);
  }
}

/** Creates (once) the xterm + listeners for terminal `id` and schedules the spawn. */
export function ensureTerminal(id: string, opts: SpawnOpts): ManagedTerminal {
  const existing = terminals.get(id);
  if (existing) return existing;

  const term = new XTerm({
    theme: XTERM_THEME,
    fontFamily: '"JetBrains Mono", "Cascadia Code", "Fira Code", monospace',
    fontSize: 13,
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
  container.style.width = "100%";
  container.style.height = "100%";
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
    mt.fit.fit();
    ptyResize(id, mt.term.cols, mt.term.rows).catch(() => {});
  } catch {
    // hidden/zero-size container — the next resize fixes it
  }
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

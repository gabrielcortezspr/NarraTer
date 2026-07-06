import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { onPtyOutput, onPtyExit } from "@/lib/ptyBus";
import { ptySpawn, ptyWrite, ptyResize, ptyKill, getSpawnSpec } from "@/lib/tauri";
import type { AgentType } from "@/lib/tauri";
import { useTerminalsStore } from "@/stores/terminals";
import { toast } from "@/stores/toasts";

// Ciclo de vida de xterm+PTY fora do React (item 1.2 do PLANO_FRONTEND).
// Desmontar um TerminalTile (culling fora da viewport, troca de LOD) apenas
// desanexa o DOM do terminal — o processo e o scrollback continuam vivos.
// A sessão só morre em disposeTerminal (nó removido / troca de história).

export interface SpawnOpts {
  agentType: AgentType;
  command?: string;
  label?: string;
  systemPrompt?: string;
  /** Escrito no composer após o spawn (agentes não-claude). */
  instructions?: string;
  /** Labels dos peers (rotas agent-pipe), para a mensagem de skill no boot. */
  pipes?: { outgoing: string[]; incoming: string[] };
}

interface ManagedTerminal {
  term: XTerm;
  fit: FitAddon;
  /** Vive fora do React; os tiles anexam/desanexam este elemento. */
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
  const { agentType, command, label, systemPrompt, instructions, pipes } = mt.opts;
  addSession(id);
  const { command: cmd, args } = getSpawnSpec(agentType, command, systemPrompt);
  try {
    const effectiveLabel = await ptySpawn({
      id, command: cmd, args, cols: mt.term.cols, rows: mt.term.rows, label, agentType,
    });
    setRunning(id);
    if (effectiveLabel && effectiveLabel !== label) {
      // Import tardio evita ciclo canvas ⇄ manager na avaliação dos módulos
      const { useCanvasStore } = await import("@/stores/canvas");
      useCanvasStore.getState().updateNodeData(id, { label: effectiveLabel });
    }
  } catch (err) {
    console.error("PTY spawn failed:", err);
    toast.error(`Falha ao iniciar ${label ?? agentType}: ${err}`);
    setExited(id, 1);
    return;
  }

  if (agentType === "claude") return; // protocolo vai no system prompt

  if (instructions?.trim()) {
    ptyWrite(id, instructions.trim() + "\n").catch(console.error);
  }
  const { outgoing = [], incoming = [] } = pipes ?? {};
  if (outgoing.length > 0 || incoming.length > 0) {
    let skillMsg = "\r\n";
    if (outgoing.length > 0) {
      skillMsg +=
        `\x1b[35m[NarraTer]\x1b[0m Você pode enviar para: \x1b[1m${outgoing.join(", ")}\x1b[0m\r\n` +
        `Use: \x1b[36mnarrater send "<nome>" "mensagem"\x1b[0m ou \x1b[36mnarrater ask "<nome>" "pergunta"\x1b[0m\r\n`;
    }
    if (incoming.length > 0) {
      skillMsg += `\x1b[35m[NarraTer]\x1b[0m Recebe mensagens de: \x1b[1m${incoming.join(", ")}\x1b[0m\r\n`;
    }
    ptyWrite(id, skillMsg + "\r\n").catch(console.error);
  }
}

/** Cria (uma vez) o xterm + listeners do terminal `id` e agenda o spawn. */
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
    console.warn("[NarraTer] WebGL indisponível, usando renderer DOM:", e);
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

/** Anexa o terminal ao elemento do tile; primeiro attach dispara o spawn. */
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

/** Remove o DOM do tile sem matar terminal nem processo (culling/LOD). */
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
    // container escondido/tamanho zero — próximo resize corrige
  }
}

/** Reinicia o processo de um terminal encerrado, mantendo o xterm/scrollback. */
export function respawnTerminal(id: string): void {
  const mt = terminals.get(id);
  if (!mt) return;
  mt.term.writeln("");
  spawnInto(id, mt);
}

/** Mata o processo e libera o xterm — nó removido ou troca de história. */
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

import { listen } from "@tauri-apps/api/event";

// Um único listener Tauri por evento PTY, despachando para handlers por id.
// Antes, cada TerminalTile registrava um listen() global e filtrava pelo
// próprio id — com N terminais, cada chunk de output acordava N listeners
// (item 1.5 do PLANO_FRONTEND). O id "*" recebe eventos de todos os
// terminais (usado pelo pipe agente→nota no Canvas).
export interface PtyOutputEvent {
  id: string;
  data: string;
}

export interface PtyExitEvent {
  id: string;
  code: number;
}

const WILDCARD = "*";

class PtyEventBus<T extends { id: string }> {
  private handlers = new Map<string, Set<(payload: T) => void>>();

  constructor(event: string) {
    // Fire-and-forget: eventos emitidos antes do listener ficar pronto se
    // perdem, como já acontecia com os listen() individuais dos tiles.
    listen<T>(event, (e) => {
      this.handlers.get(e.payload.id)?.forEach((h) => h(e.payload));
      this.handlers.get(WILDCARD)?.forEach((h) => h(e.payload));
    }).catch(console.error);
  }

  on(id: string, handler: (payload: T) => void): () => void {
    let set = this.handlers.get(id);
    if (!set) {
      set = new Set();
      this.handlers.set(id, set);
    }
    set.add(handler);
    return () => {
      set.delete(handler);
      if (set.size === 0) this.handlers.delete(id);
    };
  }
}

let outputBus: PtyEventBus<PtyOutputEvent> | undefined;
let exitBus: PtyEventBus<PtyExitEvent> | undefined;

/** Escuta o output de um terminal (ou de todos, com id "*"). Retorna unsubscribe síncrono. */
export function onPtyOutput(id: string, handler: (payload: PtyOutputEvent) => void): () => void {
  outputBus ??= new PtyEventBus<PtyOutputEvent>("pty_output");
  return outputBus.on(id, handler);
}

/** Escuta o exit de um terminal (ou de todos, com id "*"). Retorna unsubscribe síncrono. */
export function onPtyExit(id: string, handler: (payload: PtyExitEvent) => void): () => void {
  exitBus ??= new PtyEventBus<PtyExitEvent>("pty_exit");
  return exitBus.on(id, handler);
}

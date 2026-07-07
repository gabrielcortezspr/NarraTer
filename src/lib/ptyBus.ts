import { listen } from "@tauri-apps/api/event";

// A single Tauri listener per PTY event, dispatching to handlers by id.
// Previously each TerminalTile registered a global listen() and filtered by
// its own id — with N terminals, each output chunk woke N listeners
// (item 1.5 of the frontend plan). The "*" id receives events from all
// terminals (used by the agent→note pipe in the Canvas).
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
    // Fire-and-forget: events emitted before the listener is ready are lost,
    // as already happened with the tiles' individual listen() calls.
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

/** Listens to a terminal's output (or all of them, with id "*"). Returns a synchronous unsubscribe. */
export function onPtyOutput(id: string, handler: (payload: PtyOutputEvent) => void): () => void {
  outputBus ??= new PtyEventBus<PtyOutputEvent>("pty_output");
  return outputBus.on(id, handler);
}

/** Listens to a terminal's exit (or all of them, with id "*"). Returns a synchronous unsubscribe. */
export function onPtyExit(id: string, handler: (payload: PtyExitEvent) => void): () => void {
  exitBus ??= new PtyEventBus<PtyExitEvent>("pty_exit");
  return exitBus.on(id, handler);
}

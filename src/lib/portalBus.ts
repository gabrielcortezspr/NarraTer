import { listen } from "@tauri-apps/api/event";

// One Tauri listener per portal event, dispatched by portal id ("*" = all),
// like ptyBus. Frames are big (JPEG base64) — only mounted tiles subscribe.
export interface PortalFrame {
  id: string;
  data: string;
  /** Page viewport the frame represents, in CSS px. */
  width: number;
  height: number;
}

export interface PortalStateEvent {
  id: string;
  url?: string;
  title?: string;
  loading?: boolean;
  error?: string;
}

const WILDCARD = "*";

class Bus<T extends { id: string }> {
  private handlers = new Map<string, Set<(payload: T) => void>>();

  constructor(event: string) {
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

let frames: Bus<PortalFrame> | undefined;
let states: Bus<PortalStateEvent> | undefined;

export function onPortalFrame(id: string, handler: (f: PortalFrame) => void): () => void {
  frames ??= new Bus<PortalFrame>("portal_frame");
  return frames.on(id, handler);
}

export function onPortalState(id: string, handler: (s: PortalStateEvent) => void): () => void {
  states ??= new Bus<PortalStateEvent>("portal_state");
  return states.on(id, handler);
}

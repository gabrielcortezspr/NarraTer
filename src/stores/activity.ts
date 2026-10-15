import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { activityClear, activityList, type ActivityEvent } from "@/lib/tauri";

// The activity log (activity.rs): every state change of the agents and of the
// asks between them, recorded by the backend — never written by an agent.

/** Events kept in the panel; the file on disk keeps more. */
const MAX = 3000;

export type ActivityKind = "all" | "state" | "ask" | "message";

interface ActivityStore {
  events: ActivityEvent[];
  open: boolean;
  /** Only events of this terminal (as subject or peer). */
  terminal: string | null;
  /** Only the events of one ask (its request, delivery, notes and answer). */
  ask: string | null;
  kind: ActivityKind;
  /** Show state flips undone within FLIP_MS (agents booting, redraws). */
  showFlips: boolean;
  toggle: () => void;
  setOpen: (open: boolean) => void;
  setTerminal: (id: string | null) => void;
  setAsk: (id: string | null) => void;
  setKind: (k: ActivityKind) => void;
  setShowFlips: (v: boolean) => void;
  clear: () => Promise<void>;
}

export const useActivityStore = create<ActivityStore>((set) => ({
  events: [],
  open: false,
  terminal: null,
  ask: null,
  kind: "all",
  showFlips: false,
  toggle: () => set((s) => ({ open: !s.open })),
  setOpen: (open) => set({ open }),
  setTerminal: (terminal) => set({ terminal, ask: null }),
  setAsk: (ask) => set({ ask }),
  setKind: (kind) => set({ kind }),
  setShowFlips: (showFlips) => set({ showFlips }),
  clear: async () => {
    await activityClear();
    set({ events: [], ask: null });
  },
}));

const KIND_OF: Record<ActivityEvent["kind"], ActivityKind> = {
  state: "state",
  spawn: "state",
  exit: "state",
  ask: "ask",
  delivered: "ask",
  answer: "ask",
  note: "ask",
  message: "message",
};

/** A state that another state of the same terminal replaced within this long
 * is a flip: agents booting or redrawing. The file keeps it; the panel folds it
 * (and the repeats it leaves behind: ready → [working] → ready). */
export const FLIP_MS = 2000;

/** The events the panel shows with the current filters, newest first, and how
 * many quick state flips were folded away. */
export function visibleEvents(
  s: Pick<ActivityStore, "events" | "terminal" | "ask" | "kind" | "showFlips">
): { shown: ActivityEvent[]; flips: number } {
  // Which state events are flips: replaced by a newer state of the same
  // terminal within FLIP_MS, or (once those are folded) repeating the last
  // state still shown for that terminal
  const folded = new Set<number>();
  const nextState = new Map<string, number>();
  for (let i = s.events.length - 1; i >= 0; i--) {
    const e = s.events[i];
    if (e.kind !== "state") continue;
    const next = nextState.get(e.terminal);
    if (next !== undefined && next - e.ts < FLIP_MS) folded.add(i);
    nextState.set(e.terminal, e.ts);
  }
  const lastShown = new Map<string, string | undefined>();
  s.events.forEach((e, i) => {
    if (e.kind !== "state" || folded.has(i)) return;
    if (lastShown.get(e.terminal) === e.status) folded.add(i);
    else lastShown.set(e.terminal, e.status);
  });

  const shown: ActivityEvent[] = [];
  let flips = 0;
  for (let i = s.events.length - 1; i >= 0; i--) {
    const e = s.events[i];
    if (s.ask && e.ask !== s.ask) continue;
    if (s.terminal && e.terminal !== s.terminal && e.peer !== s.terminal) continue;
    if (s.kind !== "all" && KIND_OF[e.kind] !== s.kind) continue;
    if (folded.has(i) && !s.showFlips) {
      flips++;
      continue;
    }
    shown.push(e);
  }
  return { shown, flips };
}

/** Loads the recent log and follows `agent_activity`; returns the unlistener. */
export async function initActivity(): Promise<() => void> {
  try {
    const events = await activityList(MAX);
    useActivityStore.setState({ events });
  } catch (e) {
    console.warn("[NarraTer] activity log unavailable:", e);
  }
  return listen<ActivityEvent>("agent_activity", (event) => {
    useActivityStore.setState((s) => {
      const events = s.events.length >= MAX ? s.events.slice(s.events.length - MAX + 1) : s.events.slice();
      events.push(event.payload);
      return { events };
    });
  });
}

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { usageSnapshot, type ProviderUsage, type SessionUsage } from "@/lib/tauri";

// Agent usage reported by the Claude Code status line (usage.rs): account
// limits per provider, and context/cost per terminal.
interface UsageStore {
  providers: Record<string, ProviderUsage>;
  sessions: Record<string, SessionUsage>;
}

export const useUsageStore = create<UsageStore>(() => ({ providers: {}, sessions: {} }));

/** Loads what the backend knows and follows `agent_usage`; returns the unlistener. */
export async function initUsage(): Promise<() => void> {
  try {
    const snap = await usageSnapshot();
    useUsageStore.setState({
      providers: Object.fromEntries(snap.providers.map((p) => [p.provider, p])),
      sessions: Object.fromEntries(snap.sessions.map((s) => [s.terminal, s])),
    });
  } catch (e) {
    console.warn("[NarraTer] usage snapshot unavailable:", e);
  }
  return listen<{ provider: ProviderUsage | null; session: SessionUsage }>("agent_usage", (event) => {
    const { provider, session } = event.payload;
    useUsageStore.setState((s) => ({
      sessions: { ...s.sessions, [session.terminal]: session },
      providers: provider ? { ...s.providers, [provider.provider]: provider } : s.providers,
    }));
  });
}

/** Meter colour (docs/design.md): quiet below 60%, yellow below 80%, red above. */
export function usageColor(pct: number): string {
  if (pct < 60) return "var(--text-3)";
  if (pct < 80) return "var(--queue)";
  return "var(--danger)";
}

/** "14:30" today, "Thu 09:00" this week, else a date. */
export function formatReset(epochSeconds: number | null): string {
  if (!epochSeconds) return "";
  const d = new Date(epochSeconds * 1000);
  const now = new Date();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === now.toDateString()) return time;
  if (d.getTime() - now.getTime() < 6 * 24 * 3600 * 1000) {
    return `${d.toLocaleDateString([], { weekday: "short" })} ${time}`;
  }
  return d.toLocaleDateString([], { day: "numeric", month: "short" });
}

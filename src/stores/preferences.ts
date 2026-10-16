import { create } from "zustand";
import { preferencesGet, preferencesSet, type Preferences } from "@/lib/tauri";
import { toast } from "@/stores/toasts";

// App preferences (~/.config/narrater/preferences.json), shared with the
// backend. Loaded once at startup; every change is written through.

interface PreferencesStore {
  prefs: Preferences | null;
  load: () => Promise<Preferences>;
  set: (patch: Partial<Preferences>) => Promise<void>;
}

export const usePreferencesStore = create<PreferencesStore>((set, get) => ({
  prefs: null,
  load: async () => {
    const prefs = await preferencesGet();
    set({ prefs });
    return prefs;
  },
  set: async (patch) => {
    const current = get().prefs ?? (await get().load());
    const next = { ...current, ...patch };
    set({ prefs: next });
    try {
      await preferencesSet(next);
    } catch (e) {
      set({ prefs: current });
      toast.error(`Could not save the preference: ${e}`);
    }
  },
}));

/** Current preferences, loading them if needed (for CLI handlers). */
export async function currentPreferences(): Promise<Preferences> {
  return usePreferencesStore.getState().prefs ?? usePreferencesStore.getState().load();
}

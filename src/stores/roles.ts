import { create } from "zustand";
import { loadRoles, saveRoles, type Role } from "@/lib/tauri";

interface RolesStore {
  roles: Role[];
  loaded: boolean;
  load: () => Promise<void>;
  save: (roles: Role[]) => Promise<void>;
  addRole: (role: Omit<Role, "id">) => Promise<void>;
  updateRole: (id: string, patch: Partial<Omit<Role, "id">>) => Promise<void>;
  deleteRole: (id: string) => Promise<void>;
  getRole: (id: string) => Role | undefined;
}

export const useRolesStore = create<RolesStore>((set, get) => ({
  roles: [],
  loaded: false,

  load: async () => {
    const roles = await loadRoles();
    set({ roles, loaded: true });
  },

  save: async (roles) => {
    await saveRoles(roles);
    set({ roles });
  },

  addRole: async (role) => {
    const newRole: Role = {
      ...role,
      id: `role-${Date.now()}`,
    };
    const roles = [...get().roles, newRole];
    await saveRoles(roles);
    set({ roles });
  },

  updateRole: async (id, patch) => {
    const roles = get().roles.map((r) => (r.id === id ? { ...r, ...patch } : r));
    await saveRoles(roles);
    set({ roles });
  },

  deleteRole: async (id) => {
    const roles = get().roles.filter((r) => r.id !== id);
    await saveRoles(roles);
    set({ roles });
  },

  getRole: (id) => get().roles.find((r) => r.id === id),
}));

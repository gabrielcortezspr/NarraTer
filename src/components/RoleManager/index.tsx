import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, Plus, Trash2, Check, Pencil } from "lucide-react";
import { useRolesStore } from "@/stores/roles";
import type { Role } from "@/lib/tauri";

const PRESET_COLORS = [
  "#8b5cf6", "#3b82f6", "#4ade80", "#fbbf24",
  "#f87171", "#f472b6", "#22d3ee", "#a78bfa",
];

interface Props {
  open: boolean;
  onClose: () => void;
}

export default function RoleManager({ open, onClose }: Props) {
  const { roles, addRole, updateRole, deleteRole } = useRolesStore();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editDraft, setEditDraft] = useState<Partial<Role>>({});
  const [creating, setCreating] = useState(false);
  const [newDraft, setNewDraft] = useState({ name: "", color: "#8b5cf6", instructions: "" });

  const startEdit = (role: Role) => {
    setEditingId(role.id);
    setEditDraft({ name: role.name, color: role.color, instructions: role.instructions });
  };

  const saveEdit = async () => {
    if (!editingId) return;
    await updateRole(editingId, editDraft);
    setEditingId(null);
  };

  const handleCreate = async () => {
    if (!newDraft.name.trim()) return;
    await addRole(newDraft);
    setNewDraft({ name: "", color: "#8b5cf6", instructions: "" });
    setCreating(false);
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-50 flex items-center justify-center"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
        >
          <motion.div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={onClose} />

          <motion.div
            className="relative z-10 w-[520px] max-h-[85vh] bg-[#1a1a1a] border border-[#2a2a2a] rounded-xl shadow-2xl flex flex-col"
            initial={{ scale: 0.95, y: 8 }}
            animate={{ scale: 1, y: 0 }}
            exit={{ scale: 0.95, y: 8 }}
            transition={{ duration: 0.15, ease: "easeOut" }}
          >
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-[#2a2a2a] shrink-0">
              <div>
                <h2 className="text-white font-semibold text-base">Agent Roles</h2>
                <p className="text-[#6b7280] text-xs mt-0.5">Create reusable roles with custom instructions</p>
              </div>
              <button onClick={onClose} className="text-[#555] hover:text-white transition-colors p-1 rounded hover:bg-[#2a2a2a]">
                <X size={16} />
              </button>
            </div>

            {/* Role list */}
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-2">
              {roles.map((role) =>
                editingId === role.id ? (
                  <RoleEditCard
                    key={role.id}
                    draft={editDraft}
                    onChange={setEditDraft}
                    onSave={saveEdit}
                    onCancel={() => setEditingId(null)}
                  />
                ) : (
                  <RoleCard
                    key={role.id}
                    role={role}
                    onEdit={() => startEdit(role)}
                    onDelete={() => deleteRole(role.id)}
                  />
                )
              )}

              <AnimatePresence>
                {creating && (
                  <motion.div
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: "auto" }}
                    exit={{ opacity: 0, height: 0 }}
                  >
                    <RoleEditCard
                      draft={newDraft}
                      onChange={(p) => setNewDraft((d) => ({ ...d, ...p }))}
                      onSave={handleCreate}
                      onCancel={() => setCreating(false)}
                      isNew
                    />
                  </motion.div>
                )}
              </AnimatePresence>
            </div>

            {/* Footer */}
            <div className="px-5 py-3 border-t border-[#2a2a2a] shrink-0">
              <button
                onClick={() => setCreating(true)}
                className="flex items-center gap-2 text-sm text-[#8b5cf6] hover:text-[#a78bfa] transition-colors"
              >
                <Plus size={14} /> New role
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function RoleCard({ role, onEdit, onDelete }: { role: Role; onEdit: () => void; onDelete: () => void }) {
  return (
    <div className="flex items-start gap-3 p-3 rounded-lg bg-[#111] border border-[#222] hover:border-[#2a2a2a] transition-colors group">
      <span className="w-3 h-3 rounded-full shrink-0 mt-1" style={{ background: role.color }} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1">
          <span className="text-white text-sm font-medium">{role.name}</span>
        </div>
        <p className="text-[#555] text-xs leading-relaxed line-clamp-2">{role.instructions}</p>
      </div>
      <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
        <button onClick={onEdit} className="p-1 rounded text-[#555] hover:text-[#888] hover:bg-[#2a2a2a] transition-colors">
          <Pencil size={12} />
        </button>
        <button onClick={onDelete} className="p-1 rounded text-[#555] hover:text-[#f87171] hover:bg-[#2a2a2a] transition-colors">
          <Trash2 size={12} />
        </button>
      </div>
    </div>
  );
}

interface EditProps {
  draft: Partial<Role>;
  onChange: (patch: Partial<Role>) => void;
  onSave: () => void;
  onCancel: () => void;
  isNew?: boolean;
}

function RoleEditCard({ draft, onChange, onSave, onCancel, isNew }: EditProps) {
  return (
    <div className="p-3 rounded-lg bg-[#111] border border-[#8b5cf640] space-y-3">
      {/* Name + color */}
      <div className="flex items-center gap-2">
        <div className="relative shrink-0">
          <div
            className="w-6 h-6 rounded-full cursor-pointer border-2 border-[#333]"
            style={{ background: draft.color ?? "#8b5cf6" }}
          />
          <select
            className="absolute inset-0 opacity-0 cursor-pointer w-full"
            value={draft.color ?? "#8b5cf6"}
            onChange={(e) => onChange({ color: e.target.value })}
          >
            {PRESET_COLORS.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
        </div>
        <input
          value={draft.name ?? ""}
          onChange={(e) => onChange({ name: e.target.value })}
          placeholder="Role name"
          className="flex-1 bg-[#1a1a1a] border border-[#333] text-white text-sm rounded-lg px-3 py-1.5
            focus:outline-none focus:border-[#8b5cf6]"
          autoFocus={isNew}
        />
      </div>
      {/* Color swatches */}
      <div className="flex gap-1.5">
        {PRESET_COLORS.map((c) => (
          <button
            key={c}
            onClick={() => onChange({ color: c })}
            className="w-4 h-4 rounded-full border-2 transition-all hover:scale-110"
            style={{ background: c, borderColor: draft.color === c ? "#fff" : "transparent" }}
          />
        ))}
      </div>
      {/* Instructions */}
      <textarea
        value={draft.instructions ?? ""}
        onChange={(e) => onChange({ instructions: e.target.value })}
        placeholder="Instructions for the agent..."
        rows={3}
        className="w-full bg-[#1a1a1a] border border-[#333] text-white text-xs rounded-lg px-3 py-2
          focus:outline-none focus:border-[#8b5cf6] placeholder-[#444] resize-none"
      />
      <div className="flex gap-2 justify-end">
        <button onClick={onCancel} className="px-3 py-1.5 text-xs text-[#555] hover:text-white transition-colors rounded hover:bg-[#2a2a2a]">
          Cancel
        </button>
        <button
          onClick={onSave}
          disabled={!draft.name?.trim()}
          className="px-3 py-1.5 text-xs bg-[#8b5cf6] hover:bg-[#7c3aed] text-white rounded transition-colors disabled:opacity-40"
        >
          {isNew ? "Create" : "Save"}
        </button>
      </div>
    </div>
  );
}

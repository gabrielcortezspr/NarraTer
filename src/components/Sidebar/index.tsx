import { useState, useRef, useEffect, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  BookOpen, Plus, Pencil, Trash2, Check, X,
  ChevronLeft, ChevronRight, Keyboard, Users,
} from "lucide-react";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useCanvasStore } from "@/stores/canvas";
import { useRolesStore } from "@/stores/roles";
import RoleManager from "@/components/RoleManager";

interface ContextMenu {
  x: number;
  y: number;
  name: string;
}

interface Props {
  collapsed: boolean;
  onToggle: () => void;
}

export default function Sidebar({ collapsed, onToggle }: Props) {
  const { list, current, setCurrent, createWorkspace, deleteWorkspace, renameWorkspace } =
    useWorkspacesStore();
  const { saveHistoria, loadHistoria } = useCanvasStore();
  const { roles, loaded, load: loadRoles } = useRolesStore();
  const [roleManagerOpen, setRoleManagerOpen] = useState(false);

  useEffect(() => {
    if (!loaded) loadRoles();
  }, [loaded, loadRoles]);

  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [contextMenu, setContextMenu] = useState<ContextMenu | null>(null);
  const createInputRef = useRef<HTMLInputElement>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (creating) createInputRef.current?.focus();
  }, [creating]);

  useEffect(() => {
    if (renaming) renameInputRef.current?.focus();
  }, [renaming]);

  // Dismiss context menu on outside click
  useEffect(() => {
    if (!contextMenu) return;
    const handler = () => setContextMenu(null);
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [contextMenu]);

  const switchWorkspace = useCallback(
    async (name: string) => {
      if (name === current) return;
      await saveHistoria(current);
      await loadHistoria(name);
      setCurrent(name);
      setContextMenu(null);
    },
    [current, saveHistoria, loadHistoria, setCurrent]
  );

  const handleCreate = async () => {
    const trimmed = newName.trim();
    if (!trimmed) { setCreating(false); return; }
    await createWorkspace(trimmed);
    await switchWorkspace(trimmed);
    setNewName("");
    setCreating(false);
  };

  const handleRename = async () => {
    if (!renaming) return;
    await renameWorkspace(renaming, renameValue);
    setRenaming(null);
    setRenameValue("");
  };

  const handleDelete = async (name: string) => {
    setContextMenu(null);
    if (name === current) await switchWorkspace("default");
    await deleteWorkspace(name);
  };

  const startRename = (name: string) => {
    setContextMenu(null);
    setRenaming(name);
    setRenameValue(name);
  };

  return (
    <motion.aside
      animate={{ width: collapsed ? 48 : 220 }}
      transition={{ duration: 0.2, ease: "easeInOut" }}
      className="flex flex-col shrink-0 overflow-hidden border-r border-[#1f1f1f]"
      style={{ background: "#111" }}
    >
      {/* Header */}
      <div className="flex items-center h-10 px-2 shrink-0 border-b border-[#1f1f1f]">
        {!collapsed && (
          <span className="flex-1 text-[11px] font-semibold tracking-widest text-[#444] uppercase px-1">
            NarraTer
          </span>
        )}
        <button
          onClick={onToggle}
          className="ml-auto p-1.5 rounded hover:bg-[#1f1f1f] text-[#555] hover:text-[#888] transition-colors"
        >
          {collapsed ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
        </button>
      </div>

      {!collapsed && (
        <div className="flex flex-col flex-1 overflow-hidden">
          {/* Histórias section */}
          <div className="px-3 pt-3 pb-1">
            <div className="flex items-center justify-between mb-1">
              <span className="text-[10px] text-[#555] font-medium uppercase tracking-wider flex items-center gap-1">
                <BookOpen size={10} /> Histórias
              </span>
              <button
                onClick={() => setCreating(true)}
                className="p-0.5 rounded text-[#555] hover:text-[#8b5cf6] hover:bg-[#1a1a1a] transition-colors"
              >
                <Plus size={12} />
              </button>
            </div>

            <div className="flex flex-col gap-0.5 max-h-64 overflow-y-auto">
              {list.map((name) => (
                <WorkspaceItem
                  key={name}
                  name={name}
                  active={name === current}
                  isRenaming={renaming === name}
                  renameValue={renameValue}
                  renameInputRef={renaming === name ? renameInputRef : undefined}
                  onRenameChange={setRenameValue}
                  onRenameSubmit={handleRename}
                  onRenameCancel={() => setRenaming(null)}
                  onClick={() => switchWorkspace(name)}
                  onContextMenu={(e) => {
                    if (name === "default") return;
                    e.preventDefault();
                    setContextMenu({ x: e.clientX, y: e.clientY, name });
                  }}
                />
              ))}

              {/* New workspace input */}
              <AnimatePresence>
                {creating && (
                  <motion.div
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: "auto" }}
                    exit={{ opacity: 0, height: 0 }}
                    className="flex items-center gap-1 px-2 py-1"
                  >
                    <input
                      ref={createInputRef}
                      value={newName}
                      onChange={(e) => setNewName(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") handleCreate();
                        if (e.key === "Escape") { setCreating(false); setNewName(""); }
                      }}
                      placeholder="nome..."
                      className="flex-1 bg-[#1a1a1a] border border-[#333] text-white text-xs rounded px-2 py-1
                        focus:outline-none focus:border-[#8b5cf6]"
                    />
                    <button onClick={handleCreate} className="text-[#4ade80] hover:text-white transition-colors">
                      <Check size={12} />
                    </button>
                    <button onClick={() => { setCreating(false); setNewName(""); }} className="text-[#555] hover:text-white transition-colors">
                      <X size={12} />
                    </button>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          </div>

          {/* Papéis section */}
          <div className="px-3 pt-3 pb-1 border-t border-[#1a1a1a]">
            <button
              onClick={() => setRoleManagerOpen(true)}
              className="flex items-center justify-between w-full group"
            >
              <span className="text-[10px] text-[#555] font-medium uppercase tracking-wider flex items-center gap-1 group-hover:text-[#888] transition-colors">
                <Users size={10} /> Papéis
              </span>
              <span className="text-[10px] text-[#333] group-hover:text-[#555] transition-colors">{roles.length}</span>
            </button>
            {roles.length > 0 && (
              <div className="flex flex-wrap gap-1 mt-1.5">
                {roles.slice(0, 4).map((r) => (
                  <span
                    key={r.id}
                    className="text-[9px] px-1.5 py-0.5 rounded-full"
                    style={{ color: r.color, background: `${r.color}18`, border: `1px solid ${r.color}30` }}
                  >
                    {r.name}
                  </span>
                ))}
              </div>
            )}
          </div>

          {/* Spacer */}
          <div className="flex-1" />

          {/* Keyboard shortcuts */}
          <div className="px-3 pb-3 border-t border-[#1a1a1a] pt-3">
            <div className="flex items-center gap-1 mb-2">
              <Keyboard size={10} className="text-[#555]" />
              <span className="text-[10px] text-[#555] uppercase tracking-wider font-medium">Atalhos</span>
            </div>
            {[
              ["Ctrl+T", "Terminal"],
              ["Ctrl+N", "Nota"],
              ["Ctrl+D", "Desenho"],
              ["Ctrl+Z", "Desfazer"],
              ["Ctrl+S", "Salvar"],
              ["Delete", "Remover"],
            ].map(([key, label]) => (
              <div key={key} className="flex items-center justify-between mb-0.5">
                <span className="text-[10px] font-mono text-[#8b5cf6]">{key}</span>
                <span className="text-[10px] text-[#444]">{label}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      <RoleManager open={roleManagerOpen} onClose={() => setRoleManagerOpen(false)} />

      {/* Context menu */}
      {contextMenu && (
        <div
          className="fixed z-50 bg-[#1a1a1a] border border-[#2a2a2a] rounded-lg shadow-xl py-1 min-w-[140px]"
          style={{ top: contextMenu.y, left: contextMenu.x }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <button
            onClick={() => startRename(contextMenu.name)}
            className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[#ccc] hover:bg-[#2a2a2a] hover:text-white transition-colors"
          >
            <Pencil size={12} /> Renomear
          </button>
          <button
            onClick={() => handleDelete(contextMenu.name)}
            className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[#f87171] hover:bg-[#2a2a2a] transition-colors"
          >
            <Trash2 size={12} /> Excluir
          </button>
        </div>
      )}
    </motion.aside>
  );
}

interface WorkspaceItemProps {
  name: string;
  active: boolean;
  isRenaming: boolean;
  renameValue: string;
  renameInputRef?: React.RefObject<HTMLInputElement | null>;
  onRenameChange: (v: string) => void;
  onRenameSubmit: () => void;
  onRenameCancel: () => void;
  onClick: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}

function WorkspaceItem({
  name, active, isRenaming, renameValue, renameInputRef,
  onRenameChange, onRenameSubmit, onRenameCancel, onClick, onContextMenu,
}: WorkspaceItemProps) {
  if (isRenaming) {
    return (
      <div className="flex items-center gap-1 px-2 py-1">
        <input
          ref={renameInputRef as React.RefObject<HTMLInputElement>}
          value={renameValue}
          onChange={(e) => onRenameChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onRenameSubmit();
            if (e.key === "Escape") onRenameCancel();
          }}
          className="flex-1 bg-[#1a1a1a] border border-[#8b5cf6] text-white text-xs rounded px-2 py-0.5
            focus:outline-none"
        />
        <button onClick={onRenameSubmit} className="text-[#4ade80]"><Check size={11} /></button>
        <button onClick={onRenameCancel} className="text-[#555]"><X size={11} /></button>
      </div>
    );
  }

  return (
    <button
      onClick={onClick}
      onContextMenu={onContextMenu}
      className={`flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-xs transition-colors text-left
        ${active
          ? "bg-[#8b5cf620] text-white"
          : "text-[#666] hover:text-[#aaa] hover:bg-[#1a1a1a]"
        }`}
    >
      <span
        className="w-1.5 h-1.5 rounded-full shrink-0"
        style={{ background: active ? "#8b5cf6" : "transparent", border: active ? "none" : "1px solid #333" }}
      />
      <span className="truncate">{name}</span>
    </button>
  );
}

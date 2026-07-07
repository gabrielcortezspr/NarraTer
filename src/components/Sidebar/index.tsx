import { lazy, Suspense, useState, useRef, useEffect, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  BookOpen, Plus, Pencil, Trash2, Check, X,
  ChevronLeft, ChevronRight, Keyboard, Users,
} from "lucide-react";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useCanvasStore } from "@/stores/canvas";
import { saveNow } from "@/hooks/useAutoSave";
import { useSketchStore } from "@/stores/sketch";
import { useRolesStore } from "@/stores/roles";
const RoleManager = lazy(() => import("@/components/RoleManager"));

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
  const loadHistoria = useCanvasStore((s) => s.loadHistoria);
  const clearSketch = useSketchStore((s) => s.clear);
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
  const [confirmingDelete, setConfirmingDelete] = useState<string | null>(null);
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
    const handler = () => {
      setContextMenu(null);
      setConfirmingDelete(null);
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [contextMenu]);

  const switchWorkspace = useCallback(
    async (name: string) => {
      if (name === current) return;
      // saveNow (não saveHistoria direto): cancela o timer do auto-save — um
      // timer disparando após o load gravaria o canvas novo no arquivo antigo.
      await saveNow();
      await loadHistoria(name);
      setCurrent(name);
      clearSketch();
      setContextMenu(null);
    },
    [current, loadHistoria, setCurrent, clearSketch]
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
    setConfirmingDelete(null);
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
          <span className="flex-1 text-[13px] font-semibold tracking-tight px-1 select-none">
            <span className="text-[#888]">Narra</span>
            <span className="text-accent">Ter</span>
          </span>
        )}
        <button
          onClick={onToggle}
          aria-label={collapsed ? "Expandir sidebar" : "Recolher sidebar"}
          className="ml-auto p-1.5 rounded hover:bg-[#1f1f1f] text-[#555] hover:text-[#888] transition-colors"
        >
          {collapsed ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
        </button>
      </div>

      {/* Colapsada: ícones das seções, clicáveis */}
      {collapsed && (
        <div className="flex flex-col items-center gap-1 pt-2">
          <button
            onClick={onToggle}
            title="Histórias"
            aria-label="Histórias"
            className="p-2 rounded hover:bg-[#1f1f1f] text-[#555] hover:text-accent transition-colors"
          >
            <BookOpen size={14} />
          </button>
          <button
            onClick={() => setRoleManagerOpen(true)}
            title="Papéis"
            aria-label="Papéis"
            className="p-2 rounded hover:bg-[#1f1f1f] text-[#555] hover:text-accent transition-colors"
          >
            <Users size={14} />
          </button>
        </div>
      )}

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
              ["V", "Seletor"],
              ["T", "Terminal"],
              ["N", "Nota"],
              ["X", "Texto"],
              ["F", "Arquivos"],
              ["A", "Anexo"],
              ["W", "Portal"],
              ["D", "Desenho"],
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

      {roleManagerOpen && (
        <Suspense fallback={null}>
          <RoleManager open={roleManagerOpen} onClose={() => setRoleManagerOpen(false)} />
        </Suspense>
      )}

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
          {confirmingDelete === contextMenu.name ? (
            <button
              onClick={() => handleDelete(contextMenu.name)}
              className="flex items-center gap-2 w-full px-3 py-1.5 text-xs font-medium text-white bg-[#7f1d1d] hover:bg-[#991b1b] transition-colors"
            >
              <Trash2 size={12} /> Confirmar exclusão?
            </button>
          ) : (
            // Excluir é irreversível — primeiro clique só arma a confirmação
            <button
              onClick={() => setConfirmingDelete(contextMenu.name)}
              className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-[#f87171] hover:bg-[#2a2a2a] transition-colors"
            >
              <Trash2 size={12} /> Excluir
            </button>
          )}
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
      className={`relative flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-xs transition-colors text-left
        ${active
          ? "bg-[#8b5cf620] text-white"
          : "text-[#666] hover:text-[#aaa] hover:bg-[#1a1a1a]"
        }`}
    >
      {/* Barra accent do item ativo */}
      {active && <span className="absolute left-0 top-1 bottom-1 w-0.5 rounded-full bg-accent" />}
      <span
        className="w-1.5 h-1.5 rounded-full shrink-0"
        style={{ background: active ? "#8b5cf6" : "transparent", border: active ? "none" : "1px solid #333" }}
      />
      <span className="truncate">{name}</span>
    </button>
  );
}

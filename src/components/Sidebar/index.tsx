import { lazy, Suspense, useState, useRef, useEffect, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  BookOpen, Plus, Pencil, Trash2, Check, X,
  ChevronLeft, ChevronRight, Keyboard, Users, Folder, FolderOpen, Square, Server, Container, History,
} from "lucide-react";
import { toast } from "@/stores/toasts";
import EnvironmentSheet from "@/components/EnvironmentSheet";
import BackupsSheet from "@/components/BackupsSheet";
import { describeEnvironment } from "@/lib/tauri";
import { useWorkspacesStore } from "@/stores/workspaces";
import { useCanvasStore } from "@/stores/canvas";
import { switchWorkspace as switchTo } from "@/lib/workspaceNav";
import { useRolesStore } from "@/stores/roles";
import RoutinesPanel from "@/components/RoutinesPanel";
import Kbd from "@/components/ui/Kbd";
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
  const { list, current, createWorkspace, deleteWorkspace, renameWorkspace, layout, setDir } =
    useWorkspacesStore();
  const [dirValue, setDirValue] = useState<string | null>(null);
  // Workspaces running in the background (their agents keep working)
  const runningKey = useCanvasStore((s) => Object.keys(s.background).sort().join("\n"));
  const running = new Set(runningKey ? runningKey.split("\n") : []);
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
  /** Workspace whose Environment sheet is open */
  const [envFor, setEnvFor] = useState<string | null>(null);
  /** Workspace whose Backups sheet is open */
  const [backupsFor, setBackupsFor] = useState<string | null>(null);
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

  const switchWorkspace = useCallback(async (name: string) => {
    setContextMenu(null);
    await switchTo(name);
  }, []);

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
      className="glass flex flex-col shrink-0 overflow-hidden border-r border-line"
    >
      {/* Header */}
      <div className="flex items-center h-11 px-2 shrink-0">
        {!collapsed && (
          <span className="flex-1 text-[14px] font-semibold tracking-[-0.01em] px-1.5 select-none">
            <span className="text-ink">Narra</span>
            <span className="text-accent-text">Ter</span>
          </span>
        )}
        <button
          onClick={onToggle}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className="ml-auto p-1.5 rounded-md hover:bg-hover text-ink-muted hover:text-ink transition-colors"
        >
          {collapsed ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
        </button>
      </div>

      {/* Collapsed: clickable section icons */}
      {collapsed && (
        <div className="flex flex-col items-center gap-1 pt-2">
          <button
            onClick={onToggle}
            title="Scenes"
            aria-label="Scenes"
            className="p-2 rounded hover:bg-hover text-ink-muted hover:text-accent transition-colors"
          >
            <BookOpen size={14} />
          </button>
          <button
            onClick={() => setRoleManagerOpen(true)}
            title="Roles"
            aria-label="Roles"
            className="p-2 rounded hover:bg-hover text-ink-muted hover:text-accent transition-colors"
          >
            <Users size={14} />
          </button>
        </div>
      )}

      {!collapsed && (
        <div className="flex flex-col flex-1 overflow-hidden">
          {/* Scenes section */}
          <div className="px-3 pt-3 pb-1">
            <div className="flex items-center justify-between mb-1">
              <span className="text-2xs text-ink-muted font-semibold uppercase tracking-[0.06em] px-1 select-none">
                Scenes
              </span>
              <button
                onClick={() => setCreating(true)}
                title="New scene"
                aria-label="New scene"
                className="p-1 rounded-md text-ink-muted hover:text-ink hover:bg-hover transition-colors"
              >
                <Plus size={12} />
              </button>
            </div>

            <div className="flex flex-col gap-0.5 max-h-64 overflow-y-auto">
              {(() => {
                // Sidebar hierarchy (Maestri): root workspaces, root folders,
                // then groups with their folders — filed by `narrater workspace`
                const meta = (n: string) => layout.scenes[n] ?? {};
                const at = (group: string | null, folder: string | null) =>
                  list.filter((n) => (meta(n).group ?? null) === group && (meta(n).folder ?? null) === folder);
                const renderItem = (name: string) => (
                  <WorkspaceItem
                    key={name}
                    name={name}
                    dir={layout.scenes[name]?.dir ?? undefined}
                    active={name === current}
                    running={running.has(name)}
                    remote={describeEnvironment(layout.scenes[name]?.environment)}
                    isRenaming={renaming === name}
                    renameValue={renameValue}
                    renameInputRef={renaming === name ? renameInputRef : undefined}
                    onRenameChange={setRenameValue}
                    onRenameSubmit={handleRename}
                    onRenameCancel={() => setRenaming(null)}
                    onClick={() => switchWorkspace(name)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setDirValue(null);
                      setContextMenu({ x: e.clientX, y: e.clientY, name });
                    }}
                  />
                );
                const folderBlock = (f: string, group: string | null) => (
                  <div key={`f-${group}-${f}`} className="mt-1">
                    <div className="flex items-center gap-1.5 px-2 py-1 text-[12px] text-ink-muted select-none">
                      <Folder size={12} /> {f}
                    </div>
                    <div className="pl-3">{at(group, f).map(renderItem)}</div>
                  </div>
                );
                return (
                  <>
                    {at(null, null).map(renderItem)}
                    {layout.folders.filter((f) => !f.group).map((f) => folderBlock(f.name, null))}
                    {layout.groups.map((g) => (
                      <div key={`g-${g}`} className="mt-2">
                        <div className="px-2 pt-1 pb-0.5 text-2xs font-semibold text-ink-muted uppercase tracking-[0.06em] select-none">{g}</div>
                        {layout.folders.filter((f) => f.group === g).map((f) => folderBlock(f.name, g))}
                        {at(g, null).map(renderItem)}
                      </div>
                    ))}
                  </>
                );
              })()}

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
                      placeholder="name..."
                      className="flex-1 bg-surface-2 border border-line text-ink text-[12px] rounded-md px-2 py-1
                        focus:outline-none focus:border-accent/60"
                    />
                    <button onClick={handleCreate} className="text-ok hover:text-ink transition-colors">
                      <Check size={12} />
                    </button>
                    <button onClick={() => { setCreating(false); setNewName(""); }} className="text-ink-muted hover:text-ink transition-colors">
                      <X size={12} />
                    </button>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          </div>

          {/* Roles section */}
          <div className="px-3 pt-4 pb-1">
            <button
              onClick={() => setRoleManagerOpen(true)}
              className="flex items-center justify-between w-full group px-1"
            >
              <span className="text-2xs text-ink-muted font-semibold uppercase tracking-[0.06em] group-hover:text-ink-2 transition-colors">
                Roles
              </span>
              <span className="text-2xs tabular-nums text-ink-muted group-hover:text-ink-2 transition-colors">{roles.length}</span>
            </button>
            {roles.length > 0 && (
              <div className="flex flex-wrap gap-1 mt-2">
                {roles.slice(0, 6).map((r) => (
                  <button
                    key={r.id}
                    onClick={() => setRoleManagerOpen(true)}
                    className="flex items-center gap-1.5 h-[22px] px-2 rounded-full bg-hover hover:bg-press text-2xs text-ink-2 hover:text-ink transition-colors"
                  >
                    <span className="w-1.5 h-1.5 rounded-full" style={{ background: r.color }} />
                    {r.name}
                  </button>
                ))}
              </div>
            )}
          </div>

          <RoutinesPanel />

          {/* Spacer */}
          <div className="flex-1" />

          {/* Shortcuts live in the palette; one quiet hint here */}
          <div className="px-3 pb-3 pt-2">
            <div className="flex items-center justify-between px-1 text-2xs text-ink-muted select-none">
              <span className="flex items-center gap-1.5">
                <Keyboard size={12} /> Shortcuts
              </span>
              <span className="flex items-center gap-1">
                <Kbd>Ctrl</Kbd>
                <Kbd>Space</Kbd>
              </span>
            </div>
          </div>
        </div>
      )}

      {envFor && <EnvironmentSheet scene={envFor} onClose={() => setEnvFor(null)} />}
      {backupsFor && <BackupsSheet scene={backupsFor} onClose={() => setBackupsFor(null)} />}

      {roleManagerOpen && (
        <Suspense fallback={null}>
          <RoleManager open={roleManagerOpen} onClose={() => setRoleManagerOpen(false)} />
        </Suspense>
      )}

      {/* Context menu */}
      {contextMenu && (
        <div
          className="fixed z-50 bg-raised border border-line rounded-lg shadow-xl py-1 min-w-[140px]"
          style={{ top: contextMenu.y, left: contextMenu.x }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {/* Working directory: where new terminals start, and the root floors clone */}
          {dirValue === null ? (
            <button
              onClick={() => setDirValue(layout.scenes[contextMenu.name]?.dir ?? "")}
              className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-ink hover:bg-hover hover:text-ink transition-colors"
            >
              <FolderOpen size={12} /> Set directory…
            </button>
          ) : (
            <div className="px-2 py-1">
              <input
                autoFocus
                value={dirValue}
                onChange={(e) => setDirValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setDirValue(null);
                  if (e.key !== "Enter") return;
                  const name = contextMenu.name;
                  setDir(name, dirValue.trim() || null)
                    .then(() => toast.success(dirValue.trim() ? `"${name}" works in ${dirValue.trim()}` : `"${name}" has no directory now`))
                    .catch((err) => toast.error(String(err)));
                  setContextMenu(null);
                  setDirValue(null);
                }}
                placeholder="~/Projects/app (Enter)"
                className="w-[220px] bg-term border border-line-strong text-white text-[11px] font-mono rounded px-2 py-1 focus:outline-none focus:border-accent"
              />
            </div>
          )}
          <button
            onClick={() => { setEnvFor(contextMenu.name); setContextMenu(null); }}
            title="Run this workspace's agents on this machine, an SSH server or a Docker container"
            className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-ink hover:bg-hover hover:text-ink transition-colors"
          >
            <Server size={12} /> Environment…
          </button>
          <button
            onClick={() => { setBackupsFor(contextMenu.name); setContextMenu(null); }}
            title="Earlier versions of this workspace's canvas"
            className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-ink hover:bg-hover hover:text-ink transition-colors"
          >
            <History size={12} /> Backups…
          </button>
          {running.has(contextMenu.name) && (
            <button
              onClick={() => {
                const name = contextMenu.name;
                setContextMenu(null);
                useCanvasStore.getState().sleepScene(name)
                  .then(() => toast.success(`'${name}' stopped — its agents start again when you open it`))
                  .catch((e) => toast.error(String(e)));
              }}
              title="This workspace keeps running in the background — stop its terminals and routines"
              className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-ink hover:bg-hover hover:text-ink transition-colors"
            >
              <Square size={12} /> Stop agents
            </button>
          )}
          {contextMenu.name !== "default" && (
          <button
            onClick={() => startRename(contextMenu.name)}
            className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-ink hover:bg-hover hover:text-ink transition-colors"
          >
            <Pencil size={12} /> Rename
          </button>
          )}
          {contextMenu.name === "default" ? null : confirmingDelete === contextMenu.name ? (
            <button
              onClick={() => handleDelete(contextMenu.name)}
              className="flex items-center gap-2 w-full px-3 py-1.5 text-xs font-medium text-white bg-danger/70 hover:bg-danger/80 transition-colors"
            >
              <Trash2 size={12} /> Confirm delete?
            </button>
          ) : (
            // Deleting is irreversible — the first click only arms the confirmation
            <button
              onClick={() => setConfirmingDelete(contextMenu.name)}
              className="flex items-center gap-2 w-full px-3 py-1.5 text-xs text-danger-text hover:bg-hover transition-colors"
            >
              <Trash2 size={12} /> Delete
            </button>
          )}
        </div>
      )}
    </motion.aside>
  );
}

interface WorkspaceItemProps {
  name: string;
  /** Working directory (tooltip) */
  dir?: string;
  active: boolean;
  /** Running in the background (its agents keep working) */
  running: boolean;
  /** "ssh dev@box" / "docker app" when its agents run elsewhere */
  remote: string | null;
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
  name, dir, active, running, remote, isRenaming, renameValue, renameInputRef,
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
          className="flex-1 bg-surface-2 border border-accent/60 text-ink text-[12px] rounded-md px-2 py-0.5
            focus:outline-none"
        />
        <button onClick={onRenameSubmit} className="text-ok"><Check size={11} /></button>
        <button onClick={onRenameCancel} className="text-ink-muted"><X size={11} /></button>
      </div>
    );
  }

  return (
    <button
      onClick={onClick}
      onContextMenu={onContextMenu}
      title={`${dir ? `${name} — ${dir}` : `${name} — no working directory (right-click to set one)`}${remote ? ` · runs on ${remote}` : ""}${running ? " · running in the background" : ""}`}
      className={`relative flex items-center gap-2 w-full px-2 h-7 rounded-md text-[13px] transition-colors text-left
        ${active
          ? "bg-press text-ink font-medium"
          : "text-ink-2 hover:text-ink hover:bg-hover"
        }`}
    >
      {/* Accent bar for the active item */}
      {active && <span className="absolute left-0 top-1 bottom-1 w-0.5 rounded-full bg-accent" />}
      <span
        className="w-1.5 h-1.5 rounded-full shrink-0"
        style={{
          background: active ? "var(--accent)" : running ? "var(--ok)" : "transparent",
          border: active || running ? "none" : "1px solid var(--line-strong)",
        }}
      />
      <span className="truncate">{name}</span>
      {remote && (
        <span className="ml-auto shrink-0 text-ink-muted" title={`Runs on ${remote}`}>
          {remote.startsWith("ssh") ? <Server size={10} /> : <Container size={10} />}
        </span>
      )}
    </button>
  );
}

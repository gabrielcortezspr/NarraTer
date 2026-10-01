import { invoke } from "@tauri-apps/api/core";
import type { AgentType } from "@/lib/agents";

export type { AgentType };

/** Role context the backend writes as CLAUDE.md / AGENTS.md for the agent. */
export interface RoleSpec {
  /** Directory key under `.narrater/roles/` (role id, or terminal id when customized). */
  key: string;
  name?: string;
  instructions: string;
  orchestrator?: boolean;
}

export interface PtySpawnOptions {
  id: string;
  command: string;
  args?: string[];
  cols: number;
  rows: number;
  label?: string;
  agentType?: AgentType;
  /** Project directory the terminal works on (defaults to $HOME). */
  cwd?: string;
  role?: RoleSpec;
}

export interface SceneNode {
  id: string;
  node_type: string;
  x: number;
  y: number;
  width: number;
  height: number;
  agent_type?: AgentType;
  command?: string;
  label?: string;
  content?: string;
  instructions?: string;
  schedule_command?: string;
  schedule_interval_secs?: number;
  role_id?: string;
  role_name?: string;
  role_color?: string;
  skip_permissions?: boolean;
  cwd?: string;
  maestro?: boolean;
  // filetree.rootPath / attachment.path
  path?: string;
  // portal
  url?: string;
  // filetree
  expanded_paths?: string[];
}

export interface SceneEdge {
  id: string;
  source: string;
  target: string;
  edge_type?: string;
  source_handle?: string;
  target_handle?: string;
}

export interface SceneData {
  nodes: SceneNode[];
  edges: SceneEdge[];
}

/** Message waiting in a terminal's queue (pty_queue event). */
export interface QueueItem {
  from_label: string;
  msg: string;
}

/** Inter-agent message ledger record (narrater_msg event). */
export interface LedgerEntry {
  from: string;
  to: string;
  from_label: string;
  to_label: string;
  kind: "ask" | "reply" | "raw";
  msg: string;
  ts: number;
}

// Returns the effective label assigned by the backend (deduplicated if taken)
export const ptySpawn = (opts: PtySpawnOptions) =>
  invoke<string>("pty_spawn", opts as unknown as Record<string, unknown>);

export const ptyWrite = (id: string, data: string) =>
  invoke<void>("pty_write", { id, data });

export const ptyResize = (id: string, cols: number, rows: number) =>
  invoke<void>("pty_resize", { id, cols, rows });

export const ptyKill = (id: string) =>
  invoke<void>("pty_kill", { id });

export const ptyUpdateLabel = (id: string, label: string) =>
  invoke<void>("pty_update_label", { id, label });

export const ptyQueueCancel = (id: string, index: number) =>
  invoke<void>("pty_queue_cancel", { id, index });

/** Conversation between two nodes (both directions), oldest first. */
export const narraterLedger = (a: string, b: string) =>
  invoke<LedgerEntry[]>("narrater_ledger", { a, b });

/** Turn state of every live session ("running" | "idle" | "attention"). */
export const ptyStatuses = () =>
  invoke<Record<string, "running" | "idle" | "attention">>("pty_statuses");

// Mirrors the canvas agent ropes to the backend routing table (undirected)
export const connectionsSync = (connections: Array<[string, string]>) =>
  invoke<void>("connections_sync", { connections });

export const loadScene = (name: string) =>
  invoke<SceneData>("load_scene", { name });

export const saveScene = (name: string, data: SceneData) =>
  invoke<void>("save_scene", { name, data });

export const listScenes = () =>
  invoke<string[]>("list_scenes");

export const deleteScene = (name: string) =>
  invoke<void>("delete_scene", { name });

export const renameScene = (oldName: string, newName: string) =>
  invoke<void>("rename_scene", { old_name: oldName, new_name: newName });

export const openInEditor = (editor: string, path: string) =>
  invoke<void>("open_in_editor", { editor, path });

export interface FsEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface FileBlob {
  base64: string;
  mime: string;
  size: number;
}

export const fsListDir = (path: string) =>
  invoke<FsEntry[]>("fs_list_dir", { path });

export const fsReadFileBase64 = (path: string) =>
  invoke<FileBlob>("fs_read_file_base64", { path });

// Native file picker (via the Rust-side dialog plugin); null if cancelled
export const pickFile = () => invoke<string | null>("pick_file");

export const openUrl = (url: string) => invoke<void>("open_url", { url });

// ---- Browser portals (headless Chrome over CDP) ----

export interface PortalEntry {
  id: string;
  label: string;
  url: string;
}

/** Mirrors canvas portals and their [portalId, terminalId] links to the backend. */
export const portalsSync = (portals: PortalEntry[], links: Array<[string, string]>) =>
  invoke<void>("portals_sync", { portals, links });

export const portalOpen = (id: string, url?: string, width?: number, height?: number) =>
  invoke<void>("portal_open", { id, url, width, height });

/** Tile on screen: open the tab if needed and start its screencast. */
export const portalAttach = (id: string, url?: string) => invoke<void>("portal_attach", { id, url });
export const portalDetach = (id: string) => invoke<void>("portal_detach", { id });
export const portalNavigate = (id: string, url: string) => invoke<void>("portal_navigate", { id, url });
/** -1 back, 1 forward, 0 reload. */
export const portalHistory = (id: string, delta: number) => invoke<void>("portal_history", { id, delta });
/** Raw CDP input (Input.dispatchMouseEvent / dispatchKeyEvent / insertText), page CSS px. */
export const portalInput = (id: string, method: string, params: Record<string, unknown>) =>
  invoke<void>("portal_input", { id, method, params });

// ---- Routines (backend scheduler, per scene) ----

export interface RoutineView {
  id: string;
  short_id: string;
  name: string;
  command: string;
  /** "every 30m", "daily 09:00", "weekly mon,fri@09:00", "once …" */
  schedule: string;
  /** Terminal node id; null for reminders. */
  target_id: string | null;
  target_label: string;
  status: "enabled" | "paused" | "finished";
  next_fire: string | null;
  fired: number;
  skipped: number;
  last_result: string | null;
}

/** Loads the scene's routines into the scheduler (call when a scene opens). */
export const routinesLoad = (scene: string) => invoke<RoutineView[]>("routines_load", { scene });
export const routinesList = () => invoke<RoutineView[]>("routines_list");
export const routineUiCreate = (name: string, command: string, everySecs: number, terminalId: string, terminalLabel: string) =>
  invoke<void>("routine_ui_create", { name, command, everySecs, terminalId, terminalLabel });
export const routineSetEnabled = (id: string, enabled: boolean) => invoke<void>("routine_set_enabled", { id, enabled });
export const routineDelete = (id: string) => invoke<void>("routine_delete", { id });
export const routineRun = (id: string) => invoke<string>("routine_run", { id });

/** Terminals in Maestro mode (gates Maestro-only CLI commands in the backend). */
export const maestrosSync = (ids: string[]) => invoke<void>("maestros_sync", { ids });

/** Desktop notification (notify-send). */
export const systemNotify = (title: string, body: string) =>
  invoke<void>("system_notify", { title, body });

// Resolves a canvas request coming from an agent (canvas_request event)
export const canvasRespond = (reqId: string, result: string) =>
  invoke<void>("canvas_respond", { reqId, result });

export interface Role {
  id: string;
  name: string;
  color: string;
  instructions: string;
  /** Delegate-only: never executes tasks itself (claude spawns also lose the file-editing tools). */
  orchestrator?: boolean;
}

export const loadRoles = () => invoke<Role[]>("load_roles");
export const saveRoles = (roles: Role[]) => invoke<void>("save_roles", { roles });

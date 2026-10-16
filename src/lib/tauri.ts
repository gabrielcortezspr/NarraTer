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
  /** Workspace (scene) the terminal belongs to — names are unique per scene. */
  scene?: string;
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
  model?: string;
  cwd?: string;
  maestro?: boolean;
  /** Floor id (absent = ground) */
  floor?: string;
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

/** A floor of a scene; git-isolated when it has a worktree/branch. */
export interface FloorMeta {
  id: string;
  name: string;
  branch?: string | null;
  worktree?: string | null;
  /** Where its terminals work (absent: the scene's directory) */
  dir?: string | null;
  /** The checkout is a full copy of the repository (`--copy`), not a worktree */
  copy?: boolean | null;
  /** The pull request the floor reviews (`--pull-request`) */
  pull_request?: PullRequestInfo | null;
  /** The line agents post with `narrater floor status` */
  status?: FloorStatusLine | null;
}

export interface PullRequestInfo {
  number: number;
  title: string;
  url: string;
  provider: "github" | "gitlab";
  /** The floor has the PR's own branch (false: a fork, copied to pr-N) */
  same_repo: boolean;
}

export type FloorState = "working" | "blocked" | "review" | "done";

/** Where a floor's work stands (shown in the floor bar). */
export interface FloorStatusLine {
  text: string;
  state: FloorState;
  /** "2/5", "40%" */
  progress?: string | null;
  /** Epoch ms of the last post */
  at: number;
}

export interface SceneData {
  nodes: SceneNode[];
  edges: SceneEdge[];
  floors?: FloorMeta[];
  /** Status line of the ground level */
  ground_status?: FloorStatusLine | null;
  /** Freehand drawings (canvas coordinates) */
  drawings?: Drawing[];
}

/** A pen stroke: points are [x, y, pressure]; floor absent = ground. */
export interface Drawing {
  points: number[][];
  color: string;
  size: number;
  floor?: string | null;
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
  invoke<void>("rename_scene", { oldName, newName });

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

/** Tile on screen: open the tab and stream it. */
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

// ---- Prompt composer ----

/** Queues a prompt for a terminal: how many prompts are ahead of it, and whether it waits for the agent to go idle. */
export const composerSend = (terminalId: string, text: string) =>
  invoke<{ ahead: number; waiting: boolean }>("composer_send", { terminalId, text });
/** Saves a pasted image where the terminal's agent can open it; returns that path. */
export const composerAttach = (terminalId: string, dataBase64: string, ext: string) =>
  invoke<string>("composer_attach", { terminalId, dataBase64, ext });
/** Saves the clipboard's image for the terminal (WebKitGTK pastes images empty); null: no image there. */
export const composerPasteImage = (terminalId: string) => invoke<string | null>("composer_paste_image", { terminalId });

// ---- Backups ----

export interface BackupInfo {
  file: string;
  /** Epoch ms */
  at: number;
  nodes: number;
  terminals: number;
  floors: number;
}
/** A workspace's backups, newest first. */
export const sceneBackups = (name: string) => invoke<BackupInfo[]>("scene_backups", { name });
/** Puts a backup back (the current version becomes a backup first). */
export const sceneRestoreBackup = (name: string, file: string) => invoke<void>("scene_restore_backup", { name, file });

// ---- Live scenes and cross-workspace links ----

/** One end of a cross-workspace link (`recruit --workspace`). */
export interface LinkEnd {
  id: string;
  scene: string;
  label: string;
}
export interface Link {
  a: LinkEnd;
  b: LinkEnd;
}
export const linksList = () => invoke<Link[]>("links_list");
export const linkAdd = (a: LinkEnd, b: LinkEnd) => invoke<void>("link_add", { a, b });
export const linksRemoveNode = (id: string) => invoke<boolean>("links_remove_node", { id });
/** Scenes with terminals linked to `scene`'s. */
export const linksScenes = (scene: string) => invoke<string[]>("links_scenes", { scene });
export const routinesWake = (scene: string) => invoke<void>("routines_wake", { scene });
export const routinesSleep = (scene: string) => invoke<void>("routines_sleep", { scene });

// ---- Workspaces (sidebar layout) and floors ----

export interface WorkspaceMeta {
  dir?: string | null;
  group?: string | null;
  folder?: string | null;
  /** Where its terminals run (absent: this machine) */
  environment?: Environment | null;
}

/** Where a workspace's terminals run: this machine, an SSH server or a Docker container. */
export type Environment =
  | { kind: "local" }
  | { kind: "ssh"; host: string; port?: number | null; user?: string | null; persistent?: boolean }
  | {
      kind: "docker";
      container: string;
      context?: string | null;
      user?: string | null;
      shell?: string | null;
      persistent?: boolean;
    };

export interface ContainerInfo {
  name: string;
  image: string;
  status: string;
}

/** Points a workspace at an environment; `dir` is the project directory over there. */
export const workspaceSetEnvironment = (name: string, environment: Environment, dir: string | null) =>
  invoke<void>("workspace_set_environment", { name, environment, dir });
/** Connects, installs the CLI and skills, checks curl/bash/tmux; returns a summary. */
export const environmentTest = (environment: Environment) => invoke<string>("environment_test", { environment });
export const environmentSshHosts = () => invoke<string[]>("environment_ssh_hosts");
export const environmentDockerContainers = (context: string | null) =>
  invoke<ContainerInfo[]>("environment_docker_containers", { context });
export const environmentDockerContexts = () => invoke<string[]>("environment_docker_contexts");
export const environmentSessions = (environment: Environment) => invoke<string[]>("environment_sessions", { environment });
export const environmentEndSessions = (environment: Environment, session: string | null) =>
  invoke<void>("environment_end_sessions", { environment, session });

/** "ssh dev@box", "docker app" — short label for badges. */
export function describeEnvironment(env: Environment | null | undefined): string | null {
  if (!env || env.kind === "local") return null;
  if (env.kind === "ssh") return `ssh ${env.user ? `${env.user}@` : ""}${env.host}${env.port && env.port !== 22 ? `:${env.port}` : ""}`;
  return `docker ${env.container}${env.context ? ` (${env.context})` : ""}`;
}

export interface WorkspacesLayout {
  groups: string[];
  folders: Array<{ name: string; group?: string | null }>;
  scenes: Record<string, WorkspaceMeta>;
}

export const workspacesLayout = () => invoke<WorkspacesLayout>("workspaces_layout");
/** Sets (or clears, with null) a workspace's working directory; returns the absolute path. */
export const workspaceSetDir = (name: string, dir: string | null) =>
  invoke<string | null>("workspace_set_dir", { name, dir });

export interface FloorInfo {
  dir: string | null;
  worktree: string | null;
  branch: string | null;
  copy: boolean;
  pull_request: PullRequestInfo | null;
  /** Default name ("#42 Title" for a pull request) */
  name: string | null;
  /** Why the floor isn't git-isolated, when it isn't */
  note: string | null;
}

export const floorPrepare = (opts: {
  root: string | null; scene: string; name: string; branch?: string; existingBranch?: boolean; noGit?: boolean;
  copy?: boolean; pullRequest?: string; takenPrs?: number[];
}) =>
  invoke<FloorInfo>("floor_prepare", {
    root: opts.root, scene: opts.scene, name: opts.name, branch: opts.branch ?? null,
    existingBranch: opts.existingBranch ?? false, noGit: opts.noGit ?? false,
    copy: opts.copy ?? false, pullRequest: opts.pullRequest ?? null, takenPrs: opts.takenPrs ?? [],
  });
export const floorStatus = (worktree: string) => invoke<{ dirty: boolean; branch: string | null }>("floor_status", { worktree });
export const floorRemove = (worktree: string, force: boolean) => invoke<void>("floor_remove", { worktree, force });
/** Merges the floor's branch (optionally into `into`) and removes its worktree; returns a summary. */
export const floorLand = (opts: {
  worktree: string; branch: string; into?: string; deleteBranch?: boolean; keepFloor?: boolean; root: string | null;
}) =>
  invoke<string>("floor_land", {
    worktree: opts.worktree, branch: opts.branch, into: opts.into ?? null,
    deleteBranch: opts.deleteBranch ?? false, keepFloor: opts.keepFloor ?? false, root: opts.root,
  });
/** Removes the floor's worktree and, unless keepBranch, its branch; returns a summary. */
export const floorDelete = (opts: { worktree: string; branch: string | null; keepBranch: boolean; force: boolean }) =>
  invoke<string>("floor_delete", opts);
/** Local branches of a floor's repository, the ground checkout's first. */
export const floorBranches = (worktree: string, root: string | null) => invoke<string[]>("floor_branches", { worktree, root });
/** "OPEN · review: APPROVED · checks: 5 passed · mergeable: MERGEABLE" */
export const floorPrStatus = (worktree: string, pullRequest: PullRequestInfo) =>
  invoke<string>("floor_pr_status", { worktree, pullRequest });
/** Merges the floor's pull request on the host (gh/glab). */
export const floorPrMerge = (worktree: string, pullRequest: PullRequestInfo) =>
  invoke<string>("floor_pr_merge", { worktree, pullRequest });

// ---- NarraTer Chat ----

export const CHAT_THREADS = ["blue", "purple", "pink", "red", "orange", "yellow", "green"] as const;
export type ChatThread = (typeof CHAT_THREADS)[number];

export interface ChatAttachment {
  id: string;
  name: string;
  path: string;
  image: boolean;
}

export interface ChatEntry {
  id: string;
  ts: number;
  role: "user" | "agent";
  /** `narrater say --progress` — an interim update */
  progress: boolean;
  text: string;
  attachments: ChatAttachment[];
}

/** Sends a chat message (stored, then typed into the agent's terminal when it's idle). */
export const chatSend = (terminalId: string, thread: ChatThread, text: string, attachments: string[]) =>
  invoke<void>("chat_send", { terminalId, thread, text, attachments });
export const chatHistory = (terminalId: string, thread: ChatThread) =>
  invoke<ChatEntry[]>("chat_history", { terminalId, thread });
export const chatThreads = (terminalId: string) =>
  invoke<Array<{ thread: ChatThread; count: number; last_ts: number | null }>>("chat_threads", { terminalId });
export const chatClear = (terminalId: string, thread: ChatThread) => invoke<void>("chat_clear", { terminalId, thread });
/** Opens a local file with the desktop's default app. */
export const openPath = (path: string) => invoke<void>("open_path", { path });

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

// ── Agent usage (usage.rs; docs/design.md "Uso dos agentes") ────────────

export interface UsageWindow {
  used_percent: number;
  /** Unix epoch seconds. */
  resets_at: number | null;
}

/** Account-level limits of a provider (Claude: 5-hour and 7-day windows). */
export interface ProviderUsage {
  provider: string;
  five_hour: UsageWindow | null;
  seven_day: UsageWindow | null;
  /** Epoch ms of the report. */
  updated: number;
}

/** What one agent session reported about itself through its status line. */
export interface SessionUsage {
  terminal: string;
  context_percent: number | null;
  cost_usd: number | null;
  model: string | null;
  updated: number;
}

export const usageSnapshot = () =>
  invoke<{ providers: ProviderUsage[]; sessions: SessionUsage[] }>("usage_snapshot");

// --- Activity log (activity.rs) -------------------------------------------

/** One event of the activity log, written by the backend as it happens. */
export interface ActivityEvent {
  ts: number;
  kind: "state" | "ask" | "delivered" | "answer" | "message" | "spawn" | "exit" | "note";
  terminal: string;
  label: string;
  peer?: string;
  peer_label?: string;
  status?: string;
  ask?: string;
  text?: string;
  detail?: string;
}

export const activityList = (limit?: number, terminal?: string) =>
  invoke<ActivityEvent[]>("activity_list", { limit, terminal });

export const activityClear = () => invoke<void>("activity_clear");

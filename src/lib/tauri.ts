import { invoke } from "@tauri-apps/api/core";

export type AgentType = "shell" | "claude" | "codex" | "custom";

export interface PtySpawnOptions {
  id: string;
  command: string;
  args?: string[];
  cols: number;
  rows: number;
  label?: string;
  agentType?: AgentType;
  env?: Record<string, string>;
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
  msg_id?: string | null;
}

/** Inter-agent message ledger record (narrater_msg event). */
export interface LedgerEntry {
  from: string;
  to: string;
  from_label: string;
  to_label: string;
  kind: "send" | "ask" | "reply" | "broadcast";
  msg: string;
  msg_id?: string | null;
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

// Queues a system notification for an AI terminal (idle-gated, auto-submitted
// as "[narrater system]: ..."). Never call for shell targets.
export const ptyNotify = (id: string, text: string) =>
  invoke<void>("pty_notify", { id, text });

// Mirrors the canvas agent-pipe edges to the backend routing table
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

// Resolves a canvas request coming from an agent (canvas_request event)
export const canvasRespond = (reqId: string, result: string) =>
  invoke<void>("canvas_respond", { reqId, result });

export interface Role {
  id: string;
  name: string;
  color: string;
  instructions: string;
  /** Delegate-only: never executes tasks itself (claude spawns lose the execution tools). */
  orchestrator?: boolean;
}

export const loadRoles = () => invoke<Role[]>("load_roles");
export const saveRoles = (roles: Role[]) => invoke<void>("save_roles", { roles });

export interface SpawnSpec {
  command: string;
  args?: string[];
}

const NARRATER_MCP_CONFIG = '{"mcpServers":{"narrater":{"command":"narrater-mcp"}}}';

export function getSpawnSpec(
  agentType: AgentType,
  customCommand?: string,
  systemPrompt?: string,
  skipPermissions?: boolean,
  orchestrator?: boolean,
): SpawnSpec {
  const shell = "/bin/bash";
  switch (agentType) {
    case "shell":
      return { command: shell };
    case "claude": {
      // Narrater exposed as native MCP tools, pre-approved (plus the CLI as
      // fallback); identity/role/protocol go in the system prompt
      const args = [
        "--mcp-config", NARRATER_MCP_CONFIG,
        "--allowedTools", "mcp__narrater", "Bash(narrater)", "Bash(narrater:*)",
      ];
      if (orchestrator) {
        // Delegate-only role: no execution tools — deny rules also cover
        // subagents, so the leader can't route around them via Task. It keeps
        // read tools (for context) and the narrater MCP tools (its real job).
        args.push("--disallowedTools", "Bash", "Edit", "Write", "NotebookEdit");
      }
      if (skipPermissions) {
        args.push("--dangerously-skip-permissions");
      }
      if (systemPrompt?.trim()) {
        args.push("--append-system-prompt", systemPrompt);
      }
      return { command: "claude", args };
    }
    case "codex": {
      // Parity with claude: narrater as MCP server via config override
      // (-c takes dotted TOML keys; narrater-mcp is on the PATH)
      const args = ["-c", "mcp_servers.narrater.command=narrater-mcp"];
      if (skipPermissions) {
        args.push("--dangerously-bypass-approvals-and-sandbox");
      }
      return { command: "codex", args };
    }
    case "custom":
      return { command: customCommand ?? shell };
  }
}

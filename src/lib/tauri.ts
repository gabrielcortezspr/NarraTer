import { invoke } from "@tauri-apps/api/core";

export type AgentType = "shell" | "claude" | "codex" | "custom";

export interface PtySpawnOptions {
  id: string;
  command: string;
  cols: number;
  rows: number;
  label?: string;
  agentType?: AgentType;
  env?: Record<string, string>;
}

export interface HistoriaNode {
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
}

export interface HistoriaEdge {
  id: string;
  source: string;
  target: string;
  edge_type?: string;
  source_handle?: string;
  target_handle?: string;
}

export interface HistoriaData {
  nodes: HistoriaNode[];
  edges: HistoriaEdge[];
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

// Mirrors the canvas agent-pipe edges to the backend routing table
export const connectionsSync = (connections: Array<[string, string]>) =>
  invoke<void>("connections_sync", { connections });

export const loadHistoria = (name: string) =>
  invoke<HistoriaData>("load_historia", { name });

export const saveHistoria = (name: string, data: HistoriaData) =>
  invoke<void>("save_historia", { name, data });

export const listHistorias = () =>
  invoke<string[]>("list_historias");

export const deleteHistoria = (name: string) =>
  invoke<void>("delete_historia", { name });

export const renameHistoria = (oldName: string, newName: string) =>
  invoke<void>("rename_historia", { old_name: oldName, new_name: newName });

export const openInEditor = (editor: string, path: string) =>
  invoke<void>("open_in_editor", { editor, path });

export interface Role {
  id: string;
  name: string;
  color: string;
  instructions: string;
}

export const loadRoles = () => invoke<Role[]>("load_roles");
export const saveRoles = (roles: Role[]) => invoke<void>("save_roles", { roles });

export function getCommandForAgent(agentType: AgentType, customCommand?: string): string {
  const shell = "/bin/bash";
  switch (agentType) {
    case "shell": return shell;
    // Pre-approve narrater so agents can message peers without permission prompts
    case "claude": return "claude --allowedTools Bash(narrater) Bash(narrater:*)";
    case "codex": return "codex";
    case "custom": return customCommand ?? shell;
  }
}

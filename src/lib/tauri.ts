import { invoke } from "@tauri-apps/api/core";

export type AgentType = "shell" | "claude" | "codex" | "custom";

export interface PtySpawnOptions {
  id: string;
  command: string;
  cols: number;
  rows: number;
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
}

export interface HistoriaEdge {
  id: string;
  source: string;
  target: string;
}

export interface HistoriaData {
  nodes: HistoriaNode[];
  edges: HistoriaEdge[];
}

export const ptySpawn = (opts: PtySpawnOptions) =>
  invoke<void>("pty_spawn", opts as unknown as Record<string, unknown>);

export const ptyWrite = (id: string, data: string) =>
  invoke<void>("pty_write", { id, data });

export const ptyResize = (id: string, cols: number, rows: number) =>
  invoke<void>("pty_resize", { id, cols, rows });

export const ptyKill = (id: string) =>
  invoke<void>("pty_kill", { id });

export const loadHistoria = (name: string) =>
  invoke<HistoriaData>("load_historia", { name });

export const saveHistoria = (name: string, data: HistoriaData) =>
  invoke<void>("save_historia", { name, data });

export const listHistorias = () =>
  invoke<string[]>("list_historias");

export function getCommandForAgent(agentType: AgentType, customCommand?: string): string {
  const shell = "/bin/bash";
  switch (agentType) {
    case "shell": return shell;
    case "claude": return "claude";
    case "codex": return "codex";
    case "custom": return customCommand ?? shell;
  }
}

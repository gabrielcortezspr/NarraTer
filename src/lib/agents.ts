import { Terminal, Bot, Code2, Wrench, Sparkles, Braces, type LucideIcon } from "lucide-react";

// Agent presets as data. The communication protocol is the same for every
// one of them (CLI + skill + screen-state), so a preset only says how to
// launch the program and how to show it.
export type AgentType = "shell" | "claude" | "codex" | "gemini" | "opencode" | "custom";

// Identity colours (docs/design.md, "Identidade dos agentes"): shown only on
// the agent icon and the tile's top thread, never as state. Hues stay clear
// of the state colours where the vendor's brand allows.
export interface AgentPreset {
  label: string;
  description: string;
  color: string;
  Icon: LucideIcon;
  /** Program launched (custom terminals bring their own command). */
  command?: string;
  /** Flag that bypasses the agent's permission prompts, when it has one. */
  skipPermissionsArgs?: string[];
}

export const AGENT_PRESETS: Record<AgentType, AgentPreset> = {
  shell: {
    label: "Shell",
    description: "bash / zsh / fish — default terminal",
    color: "#8e8e93",
    Icon: Terminal,
  },
  claude: {
    label: "Claude Code",
    description: "Anthropic's coding agent",
    color: "#d97757",
    Icon: Bot,
    command: "claude",
    skipPermissionsArgs: ["--dangerously-skip-permissions"],
  },
  codex: {
    label: "Codex",
    description: "OpenAI's coding agent",
    color: "#d1d1d6",
    Icon: Code2,
    command: "codex",
    skipPermissionsArgs: ["--dangerously-bypass-approvals-and-sandbox"],
  },
  gemini: {
    label: "Gemini CLI",
    description: "Google's coding agent",
    color: "#5b9dff",
    Icon: Sparkles,
    command: "gemini",
    skipPermissionsArgs: ["--yolo"],
  },
  opencode: {
    label: "OpenCode",
    description: "Open-source coding agent",
    color: "#63e6be",
    Icon: Braces,
    command: "opencode",
  },
  custom: {
    label: "Custom",
    description: "Any command (aider, qwen, agy…)",
    color: "#ff7eb6",
    Icon: Wrench,
  },
};

export const AGENT_ORDER: AgentType[] = ["shell", "claude", "codex", "gemini", "opencode", "custom"];

/** Unknown types from older scenes fall back to the custom preset. */
export function presetOf(type: string | undefined): AgentPreset {
  return AGENT_PRESETS[(type ?? "shell") as AgentType] ?? AGENT_PRESETS.custom;
}

export interface SpawnSpec {
  command: string;
  args?: string[];
}

/**
 * How to launch an agent. Deliberately free of protocol plumbing: no system
 * prompt, no MCP config, no hooks — agents learn NarraTer from the installed
 * skill and their role files, and talk through the `narrater` CLI.
 */
export function getSpawnSpec(
  agentType: AgentType,
  customCommand?: string,
  skipPermissions?: boolean,
  orchestrator?: boolean,
): SpawnSpec {
  if (agentType === "shell") return { command: "/bin/bash" };
  if (agentType === "custom") return { command: customCommand?.trim() || "/bin/bash" };

  const preset = AGENT_PRESETS[agentType];
  const args: string[] = [];
  if (agentType === "claude") {
    // Pre-approve the narrater CLI so teammates can talk without a prompt
    args.push("--allowedTools", "Bash(narrater:*)");
    // Delegate-only role: no file-editing tools (deny rules also cover
    // subagents). Bash stays — `narrater` runs through it.
    if (orchestrator) args.push("--disallowedTools", "Edit", "Write", "NotebookEdit");
  }
  if (skipPermissions && preset.skipPermissionsArgs) args.push(...preset.skipPermissionsArgs);
  return { command: preset.command ?? agentType, args };
}

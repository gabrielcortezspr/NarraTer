import { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Terminal, Bot, Code2, Wrench, X, ChevronDown, Clock, Users, ShieldOff } from "lucide-react";
import { useRolesStore } from "@/stores/roles";
import type { AgentType } from "@/lib/tauri";

interface AgentOption {
  type: AgentType;
  icon: React.ReactNode;
  label: string;
  description: string;
  color: string;
  borderColor: string;
}

const AGENTS: AgentOption[] = [
  {
    type: "shell",
    icon: <Terminal size={20} />,
    label: "Shell",
    description: "bash / zsh / fish — default terminal",
    color: "text-gray-400",
    borderColor: "border-gray-600 hover:border-gray-400",
  },
  {
    type: "claude",
    icon: <Bot size={20} />,
    label: "Claude Code",
    description: "Anthropic's AI agent",
    color: "text-purple-400",
    borderColor: "border-purple-700 hover:border-purple-400",
  },
  {
    type: "codex",
    icon: <Code2 size={20} />,
    label: "Codex",
    description: "OpenAI's AI agent",
    color: "text-blue-400",
    borderColor: "border-blue-700 hover:border-blue-400",
  },
  {
    type: "custom",
    icon: <Wrench size={20} />,
    label: "Custom",
    description: "Custom command",
    color: "text-teal-400",
    borderColor: "border-teal-700 hover:border-teal-400",
  },
];

/** Agents that support bypassing their permission/approval prompts. */
const SKIP_PERMISSIONS_SUPPORT: Partial<Record<AgentType, string>> = {
  claude: "claude --dangerously-skip-permissions",
  codex: "codex --dangerously-bypass-approvals-and-sandbox",
};

export interface AgentPickerResult {
  agentType: AgentType;
  command?: string;
  instructions?: string;
  scheduleCommand?: string;
  scheduleIntervalSecs?: number;
  roleId?: string;
  roleName?: string;
  roleColor?: string;
  skipPermissions?: boolean;
}

interface Props {
  open: boolean;
  onConfirm: (result: AgentPickerResult) => void;
  onClose: () => void;
}

export default function AgentPicker({ open, onConfirm, onClose }: Props) {
  const { roles, loaded, load } = useRolesStore();
  const [selected, setSelected] = useState<AgentType>("shell");
  const [customCommand, setCustomCommand] = useState("");
  const [selectedRoleId, setSelectedRoleId] = useState<string>("");
  const [instructions, setInstructions] = useState("");
  const [skipPermissions, setSkipPermissions] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [scheduleCommand, setScheduleCommand] = useState("");
  const [scheduleInterval, setScheduleInterval] = useState("60");

  useEffect(() => {
    if (!loaded) load();
  }, [loaded, load]);

  // When role changes, fill instructions from role
  const handleRoleChange = (roleId: string) => {
    setSelectedRoleId(roleId);
    if (roleId) {
      const role = roles.find((r) => r.id === roleId);
      if (role) setInstructions(role.instructions);
    } else {
      setInstructions("");
    }
  };

  const selectedRole = roles.find((r) => r.id === selectedRoleId);
  const skipPermissionsFlag = SKIP_PERMISSIONS_SUPPORT[selected];

  const handleConfirm = () => {
    const intervalSecs = scheduleCommand.trim() ? parseInt(scheduleInterval) || 60 : undefined;
    onConfirm({
      agentType: selected,
      command: selected === "custom" ? customCommand : undefined,
      instructions: instructions.trim() || undefined,
      scheduleCommand: scheduleCommand.trim() || undefined,
      scheduleIntervalSecs: intervalSecs,
      roleId: selectedRoleId || undefined,
      roleName: selectedRole?.name,
      roleColor: selectedRole?.color,
      skipPermissions: skipPermissionsFlag ? skipPermissions : undefined,
    });
    // reset
    setSelected("shell");
    setCustomCommand("");
    setSelectedRoleId("");
    setInstructions("");
    setSkipPermissions(false);
    setShowAdvanced(false);
    setScheduleCommand("");
    setScheduleInterval("60");
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
            className="relative z-10 w-[460px] bg-[#1a1a1a] border border-[#2a2a2a] rounded-xl shadow-2xl p-6 max-h-[90vh] overflow-y-auto"
            initial={{ scale: 0.95, y: 8 }}
            animate={{ scale: 1, y: 0 }}
            exit={{ scale: 0.95, y: 8 }}
            transition={{ duration: 0.15, ease: "easeOut" }}
          >
            {/* Header */}
            <div className="flex items-center justify-between mb-5">
              <div>
                <h2 className="text-white font-semibold text-base">New Terminal</h2>
                <p className="text-[#6b7280] text-xs mt-0.5">Choose the agent type</p>
              </div>
              <button onClick={onClose} className="text-[#6b7280] hover:text-white transition-colors p-1 rounded-md hover:bg-[#2a2a2a]">
                <X size={16} />
              </button>
            </div>

            {/* Role picker */}
            {roles.length > 0 && (
              <div className="mb-4">
                <label className="flex items-center gap-1.5 text-[#9ca3af] text-xs mb-2">
                  <Users size={11} /> Agent role
                </label>
                <div className="flex gap-1.5 flex-wrap">
                  <button
                    onClick={() => handleRoleChange("")}
                    className={`px-2.5 py-1 rounded-full text-xs border transition-all ${
                      !selectedRoleId
                        ? "bg-[#2a2a2a] border-[#3a3a3a] text-white"
                        : "border-[#2a2a2a] text-[#555] hover:text-[#888] hover:border-[#333]"
                    }`}
                  >
                    No role
                  </button>
                  {roles.map((role) => (
                    <button
                      key={role.id}
                      onClick={() => handleRoleChange(role.id)}
                      className={`flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs border transition-all ${
                        selectedRoleId === role.id
                          ? "text-white"
                          : "border-[#2a2a2a] text-[#555] hover:text-[#888]"
                      }`}
                      style={
                        selectedRoleId === role.id
                          ? { background: `${role.color}20`, borderColor: role.color, color: role.color }
                          : {}
                      }
                    >
                      <span className="w-1.5 h-1.5 rounded-full" style={{ background: role.color }} />
                      {role.name}
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* Agent type grid */}
            <div className="grid grid-cols-2 gap-2 mb-4">
              {AGENTS.map((agent) => (
                <button
                  key={agent.type}
                  onClick={() => setSelected(agent.type)}
                  className={`flex flex-col items-start gap-1.5 p-3 rounded-lg border transition-all text-left
                    ${selected === agent.type
                      ? `${agent.borderColor} bg-[#222] shadow-md`
                      : "border-[#2a2a2a] hover:border-[#3a3a3a] hover:bg-[#1f1f1f]"
                    }`}
                >
                  <span className={agent.color}>{agent.icon}</span>
                  <span className="text-white text-sm font-medium">{agent.label}</span>
                  <span className="text-[#6b7280] text-xs leading-tight">{agent.description}</span>
                </button>
              ))}
            </div>

            {/* Custom command */}
            <AnimatePresence>
              {selected === "custom" && (
                <motion.div
                  className="mb-4"
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                >
                  <label className="block text-[#9ca3af] text-xs mb-1.5">Command</label>
                  <input
                    type="text"
                    value={customCommand}
                    onChange={(e) => setCustomCommand(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleConfirm()}
                    placeholder="/usr/bin/python3"
                    className="w-full bg-[#111] border border-[#333] text-white text-sm rounded-lg px-3 py-2
                      focus:outline-none focus:border-[#8b5cf6] font-mono placeholder-[#444]"
                    autoFocus
                  />
                </motion.div>
              )}
            </AnimatePresence>

            {/* Skip permissions (claude / codex) */}
            <AnimatePresence>
              {skipPermissionsFlag && (
                <motion.div
                  className="mb-4"
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                >
                  <label
                    className={`flex items-start gap-2.5 p-3 rounded-lg border cursor-pointer transition-all ${
                      skipPermissions
                        ? "border-amber-600/60 bg-amber-950/20"
                        : "border-[#2a2a2a] hover:border-[#3a3a3a]"
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={skipPermissions}
                      onChange={(e) => setSkipPermissions(e.target.checked)}
                      className="mt-0.5 accent-amber-500"
                    />
                    <div className="flex-1">
                      <span className={`flex items-center gap-1.5 text-xs font-medium ${skipPermissions ? "text-amber-400" : "text-[#9ca3af]"}`}>
                        <ShieldOff size={12} /> Skip permission prompts
                      </span>
                      <p className="text-[#6b7280] text-[10px] mt-1 leading-snug">
                        Runs <code className="font-mono text-[#888]">{skipPermissionsFlag}</code>.
                        The agent acts without asking for approval — use only in trusted directories.
                      </p>
                    </div>
                  </label>
                </motion.div>
              )}
            </AnimatePresence>

            {/* Advanced */}
            <button
              onClick={() => setShowAdvanced((v) => !v)}
              className="flex items-center gap-1.5 text-[#555] hover:text-[#888] transition-colors text-xs mb-2 w-full"
            >
              <motion.span animate={{ rotate: showAdvanced ? 180 : 0 }} transition={{ duration: 0.15 }}>
                <ChevronDown size={12} />
              </motion.span>
              Advanced settings
            </button>

            <AnimatePresence>
              {showAdvanced && (
                <motion.div
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                  className="mb-4 space-y-3"
                >
                  <div>
                    <label className="block text-[#9ca3af] text-xs mb-1.5">Initial instructions</label>
                    <textarea
                      value={instructions}
                      onChange={(e) => setInstructions(e.target.value)}
                      placeholder="E.g.: Always write tests for new code."
                      rows={3}
                      className="w-full bg-[#111] border border-[#333] text-white text-xs rounded-lg px-3 py-2
                        focus:outline-none focus:border-[#8b5cf6] placeholder-[#444] resize-none"
                    />
                    <p className="text-[#444] text-[10px] mt-1">
                      {selectedRole ? `Filled from role "${selectedRole.name}" — editable` : "Sent as the first input to the PTY"}
                    </p>
                  </div>

                  <div className="border-t border-[#2a2a2a] pt-3">
                    <div className="flex items-center gap-1.5 mb-2">
                      <Clock size={12} className="text-[#555]" />
                      <label className="text-[#9ca3af] text-xs">Scheduled prompt</label>
                    </div>
                    <input
                      type="text"
                      value={scheduleCommand}
                      onChange={(e) => setScheduleCommand(e.target.value)}
                      placeholder="E.g.: git status"
                      className="w-full bg-[#111] border border-[#333] text-white text-xs rounded-lg px-3 py-2
                        focus:outline-none focus:border-[#8b5cf6] font-mono placeholder-[#444] mb-2"
                    />
                    {scheduleCommand.trim() && (
                      <div className="flex items-center gap-2">
                        <label className="text-[#9ca3af] text-xs shrink-0">Every</label>
                        <input
                          type="number"
                          value={scheduleInterval}
                          onChange={(e) => setScheduleInterval(e.target.value)}
                          min="5"
                          className="w-20 bg-[#111] border border-[#333] text-white text-xs rounded-lg px-2 py-1.5
                            focus:outline-none focus:border-[#8b5cf6] text-center"
                        />
                        <label className="text-[#9ca3af] text-xs">seconds</label>
                      </div>
                    )}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>

            {/* Actions */}
            <div className="flex gap-2 justify-end">
              <button
                onClick={onClose}
                className="px-4 py-2 text-sm text-[#9ca3af] hover:text-white transition-colors rounded-lg hover:bg-[#2a2a2a]"
              >
                Cancel
              </button>
              <button
                onClick={handleConfirm}
                disabled={selected === "custom" && !customCommand.trim()}
                className="px-4 py-2 text-sm bg-[#8b5cf6] hover:bg-[#7c3aed] text-white rounded-lg
                  transition-colors disabled:opacity-40 disabled:cursor-not-allowed font-medium"
                style={
                  selectedRole
                    ? { background: selectedRole.color }
                    : {}
                }
              >
                Create Terminal
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Terminal, Bot, Code2, Wrench, X } from "lucide-react";
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
    description: "bash / zsh / fish — terminal padrão",
    color: "text-gray-400",
    borderColor: "border-gray-600 hover:border-gray-400",
  },
  {
    type: "claude",
    icon: <Bot size={20} />,
    label: "Claude Code",
    description: "Agente de IA da Anthropic",
    color: "text-purple-400",
    borderColor: "border-purple-700 hover:border-purple-400",
  },
  {
    type: "codex",
    icon: <Code2 size={20} />,
    label: "Codex",
    description: "Agente de IA da OpenAI",
    color: "text-blue-400",
    borderColor: "border-blue-700 hover:border-blue-400",
  },
  {
    type: "custom",
    icon: <Wrench size={20} />,
    label: "Custom",
    description: "Comando personalizado",
    color: "text-teal-400",
    borderColor: "border-teal-700 hover:border-teal-400",
  },
];

interface Props {
  open: boolean;
  onConfirm: (agentType: AgentType, command?: string) => void;
  onClose: () => void;
}

export default function AgentPicker({ open, onConfirm, onClose }: Props) {
  const [selected, setSelected] = useState<AgentType>("shell");
  const [customCommand, setCustomCommand] = useState("");

  const handleConfirm = () => {
    onConfirm(selected, selected === "custom" ? customCommand : undefined);
    setSelected("shell");
    setCustomCommand("");
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
          {/* Backdrop */}
          <motion.div
            className="absolute inset-0 bg-black/60 backdrop-blur-sm"
            onClick={onClose}
          />

          {/* Modal */}
          <motion.div
            className="relative z-10 w-[440px] bg-[#1a1a1a] border border-[#2a2a2a] rounded-xl shadow-2xl p-6"
            initial={{ scale: 0.95, y: 8 }}
            animate={{ scale: 1, y: 0 }}
            exit={{ scale: 0.95, y: 8 }}
            transition={{ duration: 0.15, ease: "easeOut" }}
          >
            {/* Header */}
            <div className="flex items-center justify-between mb-5">
              <div>
                <h2 className="text-white font-semibold text-base">Nova Cena</h2>
                <p className="text-[#6b7280] text-xs mt-0.5">Escolha o tipo de agente</p>
              </div>
              <button
                onClick={onClose}
                className="text-[#6b7280] hover:text-white transition-colors p-1 rounded-md hover:bg-[#2a2a2a]"
              >
                <X size={16} />
              </button>
            </div>

            {/* Agent options */}
            <div className="grid grid-cols-2 gap-2 mb-4">
              {AGENTS.map((agent) => (
                <button
                  key={agent.type}
                  onClick={() => setSelected(agent.type)}
                  className={`flex flex-col items-start gap-1.5 p-3 rounded-lg border transition-all text-left
                    ${selected === agent.type
                      ? `${agent.borderColor} bg-[#222] shadow-md`
                      : `border-[#2a2a2a] hover:border-[#3a3a3a] hover:bg-[#1f1f1f]`
                    }`}
                >
                  <span className={agent.color}>{agent.icon}</span>
                  <span className="text-white text-sm font-medium">{agent.label}</span>
                  <span className="text-[#6b7280] text-xs leading-tight">{agent.description}</span>
                </button>
              ))}
            </div>

            {/* Custom command input */}
            {selected === "custom" && (
              <motion.div
                className="mb-4"
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: "auto" }}
                exit={{ opacity: 0, height: 0 }}
              >
                <label className="block text-[#9ca3af] text-xs mb-1.5">Comando</label>
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

            {/* Actions */}
            <div className="flex gap-2 justify-end">
              <button
                onClick={onClose}
                className="px-4 py-2 text-sm text-[#9ca3af] hover:text-white transition-colors rounded-lg hover:bg-[#2a2a2a]"
              >
                Cancelar
              </button>
              <button
                onClick={handleConfirm}
                disabled={selected === "custom" && !customCommand.trim()}
                className="px-4 py-2 text-sm bg-[#8b5cf6] hover:bg-[#7c3aed] text-white rounded-lg
                  transition-colors disabled:opacity-40 disabled:cursor-not-allowed font-medium"
              >
                Criar Terminal
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

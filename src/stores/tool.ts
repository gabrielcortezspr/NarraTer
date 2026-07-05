import { create } from "zustand";

export type Tool =
  | "select"
  | "terminal"
  | "note"
  | "text"
  | "files"
  | "attachment"
  | "portal"
  | "draw";

// Ferramentas em que o próximo clique no pane posiciona um nó novo.
export const PLACEMENT_TOOLS: ReadonlySet<Tool> = new Set(["note", "text", "files", "portal"]);

interface ToolStore {
  active: Tool;
  setTool: (tool: Tool) => void;
}

export const useToolStore = create<ToolStore>((set) => ({
  active: "select",
  setTool: (active) => set({ active }),
}));

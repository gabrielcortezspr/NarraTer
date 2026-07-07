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

// Tools where the next click on the pane places a new node.
export const PLACEMENT_TOOLS: ReadonlySet<Tool> = new Set(["note", "text", "files", "portal"]);

interface ToolStore {
  active: Tool;
  setTool: (tool: Tool) => void;
}

export const useToolStore = create<ToolStore>((set) => ({
  active: "select",
  setTool: (active) => set({ active }),
}));

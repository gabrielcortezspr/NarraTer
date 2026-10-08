import { create } from "zustand";
import { useCanvasStore } from "@/stores/canvas";

// The pen tool: colour, size and the stroke being drawn. Finished strokes
// belong to the scene (canvas store `drawings`, saved with it, per floor).

export interface Stroke {
  points: number[][];
  color: string;
  size: number;
}

interface SketchStore {
  currentStroke: number[][];
  isDrawing: boolean;
  color: string;
  size: number;
  addPoint: (point: number[]) => void;
  commitStroke: () => void;
  cancelStroke: () => void;
  undo: () => void;
  clear: () => void;
  setColor: (color: string) => void;
  setSize: (size: number) => void;

}

export const useSketchStore = create<SketchStore>((set, get) => ({
  currentStroke: [],
  isDrawing: false,
  color: "#8b5cf6",
  size: 4,

  addPoint: (point) =>
    set((s) => ({
      currentStroke: [...s.currentStroke, point],
      isDrawing: true,
    })),

  commitStroke: () => {
    const { currentStroke, color, size } = get();
    if (currentStroke.length < 2) {
      set({ currentStroke: [], isDrawing: false });
      return;
    }
    useCanvasStore.getState().addDrawing({ points: currentStroke, color, size });
    set({ currentStroke: [], isDrawing: false });
  },

  cancelStroke: () => set({ currentStroke: [], isDrawing: false }),

  undo: () => useCanvasStore.getState().undoDrawing(),

  clear: () => {
    useCanvasStore.getState().clearDrawings();
    set({ currentStroke: [], isDrawing: false });
  },

  setColor: (color) => set({ color }),
  setSize: (size) => set({ size }),

}));

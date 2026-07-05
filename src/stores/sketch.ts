import { create } from "zustand";

export interface Stroke {
  points: number[][];
  color: string;
  size: number;
}

interface SketchStore {
  strokes: Stroke[];
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
  loadStrokes: (strokes: Stroke[]) => void;
}

export const useSketchStore = create<SketchStore>((set, get) => ({
  strokes: [],
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
    set((s) => ({
      strokes: [...s.strokes, { points: currentStroke, color, size }],
      currentStroke: [],
      isDrawing: false,
    }));
  },

  cancelStroke: () => set({ currentStroke: [], isDrawing: false }),

  undo: () => set((s) => ({ strokes: s.strokes.slice(0, -1) })),

  clear: () => set({ strokes: [], currentStroke: [], isDrawing: false }),

  setColor: (color) => set({ color }),
  setSize: (size) => set({ size }),
  loadStrokes: (strokes) => set({ strokes }),
}));

import type { ReactNode } from "react";

/** Tecla de atalho renderizada (sidebar, empty state, tooltips). */
export default function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 rounded border border-canvas-border bg-canvas-tile text-[9px] font-mono text-accent select-none">
      {children}
    </kbd>
  );
}

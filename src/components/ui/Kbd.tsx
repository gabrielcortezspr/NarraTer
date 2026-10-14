import type { ReactNode } from "react";

/** Rendered shortcut key (palette, empty state, tooltips). */
export default function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded-sm border border-line bg-surface-2 text-2xs font-sans font-medium text-ink-2 shadow-[inset_0_-1px_0_rgb(255_255_255/0.04)] select-none">
      {children}
    </kbd>
  );
}

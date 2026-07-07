import type { ReactNode } from "react";

// Icon-only button for headers/panels: stops mousedown propagation (so it
// doesn't start a node drag), consistent hover and aria-label from the title.
interface IconButtonProps {
  title: string;
  onClick: (e: React.MouseEvent) => void;
  children: ReactNode;
  /** Hover color: accent (default), danger (close/delete) or info. */
  intent?: "accent" | "danger" | "info";
  className?: string;
}

const HOVER: Record<NonNullable<IconButtonProps["intent"]>, string> = {
  accent: "hover:text-accent",
  danger: "hover:text-status-exited",
  info: "hover:text-[#60a5fa]",
};

export default function IconButton({ title, onClick, children, intent = "accent", className = "" }: IconButtonProps) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        onClick(e);
      }}
      className={`text-ink-faint ${HOVER[intent]} transition-colors p-0.5 rounded hover:bg-canvas-border nodrag ${className}`}
    >
      {children}
    </button>
  );
}

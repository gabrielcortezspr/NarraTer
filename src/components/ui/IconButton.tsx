import type { ReactNode } from "react";

// Icon-only button for headers/panels: stops mousedown propagation (so it
// doesn't start a node drag), quiet at rest, hover background + full ink,
// aria-label from the title.
interface IconButtonProps {
  title: string;
  onClick: (e: React.MouseEvent) => void;
  children: ReactNode;
  /** danger: close/delete turn red on hover. "info" is kept for older callers. */
  intent?: "accent" | "danger" | "info";
  className?: string;
}

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
      className={`text-ink-muted ${
        intent === "danger" ? "hover:text-danger-text" : "hover:text-ink"
      } hover:bg-hover active:bg-press transition-colors duration-100 p-1 rounded-md nodrag ${className}`}
    >
      {children}
    </button>
  );
}

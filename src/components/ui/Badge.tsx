import type { ReactNode } from "react";

// Pill colorida usada nos headers dos tiles (agente, papel, pipe, agenda…).
// Deriva bg/borda translúcidos da cor — substitui as 4 implementações quase
// idênticas que existiam no TerminalTile.
interface BadgeProps {
  color: string;
  children: ReactNode;
  title?: string;
  rounded?: "full" | "md";
  className?: string;
}

export default function Badge({ color, children, title, rounded = "full", className = "" }: BadgeProps) {
  return (
    <span
      title={title}
      className={`flex items-center gap-1 text-[9px] font-medium px-1.5 py-0.5 shrink-0 select-none ${
        rounded === "full" ? "rounded-full" : "rounded"
      } ${className}`}
      style={{ color, background: `${color}18`, border: `1px solid ${color}30` }}
    >
      {children}
    </span>
  );
}

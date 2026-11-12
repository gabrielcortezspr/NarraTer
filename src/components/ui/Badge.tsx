import type { ReactNode } from "react";

// Coloured pill (agent, role, state…). The tint is derived from the colour with
// color-mix, so it works with tokens (var(--accent)) as well as hex.
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
      className={`flex items-center gap-1 h-[18px] text-2xs font-medium px-1.5 shrink-0 select-none ${
        rounded === "full" ? "rounded-full" : "rounded-sm"
      } ${className}`}
      style={{
        color,
        background: `color-mix(in srgb, ${color} 12%, transparent)`,
        border: `1px solid color-mix(in srgb, ${color} 24%, transparent)`,
      }}
    >
      {children}
    </span>
  );
}

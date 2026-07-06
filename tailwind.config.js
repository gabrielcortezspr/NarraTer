/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Valores vivem nas CSS variables do index.css (tokens de design)
        canvas: {
          bg: "var(--canvas-bg)",
          panel: "var(--canvas-panel)",
          tile: "var(--canvas-tile)",
          header: "var(--canvas-header)",
          border: "var(--canvas-border)",
          hover: "var(--canvas-hover)",
        },
        accent: {
          DEFAULT: "var(--accent)",
          dim: "var(--accent-dim)",
          glow: "#8b5cf620",
        },
        agent: {
          shell: "var(--agent-shell)",
          claude: "var(--agent-claude)",
          codex: "var(--agent-codex)",
          custom: "var(--agent-custom)",
        },
        status: {
          running: "var(--status-running)",
          spawning: "var(--status-spawning)",
          idle: "var(--status-idle)",
          exited: "var(--status-exited)",
        },
        ink: {
          DEFAULT: "var(--text)",
          muted: "var(--text-muted)",
          faint: "var(--text-faint)",
        },
      },
      fontFamily: {
        sans: ["Inter Variable", "-apple-system", "BlinkMacSystemFont", "Segoe UI", "sans-serif"],
        mono: ["JetBrains Mono", "Cascadia Code", "Fira Code", "monospace"],
      },
      boxShadow: {
        tile: "0 4px 24px rgba(0,0,0,0.4), 0 0 0 1px rgba(255,255,255,0.06)",
        "tile-active": "0 4px 32px rgba(139,92,246,0.2), 0 0 0 1px rgba(139,92,246,0.3)",
      },
    },
  },
  plugins: [],
};

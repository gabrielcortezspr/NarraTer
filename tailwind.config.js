/** @type {import('tailwindcss').Config} */
// Cores do perfil "Palco" (docs/design.md). Os valores vivem nas CSS variables do
// index.css; as que aceitam opacidade (bg-accent/15) usam os canais -rgb.
const ch = (name) => `rgb(var(--${name}-rgb) / <alpha-value>)`;

export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        bg: ch("bg"),
        term: ch("term"),
        surface: { DEFAULT: ch("surface"), 2: ch("surface-2") },
        raised: ch("raised"),
        hover: "var(--hover)",
        press: "var(--press)",
        line: { DEFAULT: "var(--line)", strong: "var(--line-strong)" },
        ok: ch("ok"),
        warn: ch("warn"),
        queue: ch("queue"),
        danger: { DEFAULT: ch("danger"), text: "var(--danger-text)" },
        info: ch("info"),
        accent: {
          DEFAULT: ch("accent"),
          text: "var(--accent-text)",
          dim: "var(--accent-dim)",
          glow: "rgb(var(--accent-rgb) / 0.12)",
        },
        ink: {
          DEFAULT: ch("text"),
          2: "var(--text-2)",
          muted: "var(--text-3)",
          faint: "var(--text-4)",
        },
        // Nomes antigos
        canvas: {
          bg: ch("bg"),
          panel: ch("surface"),
          tile: ch("surface"),
          header: ch("surface-2"),
          border: "var(--line)",
          hover: "var(--hover)",
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
      },
      fontFamily: {
        sans: ["Inter Variable", "-apple-system", "BlinkMacSystemFont", "Segoe UI", "sans-serif"],
        mono: ["JetBrains Mono", "Cascadia Code", "Fira Code", "monospace"],
      },
      fontSize: {
        "2xs": ["11px", "14px"],
      },
      borderRadius: {
        sm: "var(--radius-sm)",
        md: "var(--radius-md)",
        lg: "var(--radius-lg)",
        xl: "var(--radius-lg)",
      },
      boxShadow: {
        tile: "var(--shadow-tile)",
        "tile-active": "var(--ring-accent), var(--shadow-tile)",
        float: "var(--shadow-float)",
        ring: "var(--ring-accent)",
      },
      transitionTimingFunction: {
        ease: "var(--ease)",
      },
    },
  },
  plugins: [],
};

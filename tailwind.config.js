/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        canvas: {
          bg: "#0d0d0d",
          tile: "#1a1a1a",
          header: "#222222",
          border: "#2a2a2a",
          hover: "#333333",
        },
        accent: {
          DEFAULT: "#8b5cf6",
          dim: "#6d28d9",
          glow: "#8b5cf620",
        },
        agent: {
          shell: "#6b7280",
          claude: "#8b5cf6",
          codex: "#3b82f6",
          custom: "#14b8a6",
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

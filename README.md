# NarraTer

*Narrator + Terminal.*

Every terminal is a character. A shell that does exactly what it's told. A
Claude that thinks out loud. A leader that gives orders but never touches the
code; a developer that asks its leader — not you — when in doubt. Alone, each
one just runs commands. Together, they need someone to tell the story.

That someone is you. NarraTer hands you the narrator's desk: an infinite
canvas where you place your characters, draw the lines of who may talk to
whom, and set the plot in motion. Delegations travel down the edges, questions
and reports climb back up the chain of command, and notes pile up in the
margins as the agents write down what they've learned. You don't type every
line of the story — you direct it.

And like any good book, a **scene** can be closed and reopened: leave, come
back tomorrow, and your characters are exactly where you left them.

---

In practice: NarraTer is a spatial manager for terminals and AI agents on
Linux — an infinite canvas, like Figma but for terminals, where you position
sessions, wire agents together, and watch them collaborate. Inspired by
[Maestri](https://www.themaestri.app/en) (macOS-only); NarraTer brings the
concept to Linux.

## Features

- **Infinite canvas** with pan/zoom, undo/redo, a floating toolbar and a
  Ctrl+K command palette.
- **Terminals as nodes**: spawn Shell, Claude Code, Codex or custom-command
  terminals anywhere on the canvas (xterm.js frontend, real PTYs in the Rust
  backend, with zoom-aware culling and level-of-detail rendering).
- **Six node types**: terminal, note, text, file tree, attachment and portal
  (embedded web page).
- **Agent-to-agent communication**: draw a directed edge between two terminals
  and the agents can talk. Inside every terminal a `narrater` CLI
  (`send`, `ask`, `reply`, `broadcast`, `inbox`, `peers`, `whoami`) and a
  `narrater-mcp` MCP server expose the same routes as native tools for AI
  agents. Delivery is idle-gated: messages are injected only when the target
  agent isn't in the middle of a turn.
- **Agents can edit the canvas**: MCP tools let an agent list nodes, create
  and update notes, drop text blocks, move nodes and connect them — see
  [docs/mcp-canvas-tools.md](docs/mcp-canvas-tools.md).
- **Roles**: reusable agent role presets (name, color, instructions) you can
  assign to terminals. A role can be flagged **delegate-only (orchestrator)**:
  a claude agent spawned with it loses the execution tools (Bash/Edit/Write)
  and can only coordinate — it must delegate through its team. The default
  roles are **Leader** (delegate-only) and **Developer**, wired for chain of
  command: doubts about a delegated task go back to the delegator, never to
  the user.
- **Scenes**: workspaces are auto-saved (1s debounce, Ctrl+S to flush) as JSON
  under `~/.config/narrater/scenes/` and restored on load.
- **Sticky notes, sketch layer and edge history** for annotating and auditing
  what your agents did.

## Stack

- [Tauri 2](https://tauri.app/) — Rust backend (PTY management,
  Unix-socket IPC between agents, scene persistence)
- React 18 + TypeScript + Vite — frontend
- [@xyflow/react](https://reactflow.dev/) (React Flow) — canvas
- [xterm.js](https://xtermjs.org/) — terminal rendering (WebGL addon)
- Zustand — state (the store is the single source of truth for the canvas)
- Tailwind CSS + Framer Motion — styling and animation

## Running the app

### Prerequisites

- Node.js (18+) and npm
- Rust (stable) — `rustup` recommended
- The [Tauri 2 Linux system dependencies](https://tauri.app/start/prerequisites/).
  On Ubuntu/Debian:

  ```sh
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

### Run (development)

```sh
npm install        # first time only
npm run tauri dev
```

This starts the Vite dev server on `http://localhost:1420` and launches the
desktop window with hot reload. The first run compiles the Rust backend and
takes a few minutes; subsequent runs start in seconds.

### Build (production)

```sh
npm run tauri build
```

The bundled binary and packages land in `src-tauri/target/release/`.

### Tests

```sh
cd src-tauri && cargo test   # Rust backend tests
npx tsc --noEmit             # frontend typecheck
```

## How agent communication works

1. Each terminal is spawned with `NARRATER_ID`, `NARRATER_TOKEN` and
   `NARRATER_SOCKET` in its environment.
2. The backend listens on a per-app Unix socket
   (`/tmp/narrater-<pid>.sock`); the `narrater` CLI and the `narrater-mcp`
   server (both written to `~/.local/bin` at boot) speak JSON over it.
3. Directed canvas edges define who may message whom; receiving a message
   grants a temporary reply route back. Replies to `ask` are matched by short
   ids (`#a3f2`) framed into the injected message
   (`[narrater from X #id]: ...`).
4. For Claude Code agents, a Stop hook (`narrater notify-idle`) provides the
   authoritative idle signal that drains each terminal's message queue.

## Distribution goal

`sudo apt install narrater` — a `.deb` via GitHub Releases plus an APT
repository, and Flatpak for universal Linux distribution.

## License

Not yet defined.

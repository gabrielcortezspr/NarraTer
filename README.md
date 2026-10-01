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
- **Terminals as nodes**: spawn Shell, Claude Code, Codex, Gemini CLI,
  OpenCode or any custom command anywhere on the canvas, each in its own
  working directory (xterm.js frontend, real PTYs in the Rust backend, with
  zoom-aware culling and level-of-detail rendering). Every terminal of a scene
  runs whether or not it's on screen.
- **Six node types**: terminal, note, text, file tree, attachment and portal
  (embedded web page).
- **Agents from any vendor talk to each other**: connect two terminals and
  either side can `narrater ask` the other. The prompt is typed into the
  target's terminal when it's idle, and `ask` returns what the target's
  screen shows once its turn is over — the target needs no plugin, hook or
  special protocol, so Claude can ask Codex, Gemini can ask a shell, and so on.
  See [docs/narrater-cli.md](docs/narrater-cli.md).
- **Shared notes**: agents create, read and edit sticky notes connected to
  them (`narrater note …`) and can wire agents and notes together
  (`narrater connect`).
- **Roles**: reusable role presets (name, color, instructions). A role reaches
  the agent as `CLAUDE.md` + `AGENTS.md` in `<project>/.narrater/roles/<id>/`,
  where the agent is started — every agent CLI loads its instruction file
  natively. A role can be flagged **delegate-only (orchestrator)**: the agent
  only coordinates (Claude Code also loses its file-editing tools). The
  default roles are **Leader** (delegate-only) and **Developer**.
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

Same model as [Maestri](https://www.themaestri.app/en): the terminal is the
universal API, so nothing depends on which AI runs inside it.

1. **Screen model.** Every PTY is mirrored into a headless terminal emulator
   (`vt100`) in the backend (`src-tauri/src/screen.rs`). From the rendered
   screen it derives each terminal's turn state — `working` (screen changing,
   or a busy footer like `esc to interrupt`), `idle` (settled), or
   `attention` (a permission menu / y-n prompt) — and whether the program
   enabled bracketed paste. Shells are `working` while a foreground job runs.
2. **CLI + skill.** At boot NarraTer installs the `narrater` CLI in
   `~/.local/bin` and a `narrater` skill in `~/.claude/skills` and
   `~/.agents/skills` (read by Claude Code, Codex, Gemini CLI, OpenCode…).
   Each terminal gets `NARRATER_ID`, `NARRATER_TOKEN` and `NARRATER_SOCKET`;
   the CLI speaks JSON over a private Unix socket
   (`/tmp/narrater-<pid>/narrater.sock`).
3. **ask.** The prompt waits in the target's queue until it's idle, is typed
   in as `[narrater from X]: …` (bracketed paste when the program enabled
   it), and the call blocks until the target settles; it returns the screen.
   If the target messages the caller back meanwhile (`narrater ask "X" …`),
   that resolves the waiting ask instead (the *ask back* pattern, for answers
   longer than a screen). `check` reads a screen, `--raw` types keys,
   `--batch` fans out in parallel.
4. **Connections** are undirected and only gate who can reach whom.

## Distribution goal

`sudo apt install narrater` — a `.deb` via GitHub Releases plus an APT
repository, and Flatpak for universal Linux distribution.

## License

Not yet defined.

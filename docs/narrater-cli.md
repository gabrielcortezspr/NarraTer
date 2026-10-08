# The `narrater` CLI and protocol

Every terminal on the canvas can reach the agents and notes connected to it
through the `narrater` CLI. It is installed at boot in `~/.local/bin`, and a
`narrater` skill teaching it is installed in `~/.claude/skills` and
`~/.agents/skills`, so any agent CLI that supports skills (Claude Code, Codex,
Gemini CLI, OpenCode…) knows how to use it. The command set mirrors Maestri's
`maestri` CLI.

## Commands

| Command | What it does |
|---|---|
| `narrater list` | You (`You:`), connected agents (status, role) and reachable notes |
| `narrater ask "Agent" "prompt" [--timeout S]` | Types the prompt into the agent's terminal when it's idle; blocks until its turn is over and prints its screen |
| `narrater ask --batch '{"A": "…", "B": "…"}'` | Several asks in parallel; JSON array of `{name, output}` / `{name, error}` |
| `narrater ask "Agent" --raw "2\n"` | Types raw keys (`\n` Enter, `\t`, `\e` ESC, `\xNN`) and prints the resulting screen |
| `narrater check "Agent"` | The agent's current screen, nothing sent |
| `narrater note create ["content"] [--name N]` | New note next to you, connected to you |
| `narrater note read "Note" [offset] [limit]` | Read with line numbers |
| `narrater note write "Note" "content"` | Replace the content |
| `narrater note edit "Note" "old" "new"` | Replace a unique substring |
| `narrater note delete "Note"` | Remove (only on explicit user request) |
| `narrater connect "From" "To"` | Wire two things you can reach |
| `narrater portal create URL ["Name"] [--size WxH]` | Browser portal next to you, connected to you |
| `narrater portal <verb> "Portal" …` | snapshot, click, fill, type, key, navigate, screenshot, evaluate, logs, resize, ua… (`narrater help portal`) |
| `narrater recruit "Name" [--preset P] [--role R] [--command C] [--dir D]` | Maestro: spawn a teammate next to you, connected to you |
| `narrater recruit ["New"] --preset P --replace "Old"` | Maestro: swap the agent of a teammate in place (node, connections, position kept) |
| `narrater dismiss "Name"` | Maestro: stop and remove a connected teammate |
| `narrater preset list` | Maestro: agent presets for `--preset` |
| `narrater role list \| show \| create \| write \| edit \| assign \| delete` | Maestro: manage role presets; `assign` restarts the teammate with the role |
| `narrater notify "message"` | Maestro: desktop notification (notify-send) |
| `narrater routine list \| show \| create \| edit \| enable \| disable \| run \| delete` | Maestro: scheduled prompts/commands into a terminal of the scene, or reminders (`narrater help routine`) |
| `narrater workspace list \| create \| rename \| move` | Maestro: workspaces (scenes with a directory), sidebar groups/folders, `--from` clones |
| `narrater floor list \| create \| land \| delete` | Maestro: floors of this workspace — a git worktree on its own branch when possible; land/delete only from Ground |
| `narrater floor status ["Text"] [--state S] [--progress 2/5]` | Any agent: the status line of its floor (`--floor` is Maestro-only); blocked/review/done notify the user |
| `narrater say "text" \| <<'EOF' \| --file P \| --attach P \| --progress "…"` | NarraTer Chat: reply to the user's chat message (each say is a bubble) |
| `narrater recall [thread] [--turns N \| --last … \| --from D [--to D] \| --all] [--json]` | NarraTer Chat: replay a thread; `recall list`, `recall attachment ID` |
| `narrater debug` | Environment + socket diagnostics |
| `narrater help [command]` | Help |

## Maestro mode

Management commands only work from a terminal in **Maestro mode** — the user
turns it on per terminal (the crown in the tile header, or the option in the
New Terminal dialog). It mirrors Maestri: the user decides which agent may
spawn others. A Maestro's `narrater list` also shows each teammate's own
connections. The `narrater-manager` skill teaches the workflow (reuse before
recruiting, codenames, roles, `--batch` delegation).

A swap (`--replace`, `role assign`) respawns the process under the **same
node id**: the backend kills the old process, drops asks waiting on it, and
its reader thread exits silently (no `pty_exit` for the new session); the
frontend gives the tile a fresh xterm (`sessionGen`).

## Browser portals

A portal is a real Chrome tab (`src-tauri/src/browser/`): NarraTer launches a
headless Chrome/Chromium on first use (Google Chrome, Chromium, Brave, Edge on
PATH, a Playwright Chromium, or `$NARRATER_BROWSER`) with its own profile in
`~/.local/share/narrater/browser-profile`, and drives one tab per portal node
over the DevTools Protocol (a minimal client in `browser/cdp.rs`).

- **The tile** paints the tab's screencast (`Page.startScreencast`, JPEG
  frames via the `portal_frame` event) only while it's on screen, and
  forwards mouse, wheel, keyboard and paste as `Input.*` events — so the page
  scales and moves with the canvas like any node.
- **Agents** use `narrater portal …` on the same tab. `snapshot` runs a
  script that tags interactive elements with `data-narrater-ref` and lists
  them as `@eN` refs (shadow DOM included); actions resolve refs, CSS or
  `x,y`, then use real input events (clicks are mouse presses, typing is
  `Input.insertText`).
- **Access**: a portal is reachable by the terminals wired to it (the canvas
  mirrors portals and links to the backend with `portals_sync`). Portals only
  wire to agents.
- Without Chrome, the tile and the CLI say so plainly; nothing else breaks.

## Android device portals

A portal can show an **Android emulator or phone** instead of a browser tab
(`src-tauri/src/browser/device.rs`, `android.rs`), driven over `adb` from the
SDK (`$ANDROID_HOME`, `$ANDROID_SDK_ROOT` or `~/Android/Sdk`).

- `narrater portal devices` lists phones, running emulators and Android
  Studio's AVDs; `narrater portal create --simulator ID ["Name"]` opens a
  portal on a serial or an AVD — a stopped AVD is **booted headless** (its
  settings untouched; extra emulator flags via `$NARRATER_EMULATOR_ARGS`) and
  stopped again when no portal uses it (it also dies with the app).
- The tile shows the screen (`screencap` polled ~4 fps while visible,
  downscaled JPEG) and turns clicks into taps, drags into swipes (with the
  real gesture duration), the wheel into scrolls, keys and paste into
  `input keyevent` / `input text`; Back/Home/Recents in the header.
- Agents get the maestri-portal-devices verbs: `snapshot` (the
  `uiautomator` tree as `@eN` refs in device pixels; clickable rows are named
  after their texts), `click/tap`, `type` (ASCII), `key`, `scroll`, `swipe`,
  `drag`, `button`, `home`, `lock`, `launch`, `terminate`, `navigate`,
  `screenshot`, `info`. Browser-only verbs are refused, not ignored.

## NarraTer Chat

Talk to an agent without its terminal (`src-tauri/src/chat.rs`,
`src/components/ChatPanel/`): the chat button on an agent's tile opens a
panel with **seven threads by color**. A message is stored, then typed into
the agent's terminal — idle-gated like `ask` — with a header:

```
---
Below is a message from the user through NarraTer Chat.
Reply only via `narrater say`. If the narrater-chat skill isn't already loaded, load it first.
thread: blue
---
<the user's words>
```

The agent answers with `narrater say` (each say is a bubble; `--progress` for
interim updates; `--file` sends a text file or an image; `--attach` files;
`![…](/abs.png)` lines become inline images; an identical reply within 10 s
counts once) and catches up with `narrater recall` after a restart or a
compaction. The panel shows a working indicator while the agent's turn is
open, and the tile an unread badge. Chats are stored per terminal in
`~/.config/narrater/chat/<terminal id>.json`, attachments copied next to them.

## Routines

Scheduled commands — a shell command or an agent prompt — fired into a
terminal of the scene, or desktop reminders (`src-tauri/src/routines/`). The
**scheduler runs in the backend** (1 s tick), so it keeps time whether or not
the window is focused or the tile is on screen; a fire goes through the same
idle-gated queue as `ask`, and by default is *skipped* while the target is
busy. Schedules: `--every` (≥ 10 s), `--daily`, `--weekly`, `--once`;
options `--count`, `--until`, `--pre-run` (its stdout fills `{{output}}`),
`--no-skip-if-busy`, `--no-notify`, `--disabled`.

Routines belong to the scene: `~/.config/narrater/routines/<scene>.json`,
loaded when the scene opens (renames and deletions follow). Recurring
routines don't replay fires missed while the app was closed; a missed
one-shot fires on load. The sidebar's **Routines** panel lists them (fire
now, pause/resume, delete) and the tile shows a clock badge for routines
aimed at it. Creating and editing is Maestro-only, like Maestri. The old
per-tile "scheduled prompt" is now a routine (scenes are migrated on load).

## Workspaces and floors

**Workspaces** are the scenes in the sidebar, each with an optional working
directory (right-click a scene → *Set directory…*, or
`narrater workspace create --dir`), filed under groups and folders. The
layout lives in `~/.config/narrater/workspaces.json` (backend-owned: agents
refile workspaces without racing the canvas auto-save). New terminals start
in the floor's or the workspace's directory. `--from` deep-copies a scene
with fresh ids and paths retargeted onto the new directory.

**Floors** are levels of one scene: every node has a floor (`data.floor`,
absent = ground), the canvas shows one floor at a time (floor bar, top-left),
and **every floor's terminals keep running** — so `ask`/`check` span floors.
When the workspace directory is a git repository, a floor gets a
`git worktree` on its own branch under `~/.local/share/narrater/floors/`
(`src-tauri/src/floors.rs`) — the Linux take on Maestri's copy-on-write
clones. `narrater recruit --floor "F"` staffs a floor from anywhere.

Each level carries a **status line** (`FloorMeta.status`, and
`ground_status` in the scene file) that agents post with
`narrater floor status`; the chip shows its state as a colored dot plus the
progress, the tooltip the text. Moving into `blocked`/`review`/`done` raises
a toast and a desktop notification.

**Landing** (`narrater floor land`, or *Land…* on the chip's menu) keeps the
floor's branch in the repo and removes the worktree; `--into BRANCH` merges
it first — `git merge` when BRANCH is checked out on Ground, otherwise
`merge-tree --write-tree` + `commit-tree` + `update-ref` (or a fast-forward).
Conflicts are found with `merge-tree` *before* anything changes, and
uncommitted work on the floor (untracked included) or on Ground (tracked)
refuses the land. **Deleting** from the chip keeps the branch;
`narrater floor delete` deletes it unless `--keep-branch` (printing the sha
to restore it), and a dirty worktree needs `--force`. Both CLI verbs run only
from a terminal on Ground.

## Flow

```
agent (any vendor) ── shell tool ──► narrater ask "Dev" "…"
  │ JSON {from: $NARRATER_ID, token: $NARRATER_TOKEN, mode: "ask", to, msg}
  ▼ Unix socket /tmp/narrater-<pid>/narrater.sock (dir 0700, socket 0600)
ipc.rs ask_one():
  resolve "Dev" among the caller's connections (undirected)
  ask back? if Dev is itself blocked asking the caller → wake that ask
  enqueue [narrater from Caller]: … → pty.rs monitor types it when Dev is idle
  wait until Dev's turn state settles (screen.rs) → return Dev's screen
```

- **Turn state** (`screen.rs`): every PTY feeds a headless `vt100` emulator.
  A 200 ms monitor hashes the rendered screen; `working` while it changes (or
  shows a busy footer such as `esc to interrupt`, for up to 20 s without
  change), `attention` when a settled screen shows a permission menu
  (`1. Yes`/`1. Allow`… or `(y/n)`), `idle` otherwise. Shells are `working`
  while a foreground job other than the shell runs.
- **Delivery** (`plan_delivery`): bracketed paste + Enter when the program
  enabled DECSET 2004; otherwise text then Enter after 300 ms (newlines
  flattened). Control bytes are stripped from prompts.
- **Canvas commands** (`list`, `note …`, `connect`) are forwarded to the
  frontend (`canvas_request` event → `src/lib/canvasBridge.ts` →
  `canvas_respond`), which owns the graph. An agent reaches only notes wired
  to its terminal, or chained to such a note.
- **Roles** (`roles.rs`): `<project>/.narrater/roles/<key>/{CLAUDE.md,AGENTS.md}`
  (+ `.gemini/settings.json` / `.qwen/settings.json` pointing at AGENTS.md);
  the agent starts there and the files point back to the project.

## Testing the heuristics against a real program

```sh
cd src-tauri
cargo run --example turn_probe -- --shell 'sleep 2; echo done' /bin/bash
cargo run --example turn_probe -- 'Reply with PONG' claude
```

The probe spawns the program in a PTY, waits for idle, delivers the prompt
with the app's delivery plan and prints each turn-state transition plus the
final screen. Run it when adding a new agent preset or after an agent CLI
changes its TUI.

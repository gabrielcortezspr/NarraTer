---
name: narrater-manager
description: Create, swap and dismiss agent terminals on the NarraTer canvas, manage agent roles, send desktop notifications. Use inside NarraTer when the user asks to assemble a team, delegate parallel work, spin up additional agents or terminals to help, swap a teammate's agent (e.g. Claude for Codex), or invoke Maestro mode.
---

# NarraTer team orchestration (Maestro)

You're running inside NarraTer, a spatial canvas that connects AI agents, terminals and notes. The `narrater` CLI is on your PATH (or use `"$NARRATER_CLI"`).

These commands spawn agent terminals on the canvas, assign them roles and wire them together. They need **Maestro mode** on your terminal: if a command answers "this terminal is not the Maestro", the user must turn it on (the crown in your terminal's header). Recruits are auto-connected to you, so once one exists you delegate with `narrater ask "Name" "..."` (from the `narrater` skill) and follow it with `narrater check "Name"`.

## Reuse before recruiting

**Before `narrater recruit`, always run `narrater list`.** If a connected teammate already has a fitting role, delegate to it with `narrater ask` — do NOT spin up a duplicate. Recruits cost the user a real terminal and a real model session. If a teammate's role is close but wrong, prefer `narrater role assign`/`role edit` over recruiting.

Recruit only when `narrater list` shows nobody whose role covers the task, and the work genuinely needs a new persona (a reviewer-only voice next to an implementer, frontend + backend, scout + builder…). Don't recruit for one-off questions.

## Commands

### `narrater recruit "Name" [--preset P] [--role R] [--command C] [--dir PATH]`

Spawns a terminal next to you, names it and connects it to you. Returns once it exists; your first `narrater ask` to it waits until the agent has booted.

- **Name** — invent a short codename (don't pull from a fixed list, don't reuse the role name; it may hint at the recruit's vibe). Vary names across teams.
- **--preset** — `narrater preset list` shows them (Claude Code, Codex, Gemini CLI, OpenCode, Shell, Custom). Omitted: a copy of yourself. Mixing vendors is fine — every agent speaks the same `narrater` protocol.
- **--role** — an existing role (`narrater role list`, or `narrater role create` first). The recruit boots inside `.narrater/roles/<id>/` with the role as its CLAUDE.md / AGENTS.md.
- **--command** — override the program (rarely needed). **--dir** — another working directory (must exist); default: yours.

### `narrater recruit ["New Name"] --preset P --replace "Old"`

Swaps which agent runs on an existing teammate **in place**: the node, its connections (notes, peers) and position survive; only the process restarts. Use this instead of `dismiss` + `recruit` when the user wants a different agent on a teammate. Needs `--preset` (or `--command`); `--role` and `--dir` optional; the name renames it. Chat history does not survive — brief it again.

### `narrater dismiss "Name"`

Stops a connected teammate and removes its terminal. Notes wired only to it become unreachable from the CLI (only the user can rewire them) — to swap its agent, use `--replace`.

### `narrater connect "From" "To"`

Wires two things you can reach: yourself, a teammate, or a note you or a teammate is wired to. Connecting two recruits isn't enough on its own — bake the collaboration into their roles: name the peer and tell each one when to `narrater ask "Peer" "..."` (e.g. "When you finish a draft, run `narrater ask \"Reviewer\" \"please review\"` and apply the feedback"). For a shared note, name it in the role ("Read `design-spec` first, log progress in `scratch-pad`").

### Roles

- `narrater role list` / `narrater role show "Name"`
- `narrater role create "Name" "Prompt"` — the prompt is what the recruit sees as `<your_assigned_role>`. Be specific: name + prompt should leave no doubt about the scope. Always include "Run `narrater list` to see your teammates and shared notes before delegating or asking questions."
- `narrater role write "Name" "Prompt"` / `narrater role edit "Name" "old" "new"`
- `narrater role assign "Recruit" "Role"` (or `--none`) — retargets a teammate without dismissing it; the process restarts with the new role, topology kept.
- `narrater role delete "Name" [--force]` — `--force` also unassigns it from terminals using it.

### `narrater notify "Message for the user"`

Desktop notification. Only when the user asked to be notified; otherwise assume they're watching the canvas.

## Workflow

1. **Take stock** — `narrater list`. A Maestro also sees each teammate's own connections nested under it. Reuse whoever fits.
2. **Plan the gaps** — which roles are genuinely missing.
3. **Define roles** — `narrater role list`, `narrater role create` for the gaps.
4. **Recruit** each missing teammate with a distinctive codename and `--role`.
5. **Optional** — `narrater connect` recruits that must collaborate without you in the middle.
6. **Delegate** — `narrater ask "Name" "task…"`, or several at once with `narrater ask --batch '{"A": "task…", "B": "task…"}'` so independent work runs in parallel.

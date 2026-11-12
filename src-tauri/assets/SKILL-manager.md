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

## Is a team worth it?

Usually not. In NarraTer's own benchmark, one Opus terminal did two coding tasks faster than any team (1.6 vs
3.9 min, and 1.8 vs 2.8 min on four independent modules), with the same score; coordination costs about a
minute of the lead's time per task, and the slowest teammate sets the pace. Build a team only when:
- the task has 2+ independent parts, in separate files, each worth several minutes of work;
- an independent reviewer or tester with a clean context pays off (a cheaper model writes, or the change is risky);
- the work doesn't fit in one context.

Parallel writers get the same model. A reviewer pays when the writer can err: a cheaper writer with a strong reviewer, or another vendor reviewing.

## Generate a whole team at once

`narrater team create --template T --task "..." --reason "..." [--check "CMD"] [--model M]` builds the team
in one step: terminals (agent, model, role), wiring and two notes (the task with its acceptance check, for
everyone; why this team, for you). `narrater team templates` lists the shapes — solo, dev-reviewer, dev-qa,
readers, parallel-devs — with when each pays off; `--spec` takes a custom shape as JSON. Prefer it to
recruiting one by one. Then delegate with `narrater ask "Dev" "..." --check "CMD"`: the check, not the
agent's "done", says the work is finished.

## Commands

### `narrater recruit "Name" [--preset P] [--model M] [--role R] [--floor F] [--workspace W] [--command C] [--dir PATH]`

Spawns a terminal next to you, names it and connects it to you. Returns once it exists; your first `narrater ask` to it waits until the agent has booted.

- **Name** — invent a short codename (don't pull from a fixed list, don't reuse the role name; it may hint at the recruit's vibe). Vary names across teams.
- **--model** — the model the recruit runs (its CLI's `--model`: sonnet, opus, gpt-5-codex…). Omitted: the CLI's default (or yours, when it copies you).
- **--preset** — `narrater preset list` shows them (Claude Code, Codex, Gemini CLI, OpenCode, Shell, Custom). Omitted: a copy of yourself. Mixing vendors is fine — every agent speaks the same `narrater` protocol.
- **--role** — an existing role (`narrater role list`, or `narrater role create` first). The recruit boots inside `.narrater/roles/<id>/` with the role as its CLAUDE.md / AGENTS.md.
- **--floor** — place the recruit on another floor of this workspace (`narrater floor list`; `"Ground"` for the ground level). Without `--dir`, a git-isolated floor's recruit works in that floor's own checkout. The connection spans floors: `ask`, `check` and `dismiss` work the same.
- **--workspace** — place the recruit in another workspace (its ground level, or `--floor` there; `narrater workspace list` shows them). The recruit starts in that workspace's directory and runs where that workspace runs, since it is that project's agent — the workspace starts running if it wasn't open. You address it as `Name @ Workspace`, exactly as `narrater list` prints it; `ask`, `check` and `dismiss` take that address.
- **--command** — override the program (rarely needed). **--dir** — another working directory (must exist); default: yours (or the floor's / workspace's when you pass those).

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
6. **Delegate** — `narrater ask "Name" "task…"`, or several at once with `narrater ask --batch '{"A": "task…", "B": "task…"}' --no-wait` so independent work runs in parallel; then `narrater wait` once per teammate, handling each result as it arrives (review it, give follow-up work, or dispatch the next task) instead of blocking until the slowest is done.

---
name: narrater-workspace
description: Create NarraTer workspaces and floors from the command line, optionally cloning an existing workspace as a starting point; file workspaces into sidebar groups and folders. Use inside NarraTer when the user asks to provision, spin up or scaffold a new workspace or project environment, or an isolated floor (a git worktree on its own branch) for risky or experimental work.
---

# NarraTer workspace provisioning

You're running inside NarraTer, a spatial canvas that connects AI agents, terminals, notes and browser portals. The `narrater` CLI is on your PATH (or use `"$NARRATER_CLI"`).

**Workspaces** are standalone canvases, each rooted in its own project directory — isolation *between* projects. **Floors** are levels inside one workspace, optionally backed by a git worktree of its repository on its own branch — isolation *within* a project. These commands provision either without leaving the terminal.

They need **Maestro mode** on your terminal: if one answers "this terminal is not the Maestro", the user must turn it on (the crown in your terminal's header).

## Scope & safety

- Creation and filing are additive and reversible; renaming only changes a label. There is deliberately **no delete command** — anything destructive (deleting a workspace or a floor, landing a floor's branch) stays in the UI, on explicit user action.
- Creation is silent: nothing changes on the user's screen. A new workspace appears in the sidebar; its terminals start when the user opens it.
- The directory must already exist — create it (and `git init` if wanted) before `workspace create`.

## Commands

### `narrater workspace create "Name" --dir PATH [--from "Source"] [--group "Group"] [--folder "Folder"]`

- `--from "Source"` — clone an existing workspace: its full layout (terminals, notes, file trees, floors, connections) is deep-copied and every path under the source's directory is retargeted onto PATH. Terminals start fresh (no scrollback, nothing running); floors come over as plain floors.
- `--group "Group"` — file it under a sidebar group (created if missing).
- `--folder "Folder"` — file it in a folder. Alone, the folder is matched anywhere (created at the root when missing); with `--group`, inside that group; with `--root`, only root folders. An ambiguous folder name asks for `--group` or `--root`.

The user's "template" is just a normal workspace — suggest keeping one laid out the way they like and cloning it by name:

```
narrater workspace create "Acme Churn" --dir ~/Work/acme-churn --from "Data Project Template" --group "Data Projects"
narrater workspace create "Q3 Report" --dir ~/Work/q3 --group "Data Projects" --folder "Reports"
```

### `narrater workspace move "Name" --group "G" | --folder "F" | --root`
Refiles a workspace in the sidebar (same rules as `create`; `--root` alone moves it out of any group or folder). The workspace itself is untouched.

### `narrater workspace rename "Current Name" "New Name"`
Only the label changes — canvas, floors, routines and directory stay.

### `narrater workspace list`
The sidebar hierarchy — root workspaces, folders, groups — with each workspace's directory and floor count, marking yours. Run it before `create` or `move` to pick a free name, the exact `--from` source and where to file it.

### `narrater floor create "Name" [--branch NAME] [--existing-branch] [--no-git] [--copy-ground]`
Adds a floor to **your own workspace**. When the workspace directory is a git repository, the floor gets its own **git worktree** on its own branch (default: a slug of the floor name), outside the project, so work there can't disturb the ground floor. Otherwise a plain floor is created and the answer says why.

- `--branch NAME` — branch for the worktree (fails if it already exists).
- `--existing-branch` — check out an existing branch instead of creating one.
- `--no-git` — skip isolation and share the ground directory.
- `--copy-ground` — start the floor with a copy of the ground floor's layout.

```
narrater floor create "Refactor Auth" --branch refactor-auth
narrater floor create "Scratch" --no-git
```

Landing a floor's work (merging its branch) is done by the user — there's no CLI command for it.

### `narrater floor list`
Ground and every floor, with branch, worktree path and node count.

## Isolated-experiment flow

When the user wants risky or experimental work kept out of the main checkout:

1. `narrater floor create "Experiment" --branch feat/idea`
2. `narrater recruit "Name" --floor "Experiment" --role "..."` (from the narrater-manager skill) — the recruit works in the floor's worktree, on its own branch, while you and the user stay on the ground floor. The connection spans floors: `narrater ask` and `check` work the same.

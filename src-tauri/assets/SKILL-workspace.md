---
name: narrater-workspace
description: Create NarraTer workspaces and floors from the command line, optionally cloning an existing workspace as a starting point; file workspaces into sidebar groups and folders; land or delete a floor when its work is done; post a floor's status. Use inside NarraTer when the user asks to provision, spin up or scaffold a new workspace or project environment, or an isolated floor (a git worktree on its own branch) for risky or experimental work, or to land, merge back or remove a floor — and whenever you are working on a task on a floor, to keep its status current.
---

# NarraTer workspace provisioning

You're running inside NarraTer, a spatial canvas that connects AI agents, terminals, notes and browser portals. The `narrater` CLI is on your PATH (or use `"$NARRATER_CLI"`).

**Workspaces** are standalone canvases, each rooted in its own project directory — isolation *between* projects. **Floors** are levels inside one workspace, optionally backed by a git worktree of its repository on its own branch — isolation *within* a project. These commands provision either without leaving the terminal.

They need **Maestro mode** on your terminal, except `narrater floor status`, which any agent uses for the floor it is on. If one answers "this terminal is not the Maestro", the user must turn it on (the crown in your terminal's header).

## Floor status (any agent)

When you work on a task on a floor, keep that floor's status current with `narrater floor status`. It is the line the user glances at on the floor's chip to see how things stand, so post when you start, at a real milestone, when you need the user (`--state blocked`), and when the work is ready (`--state review`). Moving to `blocked`, `review` or `done` also notifies the user, so use those states only when they are true. One short line in plain words; don't post every step.

## Scope & safety

- Creation and filing are additive and reversible; renaming only changes a label. Workspaces have deliberately **no delete command** — removing one stays in the UI, on explicit user action.
- Floors are the exception: `narrater floor land` and `narrater floor delete` change the user's project (a merge into their branch, a deleted worktree and branch). **Run them only when the user asks, or after confirming with them** — a floor reporting `review` is ready for the user to decide, not a signal to land it yourself. Both run **from a terminal on the ground level** only.
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

### `narrater floor create "Name" [--branch NAME] [--existing-branch] [--copy] [--no-git] [--copy-ground]`
Adds a floor to **your own workspace**. When the workspace directory is a git repository, the floor gets its own **git worktree** on its own branch (default: a slug of the floor name), outside the project, so work there can't disturb the ground floor. The worktree shares the project's history: its branch and commits are in the project from the start. Otherwise a plain floor is created and the answer says why.

- `--branch NAME` — branch for the worktree (fails if it already exists).
- `--existing-branch` — check out an existing branch instead of creating one. A worktree can't take the branch Ground has checked out; add `--copy` for that.
- `--copy` — instead of a worktree, a fully isolated copy of the repository, exactly as it is right now (only where the disk clones instantly — btrfs, XFS; otherwise it's refused and says so). Because it's separate, it can check out the same branch as Ground, and landing the floor transfers its work back. Use it for that, or when the user asks for full isolation; a worktree is the right choice for most work.
- `--no-git` — skip isolation and share the ground directory.
- `--copy-ground` — start the floor with a copy of the ground floor's layout.

```
narrater floor create "Refactor Auth" --branch refactor-auth
narrater floor create "Scratch" --no-git
```

### `narrater floor create ["Name"] --pull-request NUMBER|URL [--copy] [--copy-ground]`
Adds a floor to review an open pull request of the project: `42`, `#42` or its link. NarraTer asks the project's host about it (its CLI — `gh` or `glab` — signed in), brings its branch into the project and checks it out on the floor. The name defaults to `#42` and the title. When the host confirms the pull request's branch is in the project's own repository, the floor has that branch and commits pushed from it go to the pull request. Otherwise (a fork), the floor has a copy of its commits named `pr-42`: commits made there can't reach it. The floor's menu in the floor bar shows the pull request's review and checks, and can merge it. It's refused, with the reason, when the pull request isn't open, is already on a floor or being made into one, changed while it was fetched, the host has no CLI that can list pull requests, or your local branch of that name went its own way or pushes somewhere else. `--branch`, `--existing-branch` and `--no-git` don't apply.

```
narrater floor create --pull-request 42
narrater floor create "Review Login Fix" --pull-request https://github.com/acme/app/pull/42
```

### `narrater floor land "Name" [--into BRANCH] [--delete-branch] [--keep-floor]`
Brings the floor's work into the project, like the Land action on the floor's chip. **Only when the user asks, or after confirming with them; only from Ground.** A worktree's branch already lives in the repository, and a copy's branch is fetched back into the project's branch of the same name; landing then removes the floor and keeps the branch, ready to review or merge.

- `--into BRANCH` — also merge the floor's branch into BRANCH (e.g. `main`). If BRANCH is checked out on Ground the merge happens there; if it's checked out nowhere, the branch is moved directly.
- `--delete-branch` — after a merge with `--into`, delete the floor's branch.
- `--keep-floor` — leave the floor in place after landing.

A land is refused, with nothing changed, when there is uncommitted work on the floor or on the ground, or when the merge would conflict (the files are named). Ask the floor's agent to commit first, or to merge BRANCH into its own branch and resolve the conflict there, then land again.

```
narrater floor land "Refactor Auth"
narrater floor land "Refactor Auth" --into main --delete-branch
```

### `narrater floor delete "Name" [--keep-branch] [--force]`
Removes the floor — its terminals, notes and other nodes — and its worktree; the branch is deleted too unless `--keep-branch` (the answer prints the commit to bring it back). **Only when the user asks, or after confirming with them; only from Ground.** A worktree with uncommitted work is refused unless `--force`, which discards that work.

### `narrater floor list`
Ground and every floor, with branch, worktree path, node count and status line.

### `narrater floor status ["Text"] [--state working|blocked|review|done] [--progress 2/5] [--floor "Name"] [--clear]`
The one line the user sees on a floor's chip. Any terminal posts it for the floor it is on; only the Maestro can name another floor with `--floor` (`"Ground"` for the ground level, where a summary of the floors above belongs). `--state` defaults to `working`; moving into `blocked`, `review` or `done` notifies the user, new text or progress in the same state doesn't. `--progress` takes `2/5`, `40%` or `0.4`. Without text it prints the current status; `--clear` removes it. Lines are cut at 120 characters.

```
narrater floor status "Tests green, wiring the login screen" --progress 3/5
narrater floor status "Need a decision on the token format" --state blocked
narrater floor status "Branch committed, ready to land" --state review
narrater floor status "2 of 3 floors in review" --floor "Ground"
```

## Isolated-experiment flow

When the user wants risky or experimental work kept out of the main checkout:

1. `narrater floor create "Experiment" --branch feat/idea`
2. `narrater recruit "Name" --floor "Experiment" --role "..."` (from the narrater-manager skill) — the recruit works in the floor's worktree, on its own branch, while you and the user stay on the ground floor. The connection spans floors: `narrater ask` and `check` work the same. Tell the recruit to keep `narrater floor status` current and to commit its work when done.
3. When the floor reports `review`, tell the user; land it (`narrater floor land "Experiment" --into main`) only when they say so.

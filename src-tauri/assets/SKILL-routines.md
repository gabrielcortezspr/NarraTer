---
name: narrater-routines
description: Manage scheduled routines on the NarraTer canvas (recurring commands, recurring prompts for agents, reminders). Use inside NarraTer to schedule a command or a recurring prompt for an agent, automate a timed task, set a reminder, or list/edit/remove routines.
---

# NarraTer routines

You're running inside NarraTer, a spatial canvas that connects AI agents, terminals, notes and browser portals. The `narrater` CLI is on your PATH (or use `"$NARRATER_CLI"`).

Routines are scheduled commands fired into a terminal of the current scene — a shell command, or a prompt for the agent running there — or desktop reminders with no terminal. Delivery waits until the terminal is idle, exactly like `narrater ask`.

These commands need **Maestro mode** on your terminal: if one answers "this terminal is not the Maestro", the user must turn it on (the crown in your terminal's header). You see and manage the routines of the current scene only.

## Scope & safety

- **`delete` is destructive.** Only delete a routine when the user explicitly asks — never on your own initiative, not even to tidy up.
- Always run `narrater routine list` first to get exact names and short ids.

## Commands

### `narrater routine list`
Every routine of the scene: short id, schedule, target, status, next fire, fire count.

### `narrater routine show "Name"`
Full detail, including the command, the pre-run script and the last result.

### `narrater routine create "Name" --command "..." <schedule> [options]`
A name, a `--command` and exactly one schedule are required. `--command` decodes `\n` as a newline.

**Schedule (exactly one):**
- `--every 30m` — interval: `45s`, `30m`, `2h`, `1h30m` (a bare number means minutes)
- `--daily 09:00` — every day at a 24-hour time
- `--weekly mon,wed,fri@09:00` — chosen weekdays at a time (mon … sun)
- `--once "2026-06-20 15:00"` — a single time (`YYYY-MM-DD HH:MM`, local time)

**Target (optional):**
- *(omit)* — fires into **your own terminal**: a recurring task for yourself.
- `--terminal "Name"` — another terminal of the scene (`narrater list` shows names).
- `--reminder` — no terminal: a desktop notification with the command text.

**Options:**
- `--count N` — stop after N fires. `--until 2026-07-01` — stop after that day. (Default: forever.)
- `--pre-run "shell script"` — runs before each fire (in the target terminal's project directory) and must exit 0, or the fire is skipped. Put the literal `{{output}}` in `--command` where its stdout should go — **without `{{output}}` the script still runs but its output isn't inserted.** Env: `$NARRATER_SCENE_NAME`, `$NARRATER_SCENE_DIR`, `$NARRATER_ROUTINE_NAME`, `$NARRATER_TERMINAL_NAME`.
- `--no-skip-if-busy` — queue the fire even if the terminal is mid-task (by default a fire is skipped while it's busy).
- `--no-notify` — no toast on the canvas when it fires.
- `--disabled` — create it paused.

```
narrater routine create "Daily digest" --command "Summarize today's commits:\n\n{{output}}" --daily 18:00 --pre-run "git log --since=midnight --oneline"
narrater routine create "Nightly audit" --command "Run npm audit and summarize new CVEs" --daily 02:00 --terminal "Backend"
narrater routine create "Hourly self-check" --command "Re-read the plan and report progress" --every 1h
narrater routine create "Standup" --command "Time for standup" --weekly mon,tue,wed,thu,fri@09:00 --reminder
```

### `narrater routine edit "Name" [--name "New"] [--command ...] [schedule] [options]`
Changes only the fields you pass. `--name` renames. A new schedule resets the fire counters. `--skip-if-busy` / `--notify` turn those back on.

### `narrater routine enable "Name"` / `narrater routine disable "Name"`
Resume or pause without editing.

### `narrater routine run "Name"`
Fires once now, regardless of the schedule — handy to test one. If the target terminal isn't running, the fire is skipped.

### `narrater routine delete "Name"`
Removes a routine. **Destructive — only on an explicit user request.**

## Duplicate names

If two routines share a name, commands list the matches with short ids. Re-run with `--id <prefix>`:

```
narrater routine edit "Audit" --id a1b2c3 --daily 03:00
```

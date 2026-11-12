---
name: narrater
description: Send prompts to connected AI agents on the NarraTer canvas and get their answers; read and write connected sticky notes. Use when you run inside NarraTer (NARRATER_ID is set) and the intent is to collaborate with another agent — 'ask [name] to…', 'tell [name] to…', 'check on [name]', 'delegate', or 'create/update a note' — or when a prompt arrives as `[narrater from X]: …`.
---

# NarraTer inter-agent communication

You're running inside NarraTer, a spatial canvas that connects AI agents, terminals and notes. The `narrater` CLI is on your PATH; if it's not found, use `"$NARRATER_CLI"`.

Agents connected to you on the canvas can exchange prompts and answers through the CLI, whatever AI runs in them (Claude, Codex, Gemini, OpenCode, a shell…). Notes connected to you can be read and written.

## Commands

- `narrater list` — who you are (`You:`), connected agents (status, role) and notes
- `narrater ask "Agent" "your prompt"` — send a prompt and get the answer when the agent finishes
- `narrater ask --batch '{"Agent A": "prompt", "Agent B": "prompt"}'` — ask several agents in parallel
- `narrater ask "Agent" "prompt" --no-wait` (or `--batch … --no-wait`) — send without blocking; prints an ask id
- `narrater ask "Agent" "prompt" --check "CMD"` — the ask only counts as done if `CMD` passes (exit 0) in the agent's project after its turn; otherwise it ends `failed` with the command's output
- `narrater wait [ask-id…] [--all]` — the first of your asks to finish, with its answer; `narrater asks` lists yours
- `narrater ask "Agent" --raw "2\n"` — type raw input (menus, permission prompts, shell input). Escapes: `\n` Enter, `\t` Tab, `\e` ESC, `\xNN` byte (`\x03` Ctrl-C); special keys are ESC sequences, e.g. `\e[A` up arrow
- `narrater check "Agent"` — read the agent's current screen without sending anything
- `narrater note create ["content"] [--name "Name"]` — new note next to your terminal, connected to you; prints the exact name
- `narrater note read "Note" [offset] [limit]` — read with line numbers
- `narrater note write "Note" "content"` — replace the whole note
- `narrater note edit "Note" "old text" "new text"` — replace a substring
- `narrater note delete "Note"` — **destructive; only when the user explicitly asks**
- `narrater connect "From" "To"` — wire two things you can reach (agents or notes)

Always run `narrater list` first to get exact names. Run `narrater help <command>` for details; never guess a flag. If a command fails with a connection error, run `narrater debug` before anything else.

## How `ask` works

`ask` types your prompt into the agent's terminal as `[narrater from <your name>]: <prompt>` once it is idle, and blocks until its turn is over; then it prints what the agent's terminal shows. Scale your shell tool timeout to the work: about 1 minute for an easy question, up to 10 minutes for a real task.

- If your timeout expires, do NOT re-send the prompt: the ask keeps running. `narrater asks` shows its id; `narrater wait <id>` picks up the answer (and `narrater check "Agent"` shows progress meanwhile).
- **Don't block when you have other work.** `--no-wait` returns an ask id at once. To delegate to several agents, send them all with `narrater ask --batch '{…}' --no-wait`, then call `narrater wait` repeatedly: each call returns the next one to finish (`[id] Agent · state` + its answer), so you can react to the first result without waiting for the slowest. When `wait` says nothing finished in time, just wait again.
- Never interrupt an agent that is still working, and don't edit files another agent is actively modifying.
- If `ask` reports the agent is waiting for input (a permission menu), answer with `--raw` or tell the user.
- **Ask back** — for results longer than one screen (reviews, plans, reports): tell the agent to report with `narrater ask "Your Name" "<result>"` when done. That message resolves your waiting `ask` and arrives in full in your terminal as a new prompt.

## Delegating well

An agent's "done" is its own word; check it. When the task has a verifiable criterion (tests, a build, a
linter), pass it as `--check "CMD"`: NarraTer runs it after the turn, without the agent's help, and a
failure comes back as `failed` with the output — send that back to the agent instead of trusting it.

Write each delegation as a small spec, not a chat line:
- **Goal:** what must be true at the end.
- **Acceptance:** the command or the observable result that proves it (the same as `--check`).
- **Scope:** the files or modules it may change; two agents never write the same files.
- **Answer:** what to report back (e.g. "files changed and test result, in 5 lines").

For a long spec, write it in a note, connect the note to the agent and point to it in the prompt.

Don't repeat a prompt you already sent: if the answer looked incomplete, ask a precise follow-up about what
is missing. If NarraTer warns that a prompt repeats an earlier one, read that earlier answer first.

## Teams: one agent is the default

If `narrater list` shows `Automatic teams on` next to your name (or you are the Maestro), you may build a
whole team for a task with `narrater team create` (see `narrater help team`). Measured in NarraTer, one
agent did two coding tasks faster than any team with the same result, so work solo unless one of these holds:
- the task has 2+ independent parts, in separate files, each worth several minutes of work;
- an independent reviewer or tester with a clean context pays off (a cheaper model writes, or the change is risky);
- the work doesn't fit in one context (a wide investigation across many areas).

When one holds, pick the closest shape from `narrater team templates` and pass the reason with `--reason`.
Who decides: as the Maestro, build a team when the user asks for one; with Automatic teams on, you may also
decide on your own, by the rule above. Otherwise never build a team on your own.

## When a prompt arrives from another agent

A prompt starting with `[narrater from X]:` comes from agent X, which is waiting for you to finish. Do the task; your final screen is X's answer, so end your turn with a clear, concise result. For long results, or questions about the task, message X with `narrater ask "X" "..."` — doubts about a delegated task go to X, never to the user.

## Notes

Notes support markdown and update live on the canvas. A note's name comes from its first line unless it was created with `--name` (then it never changes) — prefer `--name` for notes you'll address again; `write`/`edit` tell you when a rename happens. When a note has content, prefer `edit` over `write`. Notes chained to a connected note are reachable too (`narrater list` shows them indented).

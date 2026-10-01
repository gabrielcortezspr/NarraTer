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

- If your timeout expires, do NOT re-send the prompt. Run `narrater check "Agent"` to see the progress, then wait again.
- Never interrupt an agent that is still working, and don't edit files another agent is actively modifying.
- If `ask` reports the agent is waiting for input (a permission menu), answer with `--raw` or tell the user.
- **Ask back** — for results longer than one screen (reviews, plans, reports): tell the agent to report with `narrater ask "Your Name" "<result>"` when done. That message resolves your waiting `ask` and arrives in full in your terminal as a new prompt.

## When a prompt arrives from another agent

A prompt starting with `[narrater from X]:` comes from agent X, which is waiting for you to finish. Do the task; your final screen is X's answer, so end your turn with a clear, concise result. For long results, or questions about the task, message X with `narrater ask "X" "..."` — doubts about a delegated task go to X, never to the user.

## Notes

Notes support markdown and update live on the canvas. A note's name comes from its first line unless it was created with `--name` (then it never changes) — prefer `--name` for notes you'll address again; `write`/`edit` tell you when a rename happens. When a note has content, prefer `edit` over `write`. Notes chained to a connected note are reachable too (`narrater list` shows them indented).

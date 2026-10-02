---
name: narrater-chat
description: Applies only to messages that open with a NarraTer Chat header. You MUST read this skill before replying to those messages.
---

You're running inside NarraTer, a spatial canvas that connects AI agents, terminals, notes and portals. The `narrater` CLI is on your PATH; if it's not found, use `"$NARRATER_CLI"`.

A message opening with this header comes from the user through NarraTer Chat:

    ---
    Below is a message from the user through NarraTer Chat.
    Reply only via `narrater say`. If the narrater-chat skill isn't already loaded, load it first.
    thread: blue
    ---

Everything after the closing fence is the user's own words (attachments, if any, are listed at the end with their paths). Reply only via `narrater say` — terminal text does not reach the user.

Adapt dynamically: messages with this header come from NarraTer Chat, so reply via `narrater say`; messages without it are sent to you directly (typed in your terminal, or `[narrater from X]` from another agent), so reply directly.

## Answer

Run with your shell tool:

    narrater say <<'EOF'
    your answer
    EOF
    narrater say 'short answer'
    narrater say --file <path>
    narrater say --attach <path> 'your answer'

The heredoc takes any text as-is. Quoting an argument breaks on apostrophes, quotes, `$` and backticks.

`--file` sends a UTF-8 text file as the message, or attaches an image (PNG, JPEG, GIF, HEIC or WebP) by absolute path. To present an image with prose, put `![Description](/absolute/path/image.png)` on its own line. To share other files (PDF, zip…), add `--attach <path>` (repeatable, final reply only).

## Long tasks

    narrater say --progress 'text'   # interim update; your turn continues

## Threads

The user keeps seven threads with you, named by color: blue, purple, pink, red, orange, yellow, green. The header's `thread` color identifies this message's thread. On the first message, or when that color changes, run `narrater recall <color>` before answering. Earlier terminal context may belong to another thread; treat it as unrelated unless asked.

## Recall

    narrater recall [thread] [--turns N | --last day|week|month | --from YYYY-MM-DD [--to YYYY-MM-DD] | --all] [--json]
    narrater recall list

Default: the current thread's last 10 turns. N must be between 1 and 500. Dates use the local time zone, and `--to` includes that whole day. On a fresh process, a restart or a compaction, this prints what you and the user already said. Attachments have stable ids; `narrater recall attachment <id>` prints a path you can open.

## Rules

- Every `narrater say` is a bubble the user sees, and the last one closes your turn. Send as many as the answer needs.
- After sending, do not repeat the answer in the terminal or announce delivery.
- `--progress` is for short status updates (e.g. 'Running tests...'), never for the answer itself.

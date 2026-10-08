//! Help texts of the `narrater` CLI (`narrater help [command]`).

/// `narrater help` with no topic.
pub const MAIN: &str = r#"NarraTer — a spatial canvas for AI agents and terminals.

The narrater CLI lets agents talk to each other and share sticky notes on the
canvas. Only agents and notes connected to you on the canvas are reachable.

Usage:
  narrater list                                    List connected agents and notes
  narrater ask "Agent" "prompt"                    Send a prompt to a connected agent; returns its answer when it finishes
  narrater ask --batch '{"A": "prompt", "B": "prompt"}'
                                                   Ask several agents in parallel; JSON array once all finish
  narrater ask "Agent" --raw "2\n"                 Type raw input into the agent's terminal (menus, keys, shell input).
                                                   Escapes: \n Enter, \t Tab, \e ESC, \xNN byte (\x03 = Ctrl-C), e.g. \e[A up arrow
  narrater check "Agent"                           Read what the agent's terminal shows right now
  narrater note create ["content"] [--name "Name"]  Create a note on the canvas, linked to you; prints its name
  narrater note read "Note" [offset] [limit]       Read a note with line numbers (optional line range)
  narrater note write "Note" "content"             Replace a note's content entirely
  narrater note edit "Note" "old text" "new text"  Replace a substring within a note
  narrater note delete "Note"                      Remove a connected note (only on explicit user request)
  narrater connect "From" "To"                     Wire two things you can reach (agents or notes)
  narrater say "text" | <<'EOF' | --file PATH | --attach PATH | --progress "text"
                                                   (NarraTer Chat only) Reply to the user's chat message
  narrater recall [thread] [--turns N | --last day|week|month | --from DATE [--to DATE] | --all] [--json]
                                                   (NarraTer Chat only) What a chat thread remembers
  narrater recall list | recall attachment ID      (NarraTer Chat only) The seven threads; fetch an attachment
  narrater portal create URL ["Name"] [--size WxH]  Open a browser portal next to you, connected to you
  narrater portal snapshot "Portal"                Elements with refs (@e1, @e2…) to use as selectors
  narrater portal click|hover|focus "Portal" @e3   Act on a ref, CSS selector or x,y
  narrater portal fill "Portal" @e2 "text"         Clear a field and type into it
  narrater portal type "Portal" [@e2] "text"       Type into the focused (or given) element
  narrater portal key "Portal" "Enter"             Press a key (ctrl+a, shift+tab…)
  narrater portal navigate "Portal" "url"          Go to a URL (also: info, screenshot, text, html, evaluate…)
  narrater portal devices                          List Android devices, emulators and AVDs
  narrater portal create --simulator ID ["Name"]   Open a portal on an Android device or AVD (boots it)
                                                   Run `narrater help portal` for every portal command
  narrater recruit "Name" [--preset P] [--role R] [--floor F] [--workspace W] [--command C] [--dir PATH]
                                                   (Maestro only) Spawn a teammate, auto-connected to you
  narrater recruit ["New Name"] --preset P --replace "Old"
                                                   (Maestro only) Swap which agent runs on a teammate, in place
  narrater dismiss "Name"                          (Maestro only) Stop and remove a connected teammate
  narrater preset list                             (Maestro only) List agent presets for --preset
  narrater role list | show "Name"                 (Maestro only) List roles / print a role's full prompt
  narrater role create "Name" "Prompt"             (Maestro only) Add a role preset
  narrater role write "Name" "Prompt"              (Maestro only) Replace a role's prompt
  narrater role edit "Name" "old" "new"            (Maestro only) Replace a substring within a role's prompt
  narrater role assign "Recruit" "Role" | --none   (Maestro only) Reassign a teammate's role (restarts it)
  narrater role delete "Name" [--force]            (Maestro only) Delete a role; --force unassigns it first
  narrater notify "message"                        (Maestro only) Send the user a desktop notification
  narrater workspace create "Name" --dir PATH [--from "Source"] [--group G] [--folder F]
                                                   (Maestro only) New workspace rooted at PATH (optionally cloning another)
  narrater workspace list | rename "Old" "New" | move "Name" --group G | --folder F | --root
                                                   (Maestro only) The sidebar hierarchy; rename; refile
  narrater floor create "Name" [--branch B] [--existing-branch] [--copy] [--no-git] [--copy-ground]
                                                   (Maestro only) New floor of this workspace — a git worktree when possible
  narrater floor create ["Name"] --pull-request NUMBER|URL [--copy] [--copy-ground]
                                                   (Maestro only) New floor reviewing an open pull request
  narrater floor list                              (Maestro only) Ground + floors with branch, path, node count, status
  narrater floor status ["Text"] [--state working|blocked|review|done] [--progress 2/5] [--clear]
                                                   Post (or print) your floor's status line; --floor "F" is Maestro only
  narrater floor land "Name" [--into BRANCH] [--delete-branch] [--keep-floor]
                                                   (Maestro only, from Ground, only when the user asks) Land a floor's branch
  narrater floor delete "Name" [--keep-branch] [--force]
                                                   (Maestro only, from Ground, only when the user asks) Remove a floor
  narrater routine list | show "Name"              (Maestro only) Scheduled routines of this scene
  narrater routine create "Name" --command "..." <schedule> [options]
                                                   (Maestro only) Schedule a prompt/command (default target: you)
  narrater routine edit "Name" [flags]             (Maestro only) Change only the fields you pass
  narrater routine enable|disable|run|delete "Name" (Maestro only) Resume, pause, fire now, remove
                                                   Schedules: --every 30m | --daily 09:00 | --weekly mon,fri@09:00 | --once "2026-06-20 15:00"
                                                   Options: --terminal "Name" | --reminder | --count N | --until DATE | --pre-run "..." | --no-notify | --disabled
  narrater debug                                   Diagnose connection issues
  narrater help [command]                          Show this help, or one command in detail

Prompt arguments decode \n to a newline and \t to a tab; double the
backslash to send them literally (\\n delivers \n)."#;

/// `narrater help <command>`.
pub fn topic(name: &str) -> Option<&'static str> {
    Some(match name {
        "ask" => r#"Usage:
  narrater ask "Agent" "prompt" [--timeout SECONDS]
  narrater ask --batch '{"A": "prompt", "B": "prompt"}'
  narrater ask "Agent" --raw "2\n"

`ask` types your prompt into the agent's terminal when it is idle and blocks
until its turn is over, then prints what its terminal shows. Scale your shell
tool timeout to the work: about 1 minute for something easy, up to 10 for a
real task. If your timeout expires, do NOT re-send the prompt: run
`narrater check "Agent"` to see the progress, then wait again.

The ask back pattern: tell the agent to report with
`narrater ask "Your Name" "<result>"` when it finishes (your name is under
`You:` in `narrater list`). Its message resolves your waiting `ask` and
arrives in your terminal in full as a new prompt, instead of being clipped to
one screen.

--batch takes a JSON map of agent name to prompt, runs every target in
parallel and returns a JSON array of {name, output} (or {name, error}) once
all of them are done. Batch prompts use plain JSON string escaping.

--raw types bytes straight into the terminal, for interactive menus and
prompts that `ask` cannot drive, and prints the resulting screen.
Escapes: \n Enter, \t Tab, \e ESC, \xNN byte (\x03 = Ctrl-C). Special keys
are ESC sequences, e.g. \e[A up arrow, \e[Z Shift-Tab."#,
        "check" => r#"Usage:
  narrater check "Agent"

Prints what the agent's terminal is showing right now without sending
anything. Use it to see whether earlier work finished, or to follow a long
task while it runs. Don't mistake unsent text in its input box for
instructions to you."#,
        "note" => r#"Usage:
  narrater note create ["content"] [--name "Name"]
  narrater note read "Note" [offset] [limit]
  narrater note write "Note" "content"
  narrater note edit "Note" "old text" "new text"
  narrater note delete "Note"

Notes connected to your terminal (or chained to one that is) can be read and
written. `create` puts a new note next to your terminal and connects it; the
response prints the exact name. Without --name, a note is named after its first
line and may be renamed when that line changes — prefer --name for notes you
will address again. Prefer `edit` over `write` when a note already has content.
`delete` is destructive: only run it when the user explicitly asks."#,
        "list" => r#"Usage:
  narrater list

Shows who you are (You:), the agents connected to you with their status and
role, and the notes you can read and write. Always run it first to get exact
names."#,
        "connect" => r#"Usage:
  narrater connect "From" "To"

Wires two things you can reach (yourself, a connected agent, or a connected
note). Agent ↔ agent lets both sides `ask` each other; note ↔ agent shares the
note. Tell the agents about the new connection — they are not notified."#,
        "say" => r#"Usage:
  narrater say <<'EOF'
  your answer
  EOF
  narrater say 'short answer'
  narrater say --file /abs/path.md|.png      UTF-8 text becomes the message; an image is attached
  narrater say --attach /abs/file.pdf 'text' attach files (repeatable, final reply only)
  narrater say --progress 'Running tests...' interim update; your turn continues

(NarraTer Chat only) Answers a message that arrived with the NarraTer Chat
header. Every say is a bubble the user sees; the last one closes your turn —
send as many as the answer needs, and don't repeat the answer in the terminal.
The heredoc takes any text as-is (quoting breaks on apostrophes, $, backticks).
To show an image with prose, put ![Description](/absolute/path.png) on its own
line. An identical reply sent right after the previous one counts once."#,
        "recall" => r#"Usage:
  narrater recall [thread] [--turns N | --last day|week|month | --from YYYY-MM-DD [--to YYYY-MM-DD] | --all] [--json]
  narrater recall list
  narrater recall attachment ID

(NarraTer Chat only) Prints what you and the user already said in a thread
(default: the current thread's last 10 turns; N between 1 and 500). Run it on
the first chat message, when the thread color changes, and after a restart or
a context compaction. Threads: blue, purple, pink, red, orange, yellow, green.
Dates are local; --to includes that whole day. Attachments have stable ids:
`recall attachment ID` prints a path you can open."#,
        "portal" => r#"Usage:
  narrater portal create URL ["Name"] [--size WxH]   new portal next to you, connected to you
  narrater portal edit "Portal" --url URL            point a portal at a new URL
  narrater portal close "Portal"                     remove a portal (only on explicit user request)

  narrater portal snapshot "Portal"                  elements with refs (@e1…) + viewport, url, title
  narrater portal info "Portal"                      url, title, viewport
  narrater portal screenshot "Portal"                PNG of the viewport; prints its path
  narrater portal check "Portal"                     peek: same as screenshot

  narrater portal navigate "Portal" "url"
  narrater portal click "Portal" @e3                 ref, CSS selector or x,y
  narrater portal fill "Portal" @e2 "text"           clear the field, then type
  narrater portal type "Portal" [@e2] "text"         append to the focused (or given) element
  narrater portal key "Portal" "Enter"               Enter, Tab, Escape, Backspace, Arrow*, ctrl+a, shift+tab…
  narrater portal hover|focus "Portal" @e3
  narrater portal select "Portal" @e5 "Option"       pick a <select> option by text or value
  narrater portal check|uncheck "Portal" @e6         set a checkbox
  narrater portal selectall|clear "Portal" [@e2]     on the focused (or given) field
  narrater portal scroll "Portal" down 300 [@e5|x,y] up/down/left/right, at the viewport center or a target
  narrater portal scrollintoview "Portal" @e10
  narrater portal drag "Portal" @e3 @e7
  narrater portal text "Portal" @e1                  element text
  narrater portal html "Portal"                      full page HTML
  narrater portal evaluate "Portal" "js"             run JavaScript, print the result
  narrater portal wait "Portal" @e3 [timeoutMs]      wait for an element (default 5000ms)
  narrater portal logs-start "Portal"                start capturing console output
  narrater portal logs "Portal"                      read (and clear) captured console output
  narrater portal resize "Portal" 390 844            set the CSS viewport (responsive QA)
  narrater portal ua "Portal" [preset]               ios, android, firefox-android, edge-android,
                                                     chrome, firefox, edge, desktop; omit to read

Android devices (emulators or phones over adb):
  narrater portal devices                            phones, running emulators, Android Studio AVDs
  narrater portal create --simulator ID ["Name"]     portal on a device (serial) or AVD (booted if needed)
  On a device portal: snapshot (accessibility tree, device pixels), click/tap,
  type, key (Enter, back…), scroll, swipe, button home|back|recents|menu|power|
  volumeup|volumedown, home, lock, launch|terminate PACKAGE, navigate URL,
  screenshot, info. Browser-only commands are refused.

Workflow: snapshot → act with refs → snapshot again to verify (refs go stale
when the page changes). Prefer snapshot + refs over screenshot + coordinates.
Only portals connected to you are reachable (`narrater list`)."#,
        "recruit" => r#"Usage:
  narrater recruit "Name" [--preset P] [--role R] [--floor F] [--workspace W] [--command C] [--dir PATH]
  narrater recruit ["New Name"] --preset P --replace "Old"

(Maestro only) Spawns a new terminal on the canvas next to you, names it and
connects it to you. Returns when the terminal exists; the agent may still be
booting, and your first `narrater ask` simply waits until it is ready.

Run `narrater list` first: if a connected teammate already has a fitting role,
ask it instead of recruiting a duplicate.

  Name       a short codename you invent (not the role name); the canvas
             appends (2), (3)… on a collision
  --preset   an agent preset (`narrater preset list`); default: a copy of you
  --role     an existing role (`narrater role list`); the recruit boots with
             its prompt as CLAUDE.md / AGENTS.md
  --command  override the program; matching a preset's program uses that preset
  --floor    put it on another floor of this workspace ("Ground" for the
             ground level); without --dir it works in that floor's worktree
  --workspace  put it in another workspace (its ground, or --floor there;
             `narrater workspace list` shows them). It starts in that
             workspace's directory and runs with it — the workspace starts
             running if it wasn't. You address it as `Name @ Workspace`,
             exactly as `narrater list` prints it
  --dir      working directory (must exist); default: yours

--replace swaps the agent running on an existing teammate in place: the node,
its connections and position survive; only the process restarts (chat history
is lost — brief it again). Needs --preset or --command; --role and --dir
optional; the positional name renames it."#,
        "dismiss" => r#"Usage:
  narrater dismiss "Name"

(Maestro only) Stops a connected teammate's process and removes its terminal.
Notes wired only to it become unreachable from the CLI — to swap its agent,
use `narrater recruit --replace` instead."#,
        "role" => r#"Usage:
  narrater role list
  narrater role show "Name"
  narrater role create "Name" "Prompt"
  narrater role write "Name" "Prompt"
  narrater role edit "Name" "old text" "new text"
  narrater role assign "Recruit" "Role" | narrater role assign "Recruit" --none
  narrater role delete "Name" [--force]

(Maestro only) Role presets are the instructions a recruit boots with
(<your_assigned_role> in its CLAUDE.md / AGENTS.md). Be specific: name and
prompt should leave no doubt about the recruit's scope, and tell it to run
`narrater list` before asking anyone anything. `assign` restarts the recruit
with the new role (connections kept, chat history lost)."#,
        "preset" => r#"Usage:
  narrater preset list

(Maestro only) Lists the agent presets you can pass to `recruit --preset`."#,
        "notify" => r#"Usage:
  narrater notify "message"

(Maestro only) Sends the user a desktop notification. Only when the user asked
to be notified — otherwise assume they're following along on the canvas."#,
        "workspace" => r#"Usage:
  narrater workspace list
  narrater workspace create "Name" --dir PATH [--from "Source"] [--group "G"] [--folder "F"] [--root]
  narrater workspace rename "Current" "New"
  narrater workspace move "Name" --group "G" | --folder "F" | --root

(Maestro only) Workspaces are the canvases in the sidebar, each rooted in a
project directory. Creation is silent: the new workspace appears in the
sidebar and its terminals start when the user opens it. The directory must
already exist (create it, and `git init` if wanted, first).

--from clones an existing workspace: its whole layout (terminals, notes,
floors, connections) with every path under the source's directory retargeted
onto PATH; terminals start fresh, and floors come over as plain floors.
--group files it under a sidebar group; --folder in a folder (matched
anywhere, created at the root when missing; with --group, inside that group;
with --root, only root folders). `move` refiles; --root alone moves it out of
any group or folder. There is no delete command — that stays in the UI."#,
        "floor" => r#"Usage:
  narrater floor list
  narrater floor create "Name" [--branch NAME] [--existing-branch] [--copy] [--no-git] [--copy-ground]
  narrater floor create ["Name"] --pull-request NUMBER|URL [--copy] [--copy-ground]
  narrater floor status ["Text"] [--state working|blocked|review|done] [--progress 2/5] [--floor "F"] [--clear]
  narrater floor land "Name" [--into BRANCH] [--delete-branch] [--keep-floor]
  narrater floor delete "Name" [--keep-branch] [--force]

(Maestro only, except `status`) A floor is a separate canvas inside this workspace. When the
workspace directory is a git repository, the floor gets its own git worktree
on its own branch (default: a slug of the name), so a teammate can work there
without touching the ground checkout. Otherwise it's a plain floor sharing the
directory, and the answer says why.

  --branch NAME         branch for the worktree (fails if it already exists)
  --existing-branch     check out an existing branch instead (not the one
                        Ground has checked out — add --copy for that)
  --copy                a full copy of the repository instead of a worktree,
                        only where the disk clones instantly (btrfs/XFS); it
                        can share Ground's branch, and landing brings its work back
  --no-git              skip isolation and share the ground directory
  --copy-ground         start with a copy of the ground floor's layout

--pull-request reviews an open pull/merge request (42, #42 or its link): the
host's CLI (gh or glab, signed in) says where it comes from, its branch is
brought into the project and checked out on the floor (named #42 and its title).
From the project's own repository the floor has that branch, so pushes reach
the pull request; from a fork it gets a copy of the commits as pr-42.
Refused when the pull request isn't open, is already on a floor, moved while
fetched, or your local branch of that name went its own way or pushes
somewhere else. --branch, --existing-branch and --no-git don't apply.

Staff a floor without leaving yours: `narrater recruit "Name" --floor "F"` —
the recruit works in the floor's worktree and stays connected to you, so ask
and check work across floors.

status — the one line the user sees on a floor's chip. Any terminal posts it
for the floor it is on; only the Maestro may name another with --floor
("Ground" for the ground level). --state defaults to working; moving into
blocked, review or done notifies the user. --progress takes 2/5, 40% or 0.4.
Without text (or flags) it prints the current status; --clear removes it.

land — ONLY when the user asks, from a terminal on Ground. Removes the floor
and keeps its branch in the project; --into BRANCH also merges it there
(--delete-branch then deletes the floor's branch); --keep-floor leaves the
floor in place. Refused, with nothing changed, on uncommitted work on the
floor or the ground, or when the merge would conflict (the files are named).

delete — ONLY when the user asks, from Ground. Removes the floor, everything
on it and its worktree, and deletes its branch unless --keep-branch. A
worktree with uncommitted work is refused unless --force (which discards it)."#,
        "routine" => r#"Usage:
  narrater routine list
  narrater routine show "Name" [--id PREFIX]
  narrater routine create "Name" --command "..." <schedule> [options]
  narrater routine edit "Name" [--id PREFIX] [--name "New"] [--command ...] [schedule] [options]
  narrater routine enable|disable|run|delete "Name" [--id PREFIX]

(Maestro only) Routines fire a command — a shell command or a prompt for an
agent — into a terminal of this scene on a schedule, or show a desktop
reminder. Delivery waits until the terminal is idle, like `ask`.

Schedule (exactly one):
  --every 30m                 interval: 45s, 30m, 2h, 1h30m (a bare number = minutes)
  --daily 09:00               every day at a 24-hour time
  --weekly mon,wed,fri@09:00  chosen weekdays at a time
  --once "2026-06-20 15:00"   a single time (YYYY-MM-DD HH:MM)

Target: omit to fire into your own terminal; --terminal "Name" for another
terminal of the scene; --reminder for a desktop notification instead.

Options:
  --count N            stop after N fires        --until 2026-07-01   stop after that day
  --pre-run "script"   runs first (sh); must exit 0 or the fire is skipped. Its
                       stdout replaces {{output}} in --command. Env:
                       $NARRATER_SCENE_NAME $NARRATER_SCENE_DIR
                       $NARRATER_ROUTINE_NAME $NARRATER_TERMINAL_NAME
  --no-skip-if-busy    queue the fire even if the terminal is mid-task
                       (default: a fire is skipped while it's busy)
  --no-notify          no toast when it fires    --disabled   create it paused

edit changes only the fields you pass (--name renames; a new schedule resets
the fire counters; --skip-if-busy / --notify turn those back on). `run` fires
once now. `delete` is destructive — only on an explicit user request. Two
routines with the same name? Add --id with the prefix from `routine list`.

  narrater routine create "Daily digest" --command "Summarize today's commits:

{{output}}" --daily 18:00 --pre-run "git log --since=midnight --oneline"
  narrater routine create "Standup" --command "Time for standup" --weekly mon,tue,wed,thu,fri@09:00 --reminder"#,
        "debug" => r#"Usage:
  narrater debug

Prints the environment and tests the connection to NarraTer."#,
        _ => return None,
    })
}

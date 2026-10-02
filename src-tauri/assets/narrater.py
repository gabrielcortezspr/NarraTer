#!/usr/bin/env python3
"""NarraTer — a spatial canvas for AI agents and terminals.

The narrater CLI lets agents talk to each other and share sticky notes on the
canvas. Only agents and notes connected to you on the canvas are reachable.

Usage:
  narrater list                                    List connected agents and notes
  narrater ask "Agent" "prompt"                    Send a prompt to a connected agent; returns its answer when it finishes
  narrater ask --batch '{"A": "prompt", "B": "prompt"}'
                                                   Ask several agents in parallel; JSON array once all finish
  narrater ask "Agent" --raw "2\\n"                 Type raw input into the agent's terminal (menus, keys, shell input).
                                                   Escapes: \\n Enter, \\t Tab, \\e ESC, \\xNN byte (\\x03 = Ctrl-C), e.g. \\e[A up arrow
  narrater check "Agent"                           Read what the agent's terminal shows right now
  narrater note create ["content"] [--name "Name"]  Create a note on the canvas, linked to you; prints its name
  narrater note read "Note" [offset] [limit]       Read a note with line numbers (optional line range)
  narrater note write "Note" "content"             Replace a note's content entirely
  narrater note edit "Note" "old text" "new text"  Replace a substring within a note
  narrater note delete "Note"                      Remove a connected note (only on explicit user request)
  narrater connect "From" "To"                     Wire two things you can reach (agents or notes)
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
  narrater recruit "Name" [--preset P] [--role R] [--floor F] [--command C] [--dir PATH]
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
  narrater floor create "Name" [--branch B] [--existing-branch] [--no-git] [--copy-ground]
                                                   (Maestro only) New floor of this workspace — a git worktree when possible
  narrater floor list                              (Maestro only) Ground + floors with branch, path, node count
  narrater routine list | show "Name"              (Maestro only) Scheduled routines of this scene
  narrater routine create "Name" --command "..." <schedule> [options]
                                                   (Maestro only) Schedule a prompt/command (default target: you)
  narrater routine edit "Name" [flags]             (Maestro only) Change only the fields you pass
  narrater routine enable|disable|run|delete "Name" (Maestro only) Resume, pause, fire now, remove
                                                   Schedules: --every 30m | --daily 09:00 | --weekly mon,fri@09:00 | --once "2026-06-20 15:00"
                                                   Options: --terminal "Name" | --reminder | --count N | --until DATE | --pre-run "..." | --no-notify | --disabled
  narrater debug                                   Diagnose connection issues
  narrater help [command]                          Show this help, or one command in detail

Prompt arguments decode \\n to a newline and \\t to a tab; double the
backslash to send them literally (\\\\n delivers \\n).
"""
import json
import os
import socket
import string
import sys

HELP = {
    "ask": """Usage:
  narrater ask "Agent" "prompt" [--timeout SECONDS]
  narrater ask --batch '{"A": "prompt", "B": "prompt"}'
  narrater ask "Agent" --raw "2\\n"

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
Escapes: \\n Enter, \\t Tab, \\e ESC, \\xNN byte (\\x03 = Ctrl-C). Special keys
are ESC sequences, e.g. \\e[A up arrow, \\e[Z Shift-Tab.""",
    "check": """Usage:
  narrater check "Agent"

Prints what the agent's terminal is showing right now without sending
anything. Use it to see whether earlier work finished, or to follow a long
task while it runs. Don't mistake unsent text in its input box for
instructions to you.""",
    "note": """Usage:
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
`delete` is destructive: only run it when the user explicitly asks.""",
    "list": """Usage:
  narrater list

Shows who you are (You:), the agents connected to you with their status and
role, and the notes you can read and write. Always run it first to get exact
names.""",
    "connect": """Usage:
  narrater connect "From" "To"

Wires two things you can reach (yourself, a connected agent, or a connected
note). Agent ↔ agent lets both sides `ask` each other; note ↔ agent shares the
note. Tell the agents about the new connection — they are not notified.""",
    "portal": """Usage:
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
Only portals connected to you are reachable (`narrater list`).""",
    "recruit": """Usage:
  narrater recruit "Name" [--preset P] [--role R] [--command C] [--dir PATH]
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
  --dir      working directory (must exist); default: yours

--replace swaps the agent running on an existing teammate in place: the node,
its connections and position survive; only the process restarts (chat history
is lost — brief it again). Needs --preset or --command; --role and --dir
optional; the positional name renames it.""",
    "dismiss": """Usage:
  narrater dismiss "Name"

(Maestro only) Stops a connected teammate's process and removes its terminal.
Notes wired only to it become unreachable from the CLI — to swap its agent,
use `narrater recruit --replace` instead.""",
    "role": """Usage:
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
with the new role (connections kept, chat history lost).""",
    "preset": """Usage:
  narrater preset list

(Maestro only) Lists the agent presets you can pass to `recruit --preset`.""",
    "notify": """Usage:
  narrater notify "message"

(Maestro only) Sends the user a desktop notification. Only when the user asked
to be notified — otherwise assume they're following along on the canvas.""",
    "workspace": """Usage:
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
any group or folder. There is no delete command — that stays in the UI.""",
    "floor": """Usage:
  narrater floor list
  narrater floor create "Name" [--branch NAME] [--existing-branch] [--no-git] [--copy-ground]

(Maestro only) A floor is a separate canvas inside this workspace. When the
workspace directory is a git repository, the floor gets its own git worktree
on its own branch (default: a slug of the name), so a teammate can work there
without touching the ground checkout. Otherwise it's a plain floor sharing the
directory, and the answer says why.

  --branch NAME         branch for the worktree (fails if it already exists)
  --existing-branch     check out an existing branch instead
  --no-git              skip isolation and share the ground directory
  --copy-ground         start with a copy of the ground floor's layout

Staff a floor without leaving yours: `narrater recruit "Name" --floor "F"` —
the recruit works in the floor's worktree and stays connected to you, so ask
and check work across floors. Landing a floor's branch is up to the user.""",
    "routine": """Usage:
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

  narrater routine create "Daily digest" --command "Summarize today's commits:\n\n{{output}}" --daily 18:00 --pre-run "git log --since=midnight --oneline"
  narrater routine create "Standup" --command "Time for standup" --weekly mon,tue,wed,thu,fri@09:00 --reminder""",
    "debug": """Usage:
  narrater debug

Prints the environment and tests the connection to NarraTer.""",
}

ENV_KEYS = ("NARRATER_SOCKET", "NARRATER_ID", "NARRATER_LABEL", "NARRATER_CLI")


def fail(msg, code=1):
    print(msg, file=sys.stderr)
    sys.exit(code)


def request(payload, sock_path=None):
    sock_path = sock_path or os.environ.get("NARRATER_SOCKET", "")
    if not sock_path:
        fail("Error: NARRATER_SOCKET not set. Run this from a NarraTer terminal (`narrater debug` to diagnose).")
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        s.connect(sock_path)
        s.sendall(json.dumps(payload).encode("utf-8"))
        s.shutdown(socket.SHUT_WR)
        chunks = []
        while True:
            c = s.recv(65536)
            if not c:
                break
            chunks.append(c)
        return b"".join(chunks).decode("utf-8", errors="replace")
    except OSError as e:
        fail(f"Error: cannot reach NarraTer at {sock_path} ({e}). Run `narrater debug`.")
    finally:
        s.close()


def base_payload(mode):
    return {
        "from": os.environ.get("NARRATER_ID", ""),
        "token": os.environ.get("NARRATER_TOKEN", ""),
        "mode": mode,
    }


def decode(text, raw=False):
    """Decodes \\n, \\t and \\\\ (plus \\r, \\e, \\xNN, \\0 with raw=True)."""
    out = []
    i = 0
    while i < len(text):
        c = text[i]
        if c != "\\" or i + 1 >= len(text):
            out.append(c)
            i += 1
            continue
        n = text[i + 1]
        if n == "n":
            out.append("\r" if raw else "\n")
        elif n == "t":
            out.append("\t")
        elif n == "\\":
            out.append("\\")
        elif raw and n == "r":
            out.append("\r")
        elif raw and n == "e":
            out.append("\x1b")
        elif raw and n == "0":
            out.append("\x00")
        elif raw and n == "x" and len(text) >= i + 4 and all(ch in string.hexdigits for ch in text[i + 2:i + 4]):
            out.append(chr(int(text[i + 2:i + 4], 16)))
            i += 4
            continue
        else:
            out.append(c + n)
        i += 2
    return "".join(out)


def pop_flag(args, flag, has_value=True):
    """Removes `flag [value]` from args; returns the value (True if no value)."""
    if flag not in args:
        return None
    i = args.index(flag)
    if not has_value:
        del args[i]
        return True
    if i + 1 >= len(args):
        fail(f"Error: {flag} requires a value")
    value = args[i + 1]
    del args[i:i + 2]
    return value


def emit(out):
    if out:
        sys.stdout.write(out if out.endswith("\n") else out + "\n")
    if out.startswith("Error"):
        sys.exit(1)


def canvas(action, params):
    payload = base_payload("canvas")
    payload["action"] = action
    payload["params"] = params
    emit(request(payload))


def cmd_ask(args):
    batch = pop_flag(args, "--batch")
    raw = pop_flag(args, "--raw")
    timeout = pop_flag(args, "--timeout")
    if batch is not None:
        try:
            targets = json.loads(batch)
        except ValueError as e:
            fail(f"Error: --batch is not valid JSON ({e})")
        if not isinstance(targets, dict) or not targets:
            fail('Error: --batch takes a JSON object, e.g. \'{"A": "prompt", "B": "prompt"}\'')
        payload = base_payload("batch")
        payload["batch"] = targets
    elif raw is not None:
        if len(args) != 1:
            fail('Usage: narrater ask "Agent" --raw "input"')
        payload = base_payload("raw")
        payload["to"] = args[0]
        payload["msg"] = decode(raw, raw=True)
    else:
        if len(args) < 2:
            fail('Usage: narrater ask "Agent" "prompt"   (run `narrater list` for names)')
        payload = base_payload("ask")
        payload["to"] = args[0]
        payload["msg"] = decode(" ".join(args[1:]))
    if timeout is not None:
        try:
            payload["timeout_secs"] = int(timeout)
        except ValueError:
            fail("Error: --timeout takes a number of seconds")
    emit(request(payload))


def cmd_note(args):
    if not args:
        fail(HELP["note"])
    sub, rest = args[0], args[1:]
    if sub == "create":
        name = pop_flag(rest, "--name")
        canvas("note_create", {"content": decode(" ".join(rest)) if rest else "", "name": name})
    elif sub == "read":
        if not rest:
            fail('Usage: narrater note read "Note" [offset] [limit]')
        params = {"name": rest[0]}
        try:
            if len(rest) > 1:
                params["offset"] = int(rest[1])
            if len(rest) > 2:
                params["limit"] = int(rest[2])
        except ValueError:
            fail("Error: offset and limit must be numbers")
        canvas("note_read", params)
    elif sub == "write":
        if len(rest) < 2:
            fail('Usage: narrater note write "Note" "content"')
        canvas("note_write", {"name": rest[0], "content": decode(" ".join(rest[1:]))})
    elif sub == "edit":
        if len(rest) != 3:
            fail('Usage: narrater note edit "Note" "old text" "new text"')
        canvas("note_edit", {"name": rest[0], "old": decode(rest[1]), "new": decode(rest[2])})
    elif sub == "delete":
        if len(rest) != 1:
            fail('Usage: narrater note delete "Note"')
        canvas("note_delete", {"name": rest[0]})
    else:
        fail(HELP["note"])


def cmd_portal(args):
    if not args:
        fail(HELP["portal"])
    verb, rest = args[0], args[1:]
    if verb == "create":
        size = pop_flag(rest, "--size")
        simulator = pop_flag(rest, "--simulator")
        if simulator is not None:
            if len(rest) > 1:
                fail('Usage: narrater portal create --simulator ID ["Name"]')
            canvas("portal_create", {"simulator": simulator, "name": rest[0] if rest else ""})
            return
        if not rest or len(rest) > 2:
            fail('Usage: narrater portal create URL ["Name"] [--size WxH]')
        canvas("portal_create", {"url": rest[0], "name": rest[1] if len(rest) > 1 else "", "size": size or ""})
        return
    if verb == "devices":
        payload = base_payload("portal")
        payload["to"] = ""
        payload["msg"] = "devices"
        payload["args"] = []
        emit(request(payload))
        return
    if verb == "close":
        if len(rest) != 1:
            fail('Usage: narrater portal close "Portal"')
        canvas("portal_close", {"name": rest[0]})
        return
    if not rest:
        fail(f'Usage: narrater portal {verb} "Portal" …   (run `narrater help portal`)')
    name, rest = rest[0], rest[1:]
    if verb == "edit":
        url = pop_flag(rest, "--url")
        if url is None or rest:
            fail('Usage: narrater portal edit "Portal" --url URL')
        verb, rest = "navigate", [url]
    payload = base_payload("portal")
    payload["to"] = name
    payload["msg"] = verb
    payload["args"] = rest
    emit(request(payload))


def cmd_recruit(args):
    if pop_flag(args, "--workspace") is not None:
        fail("Error: --workspace isn't supported yet — NarraTer runs one workspace at a time; recruit here or on a --floor")
    floor = pop_flag(args, "--floor")
    preset = pop_flag(args, "--preset")
    role = pop_flag(args, "--role")
    command = pop_flag(args, "--command")
    directory = pop_flag(args, "--dir")
    replace = pop_flag(args, "--replace")
    if len(args) > 1:
        fail(HELP["recruit"])
    if not args and replace is None:
        fail('Usage: narrater recruit "Name" [--preset P] [--role R]   (invent a short codename)')
    canvas("recruit", {
        "name": args[0] if args else "",
        "preset": preset or "",
        "role": role or "",
        "command": command or "",
        "dir": directory or "",
        "replace": replace or "",
        "floor": floor or "",
    })


def cmd_role(args):
    if not args:
        fail(HELP["role"])
    sub, rest = args[0], args[1:]
    if sub == "list" and not rest:
        canvas("role_list", {})
    elif sub == "show" and len(rest) == 1:
        canvas("role_show", {"name": rest[0]})
    elif sub in ("create", "write") and len(rest) == 2:
        canvas("role_" + sub, {"name": rest[0], "prompt": decode(rest[1])})
    elif sub == "edit" and len(rest) == 3:
        canvas("role_edit", {"name": rest[0], "old": decode(rest[1]), "new": decode(rest[2])})
    elif sub == "assign":
        none = pop_flag(rest, "--none", has_value=False)
        if none and len(rest) == 1:
            canvas("role_assign", {"recruit": rest[0], "none": True})
        elif not none and len(rest) == 2:
            canvas("role_assign", {"recruit": rest[0], "name": rest[1]})
        else:
            fail('Usage: narrater role assign "Recruit" "Role" | narrater role assign "Recruit" --none')
    elif sub == "delete":
        force = pop_flag(rest, "--force", has_value=False)
        if len(rest) != 1:
            fail('Usage: narrater role delete "Name" [--force]')
        canvas("role_delete", {"name": rest[0], "force": bool(force)})
    elif "--scope" in rest:
        fail("Error: NarraTer roles are global — drop --scope")
    else:
        fail(HELP["role"])


ROUTINE_VALUE_FLAGS = ("--id", "--command", "--every", "--daily", "--weekly", "--once", "--terminal",
                       "--count", "--until", "--pre-run")
ROUTINE_BOOL_FLAGS = ("--reminder", "--no-skip-if-busy", "--skip-if-busy", "--no-notify", "--notify", "--disabled")


def cmd_workspace(args):
    if not args:
        fail(HELP["workspace"])
    verb, rest = args[0], args[1:]
    params = {"verb": verb}
    for flag in ("--dir", "--from", "--group", "--folder"):
        value = pop_flag(rest, flag)
        if value is not None:
            params[flag[2:]] = value
    if pop_flag(rest, "--root", has_value=False):
        params["root"] = True
    if verb == "list":
        if rest:
            fail("Usage: narrater workspace list")
    elif verb == "rename":
        if len(rest) != 2:
            fail('Usage: narrater workspace rename "Current Name" "New Name"')
        params["name"], params["new_name"] = rest
    elif verb in ("create", "move"):
        if len(rest) != 1:
            fail(HELP["workspace"])
        params["name"] = rest[0]
    else:
        fail(HELP["workspace"])
    payload = base_payload("workspace")
    payload["params"] = params
    emit(request(payload))


def cmd_floor(args):
    if not args:
        fail(HELP["floor"])
    verb, rest = args[0], args[1:]
    if verb == "list":
        canvas("floor_list", {})
    elif verb == "create":
        branch = pop_flag(rest, "--branch")
        params = {
            "branch": branch or "",
            "existing_branch": bool(pop_flag(rest, "--existing-branch", has_value=False)),
            "no_git": bool(pop_flag(rest, "--no-git", has_value=False)),
            "copy_ground": bool(pop_flag(rest, "--copy-ground", has_value=False)),
        }
        if len(rest) != 1:
            fail('Usage: narrater floor create "Name" [--branch B] [--existing-branch] [--no-git] [--copy-ground]')
        params["name"] = rest[0]
        canvas("floor_create", params)
    else:
        fail(HELP["floor"])


def cmd_routine(args):
    if not args:
        fail(HELP["routine"])
    verb, rest = args[0], args[1:]
    if verb not in ("list", "show", "create", "edit", "enable", "disable", "run", "delete"):
        fail(HELP["routine"])
    params = {"verb": verb}
    new_name = pop_flag(rest, "--name")
    if new_name is not None:
        if verb != "edit":
            fail("Error: --name renames a routine in `routine edit`; the name is the first argument")
        params["new_name"] = new_name
    for flag in ROUTINE_VALUE_FLAGS:
        value = pop_flag(rest, flag)
        if value is not None:
            params[flag[2:].replace("-", "_")] = value
    for flag in ROUTINE_BOOL_FLAGS:
        if pop_flag(rest, flag, has_value=False):
            params[flag[2:].replace("-", "_")] = True
    if "command" in params:
        params["command"] = decode(params["command"])
    if "count" in params:
        try:
            params["count"] = int(params["count"])
        except ValueError:
            fail("Error: --count takes a number")
    unknown = [a for a in rest if a.startswith("--")]
    if unknown:
        fail(f"Error: unknown flag {unknown[0]} — run `narrater help routine`")
    if verb == "list":
        if rest:
            fail("Usage: narrater routine list")
    elif len(rest) != 1:
        fail(f'Usage: narrater routine {verb} "Name" …   (run `narrater help routine`)')
    else:
        params["name"] = rest[0]
    payload = base_payload("routine")
    payload["params"] = params
    emit(request(payload))


def cmd_debug():
    print("=== NarraTer CLI Diagnostics ===\n")
    print("--- Environment ---")
    for k in ENV_KEYS:
        print(f"{k:<16} {os.environ.get(k, '(not set)')}")
    print(f"{'NARRATER_TOKEN':<16} {'(set)' if os.environ.get('NARRATER_TOKEN') else '(not set)'}")
    print(f"{'CLI':<16} {os.path.abspath(__file__)}")
    sock = os.environ.get("NARRATER_SOCKET", "")
    print("\n--- Transport probe ---")
    if not sock:
        print("Socket:          (no NARRATER_SOCKET — not inside a NarraTer terminal?)")
        return
    exists = os.path.exists(sock)
    print(f"Socket exists:   {'yes' if exists else 'no'}")
    if exists:
        st = os.stat(sock)
        print(f"Socket mode:     {oct(st.st_mode & 0o777)[2:]}")
        print(f"Socket owner:    {st.st_uid} (you are {os.getuid()})")
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            s.settimeout(5)
            s.connect(sock)
            s.sendall(json.dumps({"mode": "ping"}).encode())
            s.shutdown(socket.SHUT_WR)
            ok = s.recv(64).decode().strip() == "ok"
            print(f"Ping:            {'OK' if ok else 'unexpected answer'}")
        except OSError as e:
            print(f"Ping:            FAILED ({e})")
        finally:
            s.close()
        if os.environ.get("NARRATER_ID"):
            out = request(base_payload("whoami"))
            print(f"Identity:        {out.strip().replace(chr(10), ' · ')}")
    else:
        print("Hint:            stale NARRATER_SOCKET (NarraTer restarted?) — restart this terminal from the canvas.")


def main():
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help", "help"):
        topic = args[1] if len(args) > 1 and args[0] == "help" else None
        print(HELP.get(topic, __doc__.strip()) if topic else __doc__.strip())
        return
    cmd, rest = args[0], args[1:]
    if "--help" in rest or "-h" in rest:
        print(HELP.get(cmd, __doc__.strip()))
        return
    if cmd == "list":
        canvas("list", {})
    elif cmd == "ask":
        cmd_ask(rest)
    elif cmd == "check":
        if len(rest) != 1:
            fail('Usage: narrater check "Agent"')
        payload = base_payload("check")
        payload["to"] = rest[0]
        emit(request(payload))
    elif cmd == "note":
        cmd_note(rest)
    elif cmd == "connect":
        if len(rest) != 2:
            fail('Usage: narrater connect "From" "To"')
        canvas("connect", {"from": rest[0], "to": rest[1]})
    elif cmd == "portal":
        cmd_portal(rest)
    elif cmd == "recruit":
        cmd_recruit(rest)
    elif cmd == "dismiss":
        if len(rest) != 1:
            fail('Usage: narrater dismiss "Name"')
        canvas("dismiss", {"name": rest[0]})
    elif cmd == "preset":
        if rest != ["list"]:
            fail("Usage: narrater preset list")
        canvas("preset_list", {})
    elif cmd == "role":
        cmd_role(rest)
    elif cmd == "notify":
        if not rest:
            fail('Usage: narrater notify "message"')
        canvas("notify", {"message": decode(" ".join(rest))})
    elif cmd == "routine":
        cmd_routine(rest)
    elif cmd == "workspace":
        cmd_workspace(rest)
    elif cmd == "floor":
        cmd_floor(rest)
    elif cmd == "debug":
        cmd_debug()
    else:
        fail(f"Unknown command '{cmd}'. Run `narrater help`.")


if __name__ == "__main__":
    main()

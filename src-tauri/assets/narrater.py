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
    elif cmd == "debug":
        cmd_debug()
    else:
        fail(f"Unknown command '{cmd}'. Run `narrater help`.")


if __name__ == "__main__":
    main()

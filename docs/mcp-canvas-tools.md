# MCP → Canvas: foundation and roadmap

Agents running inside NarraTer terminals can manipulate the canvas through
MCP tools exposed by the `narrater-mcp` server. This document describes the
bridge contract and the roadmap for upcoming tools.

## Flow

```
agent (claude) ── tool call canvas_* ──► narrater-mcp (stdio, ~/.local/bin)
  │ JSON {from: $NARRATER_ID, mode: "canvas", action, params}
  ▼ Unix socket /tmp/narrater-<pid>.sock
ipc.rs handle_canvas():
  validates the `from` session
  req_id = uuid; registers a oneshot::Sender in PtyStateInner.canvas_waiters[req_id]
  app.emit("canvas_request", { req_id, from, from_label, action, params })
  await oneshot (10s timeout) ──► writes the result to the socket ──► tool returns
                    ▲
frontend (src/lib/canvasBridge.ts, global listener in App):
  applies the action to useCanvasStore (single source of truth; auto-save persists)
  invoke("canvas_respond", { reqId, result })
```

Contract points:

- **`canvas_request`** (Tauri event): `{ req_id, from, from_label, action, params }`.
  `params` arrives as sent by the agent (`serde_json::Value`, may be `null`).
- **`canvas_respond`** (Tauri command, `src-tauri/src/canvas_bridge.rs`): resolves the
  waiter by `req_id`. The frontend **always** responds, including with `Error: ...`,
  so the agent is never left waiting for the timeout.
- **Timeout**: 10s (`CANVAS_TIMEOUT` in `ipc.rs`). If the frontend doesn't respond,
  the waiter is removed and the tool returns `Error: timeout waiting for the canvas`.
- **Result convention**: free-form text; an `Error:` prefix marks failure
  (narrater-mcp converts it into `isError: true`).

## ACL (v1)

Any agent with a **valid PTY session** can use the canvas tools — it
manipulates the very canvas it lives on. **Edges keep governing only
agent↔agent communication** (send/ask). Future evolution: per-node permission
(e.g. a "locked" note), per-role capabilities, and a read-only mode.

## Implemented tools

| Tool | Params | Result |
|---|---|---|
| `canvas_list_nodes` | — | JSON `[{id, type, label, x, y}]` |
| `canvas_create_note` | `content` (req), `label?`, `x?`, `y?` | `ok: note created (id: …)`. Without x/y, it spawns to the right of the agent's terminal. |
| `canvas_update_note` | `id` (id **or** label), `content` (req), `mode?: append\|replace` | `ok: note updated (id: …)`; readable error if the label is ambiguous. |
| `canvas_read_note` | `id` (id **or** label) | Note content (`(empty note)` if blank). |
| `canvas_create_text` | `text` (req), `x?`, `y?` | `ok: text created (id: …)`. Same position default as notes. |
| `canvas_move_node` | `id` (id **or** label, any node type), `x`, `y` | `ok: node moved (id: …) to (x, y)`. |
| `canvas_connect_nodes` | `source`, `target` (id **or** label) | `ok: connection created (id, type, route)`. Classifies agent-pipe/agent-note/default like the Canvas `onConnect`; `addEdge` runs `syncConnections`, so agent-pipe routes take effect in the backend immediately. Idempotent: an existing edge returns `ok: connection already existed`. |

Note on `connect_nodes`: unlike a user-drawn edge, an agent-created edge does
**not** inject the system message into the endpoints — the creating agent
already knows about the route and can introduce itself with `send_message` if
it wants.

## Roadmap (schema drafts)

- `canvas_create_terminal { agent_type, role?, instructions? }` → agent
  spawning agents; will require a user approval policy (prompt/toast) before
  executing.
- `canvas_delete_node { id }` → destructive; will require user confirmation.
- `canvas_disconnect_nodes { source, target }` → remove an edge (the connect
  counterpart).

## Gotchas

- `narrater-mcp` is **rewritten on every app boot** (`ipc.rs`); changes to the
  tool list only take effect after restarting NarraTer **and** the claude
  agents (the MCP server is a child process of claude).
- `--allowedTools mcp__narrater` (in `src/lib/tauri.ts`) pre-approves the whole
  server — new tools need no change there.
- The bridge depends on the "store as single source of truth" refactor: a node
  created by the backend enters the zustand store without resetting positions,
  and auto-save persists it without user interaction.

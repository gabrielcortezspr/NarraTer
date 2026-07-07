use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::pty::{
    enqueue_message, now_ms, queue_snapshot, record_msg, AskWaiter, LedgerEntry, PtyQueueEvent,
    PtyStateInner, PtyStatusEvent, QueuedMsg, ResponseListener, RunStatus, HOOKED_QUEUE_WAIT,
    MAX_QUEUE_WAIT, REPLY_GRANT_TTL,
};

const ASK_DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const CANVAS_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
struct IpcRequest {
    /// Sender node id (NARRATER_ID inside the terminal).
    #[serde(default)]
    from: String,
    /// Target label or node id.
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    msg: Option<String>,
    /// "send" | "ask" | "reply" | "peers" | "whoami" | "canvas"
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
    /// mode "reply": short id of the ask being answered (the frame's `#a3f2`)
    #[serde(default)]
    msg_id: Option<String>,
    /// Terminal secret (env NARRATER_TOKEN) proving the `from` identity
    #[serde(default)]
    token: Option<String>,
    /// mode "canvas": action (list_nodes, create_note, update_note…)
    #[serde(default)]
    action: Option<String>,
    /// mode "canvas": action arguments, forwarded to the frontend as-is
    #[serde(default)]
    params: Option<serde_json::Value>,
}

pub async fn start_ipc_server(app: AppHandle, state: Arc<Mutex<PtyStateInner>>) {
    write_narrater_script();
    write_narrater_mcp_script();
    write_claude_hooks_settings();

    let socket_path = format!("/tmp/narrater-{}.sock", std::process::id());
    let _ = std::fs::remove_file(&socket_path);

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[NarraTer IPC] Failed to bind socket {}: {}", socket_path, e);
            return;
        }
    };

    if let Ok(meta) = std::fs::metadata(&socket_path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        let _ = std::fs::set_permissions(&socket_path, perms);
    }

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let state_clone = Arc::clone(&state);
                let app_clone = app.clone();
                tokio::spawn(handle_connection(stream, app_clone, state_clone));
            }
            Err(e) => {
                eprintln!("[NarraTer IPC] Accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(
    mut stream: tokio::net::UnixStream,
    app: AppHandle,
    state: Arc<Mutex<PtyStateInner>>,
) {
    let mut buf = Vec::new();
    if stream.read_to_end(&mut buf).await.is_err() {
        return;
    }

    let req: IpcRequest = match serde_json::from_slice(&buf) {
        Ok(r) => r,
        Err(_) => {
            let _ = stream.write_all("Error: invalid request format\n".as_bytes()).await;
            return;
        }
    };

    // Anti-spoof: whoever identifies themselves (`from`) proves it with their
    // own terminal's token (env NARRATER_TOKEN). A missing session proceeds
    // and hits each handler's specific errors.
    if !req.from.is_empty() {
        let token_ok = {
            let inner = state.lock().unwrap();
            inner
                .sessions
                .get(&req.from)
                .map(|s| req.token.as_deref() == Some(s.token.as_str()))
                .unwrap_or(true)
        };
        if !token_ok {
            let _ = stream
                .write_all("Error: invalid NARRATER_TOKEN — the given identity doesn't match\n".as_bytes())
                .await;
            return;
        }
    }

    let reply = match req.mode.as_deref() {
        Some("whoami") => handle_whoami(&req, &state),
        Some("peers") => handle_peers(&req, &state),
        Some("send") => handle_send(&req, &app, &state),
        Some("ask") => handle_ask(&req, &app, &state).await,
        Some("reply") => handle_reply(&req, &app, &state),
        Some("broadcast") => handle_broadcast(&req, &app, &state),
        Some("inbox") => handle_inbox(&req, &app, &state),
        Some("notify-idle") => handle_notify_idle(&req, &app, &state),
        Some("canvas") => handle_canvas(&req, &app, &state).await,
        _ => "Usage: narrater send|ask <target> <message> | narrater reply <id> <answer> | narrater broadcast <message> | narrater inbox | narrater peers | narrater whoami\n".to_string(),
    };

    let _ = stream.write_all(reply.as_bytes()).await;
}

fn label_of(inner: &PtyStateInner, id: &str) -> String {
    inner.labels.get(id).cloned().unwrap_or_else(|| id.to_string())
}

/// Resolves `to` (label or node id) and validates the directed route
/// `from → to` against the canvas connection graph.
fn resolve_route(
    req: &IpcRequest,
    state: &Arc<Mutex<PtyStateInner>>,
) -> Result<(String, String), String> {
    let to = req.to.clone().ok_or("Error: no target given\n")?;
    if req.from.is_empty() {
        return Err("Error: NARRATER_ID not set. Are you inside a NarraTer terminal?\n".into());
    }

    let inner = state.lock().unwrap();
    let target_id = inner
        .label_to_id
        .get(&to)
        .cloned()
        .or_else(|| inner.sessions.contains_key(&to).then(|| to.clone()))
        .ok_or_else(|| format!("Error: no agent named '{}'\n", to))?;

    if !route_allowed(&inner.connections, &inner.reply_grants, &req.from, &target_id) {
        let from_label = label_of(&inner, &req.from);
        return Err(format!(
            "Error: no connection from '{}' to '{}' — connect the terminals on the canvas\n",
            from_label, to
        ));
    }

    let from_label = label_of(&inner, &req.from);
    Ok((target_id, from_label))
}

/// Allowed route: a canvas edge OR a temporary reply grant (receiving a
/// message from someone authorizes replying for REPLY_GRANT_TTL).
fn route_allowed(
    connections: &std::collections::HashSet<(String, String)>,
    reply_grants: &std::collections::HashMap<(String, String), Instant>,
    from: &str,
    to: &str,
) -> bool {
    let route = (from.to_string(), to.to_string());
    connections.contains(&route)
        || reply_grants.get(&route).is_some_and(|t| t.elapsed() < REPLY_GRANT_TTL)
}

fn handle_whoami(req: &IpcRequest, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let inner = state.lock().unwrap();
    format!("id: {}\nlabel: {}\n", req.from, label_of(&inner, &req.from))
}

fn handle_peers(req: &IpcRequest, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let inner = state.lock().unwrap();
    let mut lines: Vec<String> = inner
        .connections
        .iter()
        .filter(|(from, _)| *from == req.from)
        .map(|(_, to)| {
            let status = inner
                .sessions
                .get(to)
                .map(|s| s.status.as_str())
                .unwrap_or("offline");
            format!("{}\t{}", label_of(&inner, to), status)
        })
        .collect();
    lines.sort();
    if lines.is_empty() {
        "(no connected agents — create an edge on the canvas)\n".to_string()
    } else {
        lines.join("\n") + "\n"
    }
}

fn handle_send(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let (target_id, from_label) = match resolve_route(req, state) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let msg = match req.msg.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return "Error: empty message\n".to_string(),
    };

    let to_label = { label_of(&state.lock().unwrap(), &target_id) };
    let position = enqueue_message(state, app, &target_id, QueuedMsg {
        from_label: from_label.clone(),
        from_id: Some(req.from.clone()),
        msg: msg.clone(),
        enqueued: Instant::now(),
        msg_id: None,
        delivered_tx: None,
    });
    record_msg(state, app, LedgerEntry {
        from: req.from.clone(),
        to: target_id,
        from_label,
        to_label,
        kind: "send".to_string(),
        msg,
        msg_id: None,
        ts: now_ms(),
    });
    let to = req.to.clone().unwrap_or_default();
    if position == 0 {
        format!("ok: message to '{}' will be delivered when the agent is idle\n", to)
    } else {
        format!("ok: message to '{}' queued ({} ahead)\n", to, position)
    }
}

async fn handle_ask(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let (target_id, from_label) = match resolve_route(req, state) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let msg = match req.msg.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return "Error: empty message\n".to_string(),
    };

    // AI agents answer deliberately via `reply` (clean text, no echo or ANSI,
    // concurrent asks don't mix); shells have no organic way to call reply,
    // so they keep the stdout scraping as a fallback.
    let target_is_shell = {
        let inner = state.lock().unwrap();
        inner
            .sessions
            .get(&target_id)
            .map(|s| s.agent_type == "shell")
            .unwrap_or(true)
    };
    if target_is_shell {
        ask_by_scraping(req, app, state, target_id, from_label, msg).await
    } else {
        ask_by_reply(req, app, state, target_id, from_label, msg).await
    }
}

/// Sends the same message to all peers (outgoing edges) at once — the
/// orchestrator → workers pattern. Fire-and-forget like send.
fn handle_broadcast(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    if req.from.is_empty() {
        return "Error: NARRATER_ID not set. Are you inside a NarraTer terminal?\n".into();
    }
    let msg = match req.msg.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return "Error: empty message\n".to_string(),
    };

    let (from_label, targets): (String, Vec<(String, String)>) = {
        let inner = state.lock().unwrap();
        let targets = inner
            .connections
            .iter()
            .filter(|(from, _)| *from == req.from)
            .filter(|(_, to)| inner.sessions.contains_key(to))
            .map(|(_, to)| (to.clone(), label_of(&inner, to)))
            .collect();
        (label_of(&inner, &req.from), targets)
    };
    if targets.is_empty() {
        return "Error: no connected agents — create edges on the canvas\n".to_string();
    }

    for (target_id, target_label) in &targets {
        enqueue_message(state, app, target_id, QueuedMsg {
            from_label: from_label.clone(),
            from_id: Some(req.from.clone()),
            msg: msg.clone(),
            enqueued: Instant::now(),
            msg_id: None,
            delivered_tx: None,
        });
        record_msg(state, app, LedgerEntry {
            from: req.from.clone(),
            to: target_id.clone(),
            from_label: from_label.clone(),
            to_label: target_label.clone(),
            kind: "broadcast".to_string(),
            msg: msg.clone(),
            msg_id: None,
            ts: now_ms(),
        });
    }
    let mut labels: Vec<String> = targets.into_iter().map(|(_, l)| l).collect();
    labels.sort();
    format!("ok: message sent to {} agent(s): {}\n", labels.len(), labels.join(", "))
}

/// Pulls (and drains) the caller's pending messages instead of waiting for
/// idle-gated injection — covers the agent that stays busy for a long time.
fn handle_inbox(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let msgs: Vec<QueuedMsg> = {
        let mut inner = state.lock().unwrap();
        if req.from.is_empty() || !inner.sessions.contains_key(&req.from) {
            return "Error: unknown session — are you inside a NarraTer terminal?\n".to_string();
        }
        let msgs: Vec<QueuedMsg> = inner
            .inbox
            .remove(&req.from)
            .map(|q| q.into_iter().collect())
            .unwrap_or_default();
        // Pulling the message also grants the reply route, like on delivery
        for m in &msgs {
            if let Some(from_id) = m.from_id.clone() {
                inner.reply_grants.insert((req.from.clone(), from_id), Instant::now());
            }
        }
        msgs
    };

    if msgs.is_empty() {
        return "(no pending messages)\n".to_string();
    }
    let _ = app.emit("pty_queue", crate::pty::PtyQueueEvent {
        id: req.from.clone(),
        pending: 0,
        items: Vec::new(),
    });

    let lines: Vec<String> = msgs
        .into_iter()
        .map(|mut m| {
            // A pending ask was "delivered" via pull: release the caller to
            // wait for the reply
            if let Some(tx) = m.delivered_tx.take() {
                let _ = tx.send(());
            }
            let id_part = m.msg_id.as_deref().map(|i| format!(" #{}", i)).unwrap_or_default();
            format!("from {}{}: {}", m.from_label, id_part, m.msg)
        })
        .collect();
    lines.join("\n") + "\n"
}

/// Ask with an explicit answer: delivers `[narrater from X #id]: msg` and
/// waits for the target to resolve the id via `narrater reply` / reply_message.
async fn ask_by_reply(
    req: &IpcRequest,
    app: &AppHandle,
    state: &Arc<Mutex<PtyStateInner>>,
    target_id: String,
    from_label: String,
    msg: String,
) -> String {
    let (tx, rx) = oneshot::channel::<String>();
    let (delivered_tx, delivered_rx) = oneshot::channel::<()>();

    let (msg_id, to_label, delivery_wait) = {
        let mut inner = state.lock().unwrap();
        let id = new_ask_id(&inner.ask_waiters);
        inner.ask_waiters.insert(id.clone(), AskWaiter {
            responder_id: target_id.clone(),
            asker_id: req.from.clone(),
            tx,
        });
        // Wait for delivery at least as long as the monitor's force-inject
        // backstop for this target — hooked sessions only get force-injected
        // after HOOKED_QUEUE_WAIT, so giving up earlier strands the message.
        let force_wait = inner
            .sessions
            .get(&target_id)
            .map(|s| if s.hook_idle { HOOKED_QUEUE_WAIT } else { MAX_QUEUE_WAIT })
            .unwrap_or(MAX_QUEUE_WAIT);
        (id, label_of(&inner, &target_id), force_wait + Duration::from_secs(5))
    };

    enqueue_message(state, app, &target_id, QueuedMsg {
        from_label: from_label.clone(),
        from_id: Some(req.from.clone()),
        msg: msg.clone(),
        enqueued: Instant::now(),
        msg_id: Some(msg_id.clone()),
        delivered_tx: Some(delivered_tx),
    });
    record_msg(state, app, LedgerEntry {
        from: req.from.clone(),
        to: target_id.clone(),
        from_label,
        to_label,
        kind: "ask".to_string(),
        msg,
        msg_id: Some(msg_id.clone()),
        ts: now_ms(),
    });

    if timeout(delivery_wait, delivered_rx)
        .await
        .map(|r| r.is_err())
        .unwrap_or(true)
    {
        // Give up cleanly: withdraw the undelivered message from the target's
        // queue too — otherwise it gets injected later as a zombie ask nobody
        // is waiting on, and the target burns a turn answering into the void.
        let items = {
            let mut inner = state.lock().unwrap();
            inner.ask_waiters.remove(&msg_id);
            if let Some(queue) = inner.inbox.get_mut(&target_id) {
                queue.retain(|m| m.msg_id.as_deref() != Some(msg_id.as_str()));
                if queue.is_empty() {
                    inner.inbox.remove(&target_id);
                }
            }
            queue_snapshot(&inner, &target_id)
        };
        let _ = app.emit("pty_queue", PtyQueueEvent {
            id: target_id.clone(),
            pending: items.len(),
            items,
        });
        return "Error: timeout waiting for the message to be delivered — the agent stayed busy; the message was withdrawn from the queue, try again\n".to_string();
    }

    let max = req.timeout_secs.map(Duration::from_secs).unwrap_or(ASK_DEFAULT_TIMEOUT);
    match timeout(max, rx).await {
        Ok(Ok(text)) => {
            if text.ends_with('\n') { text } else { format!("{}\n", text) }
        }
        Ok(Err(_)) => "Error: the agent exited without replying\n".to_string(),
        Err(_) => {
            state.lock().unwrap().ask_waiters.remove(&msg_id);
            format!(
                "Error: the agent did not reply within {}s — it may still be working; try again with a larger --timeout\n",
                max.as_secs()
            )
        }
    }
}

/// Short 4-hex ids, unique among the pending asks — readable in the frame
/// (`#a3f2`) and easy to type in a `narrater reply`.
fn new_ask_id(pending: &std::collections::HashMap<String, AskWaiter>) -> String {
    loop {
        let id = uuid::Uuid::new_v4().simple().to_string()[..4].to_string();
        if !pending.contains_key(&id) {
            return id;
        }
    }
}

/// Answers a pending ask: resolves the oneshot registered under the short id.
/// No edge check — the reply is the return channel of a question that already
/// reached you; only the terminal that received the ask can answer it.
fn handle_reply(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let msg_id = match req.msg_id.as_deref() {
        Some(i) if !i.is_empty() => i.trim_start_matches('#').to_string(),
        _ => return "Error: provide the message id (the #id from the received frame)\n".to_string(),
    };
    let text = match req.msg.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return "Error: empty reply\n".to_string(),
    };

    let entry = {
        let mut inner = state.lock().unwrap();
        if req.from.is_empty() || !inner.sessions.contains_key(&req.from) {
            return "Error: unknown session — are you inside a NarraTer terminal?\n".to_string();
        }
        match inner.ask_waiters.get(&msg_id) {
            None => {
                return format!(
                    "Error: no pending question with id '{}' — it expired, was already answered, or the id is wrong\n",
                    msg_id
                )
            }
            Some(w) if w.responder_id != req.from => {
                return "Error: that question was not directed to you\n".to_string()
            }
            Some(_) => {}
        }
        let waiter = inner.ask_waiters.remove(&msg_id).unwrap();
        let entry = LedgerEntry {
            from: req.from.clone(),
            to: waiter.asker_id.clone(),
            from_label: label_of(&inner, &req.from),
            to_label: label_of(&inner, &waiter.asker_id),
            kind: "reply".to_string(),
            msg: text.clone(),
            msg_id: Some(msg_id),
            ts: now_ms(),
        };
        let _ = waiter.tx.send(text);
        entry
    };
    record_msg(state, app, entry);
    "ok: reply delivered\n".to_string()
}

/// claude's Stop hook (`narrater notify-idle`): the authoritative end-of-turn
/// signal. Marks the session Idle (the queue drains on the next tick) and
/// records that this session has a hook — the silence timer no longer applies
/// to it.
fn handle_notify_idle(
    req: &IpcRequest,
    app: &AppHandle,
    state: &Arc<Mutex<PtyStateInner>>,
) -> String {
    {
        let mut inner = state.lock().unwrap();
        let Some(session) = inner.sessions.get_mut(&req.from) else {
            return "Error: unknown session — are you inside a NarraTer terminal?\n".to_string();
        };
        session.hook_idle = true;
        session.status = RunStatus::Idle;
    }
    let _ = app.emit("pty_status", PtyStatusEvent {
        id: req.from.clone(),
        status: RunStatus::Idle.as_str(),
    });
    "ok\n".to_string()
}

/// Fallback for shell targets: scrapes stdout until the target settles on Idle.
async fn ask_by_scraping(
    req: &IpcRequest,
    app: &AppHandle,
    state: &Arc<Mutex<PtyStateInner>>,
    target_id: String,
    from_label: String,
    msg: String,
) -> String {
    let req_id = uuid::Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::channel::<String>(256);
    let (delivered_tx, mut delivered_rx) = oneshot::channel::<()>();

    {
        let mut inner = state.lock().unwrap();
        inner.response_listeners.insert(req_id.clone(), ResponseListener {
            target_id: target_id.clone(),
            tx,
        });
    }

    enqueue_message(state, app, &target_id, QueuedMsg {
        from_label: from_label.clone(),
        from_id: Some(req.from.clone()),
        msg: msg.clone(),
        enqueued: Instant::now(),
        msg_id: None,
        delivered_tx: Some(delivered_tx),
    });
    let to_label = { label_of(&state.lock().unwrap(), &target_id) };
    record_msg(state, app, LedgerEntry {
        from: req.from.clone(),
        to: target_id.clone(),
        from_label: from_label.clone(),
        to_label,
        kind: "ask".to_string(),
        msg: msg.clone(),
        msg_id: None,
        ts: now_ms(),
    });

    // Phase A: wait for actual injection, discarding output that still belongs
    // to whatever the target was doing before.
    let wait_delivery = async {
        loop {
            tokio::select! {
                r = &mut delivered_rx => return r.is_ok(),
                d = rx.recv() => {
                    if d.is_none() {
                        return false;
                    }
                }
            }
        }
    };
    let delivered = timeout(MAX_QUEUE_WAIT + Duration::from_secs(5), wait_delivery)
        .await
        .unwrap_or(false);

    if !delivered {
        state.lock().unwrap().response_listeners.remove(&req_id);
        return "Error: timeout waiting for the message to be delivered to the agent\n".to_string();
    }

    // Phase B: capture until the target settles back to Idle (authoritative
    // state machine), bounded by the requested timeout.
    let max = req.timeout_secs.map(Duration::from_secs).unwrap_or(ASK_DEFAULT_TIMEOUT);
    let started = Instant::now();
    let mut response = String::new();

    loop {
        if started.elapsed() >= max {
            break;
        }
        match timeout(Duration::from_millis(400), rx.recv()).await {
            Ok(Some(data)) => response.push_str(&data),
            Ok(None) => break,
            Err(_) => {
                let target_idle = {
                    let inner = state.lock().unwrap();
                    inner
                        .sessions
                        .get(&target_id)
                        .map(|s| s.status == RunStatus::Idle)
                        .unwrap_or(true)
                };
                if target_idle && !response.is_empty() {
                    break;
                }
            }
        }
    }

    state.lock().unwrap().response_listeners.remove(&req_id);
    strip_injected_echo(&strip_ansi(&response), &from_label, &msg)
}

/// Strips ANSI escape sequences (CSI, OSC and 2-byte escapes) and carriage
/// returns, leaving only the text — raw PTY output is full of colors, redraws
/// and cursor movement.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: ESC [ ... final byte in 0x40-0x7E
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] ... terminated by BEL or ST (ESC \)
                Some(']') => {
                    while let Some(n) = chars.next() {
                        if n == '\x07' {
                            break;
                        }
                        if n == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // Charset designation: ESC ( X / ESC ) X
                Some('(') | Some(')') => {
                    chars.next();
                }
                // 2-byte escapes (ESC =, ESC >, ESC 7, …): already consumed
                _ => {}
            },
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

#[derive(Clone, serde::Serialize)]
struct CanvasRequestEvent {
    req_id: String,
    from: String,
    from_label: String,
    action: String,
    params: serde_json::Value,
}

/// Agent → canvas bridge: registers a waiter, emits `canvas_request` to the
/// frontend (which applies the action to the store and answers via
/// canvas_respond) and awaits the result. ACL v1: any agent with a valid
/// session can manipulate the canvas — edges keep governing only agent↔agent
/// communication (see docs/mcp-canvas-tools.md).
async fn handle_canvas(
    req: &IpcRequest,
    app: &AppHandle,
    state: &Arc<Mutex<PtyStateInner>>,
) -> String {
    let action = match req.action.as_deref() {
        Some(a) if !a.is_empty() => a.to_string(),
        _ => return "Error: no canvas action given\n".to_string(),
    };
    if req.from.is_empty() {
        return "Error: NARRATER_ID not set. Are you inside a NarraTer terminal?\n".into();
    }

    let req_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<String>();
    let from_label = {
        let mut inner = state.lock().unwrap();
        if !inner.sessions.contains_key(&req.from) {
            return "Error: unknown session — are you inside a NarraTer terminal?\n".to_string();
        }
        inner.canvas_waiters.insert(req_id.clone(), tx);
        label_of(&inner, &req.from)
    };

    let _ = app.emit("canvas_request", CanvasRequestEvent {
        req_id: req_id.clone(),
        from: req.from.clone(),
        from_label,
        action,
        params: req.params.clone().unwrap_or(serde_json::Value::Null),
    });

    match timeout(CANVAS_TIMEOUT, rx).await {
        Ok(Ok(result)) => {
            if result.ends_with('\n') { result } else { format!("{}\n", result) }
        }
        _ => {
            state.lock().unwrap().canvas_waiters.remove(&req_id);
            "Error: timeout waiting for the canvas\n".to_string()
        }
    }
}

/// The target PTY echoes the injected line back — `[narrater from <label>]: ...`
/// for AI agents, the bare command for shells. Cut everything up to and
/// including that echo so the caller sees only the actual reply.
fn strip_injected_echo(response: &str, from_label: &str, msg: &str) -> String {
    let marker = format!("[narrater from {}]", from_label);
    let pos = response.find(&marker).or_else(|| response.find(msg));
    if let Some(pos) = pos {
        if let Some(nl) = response[pos..].find('\n') {
            let rest = &response[pos + nl + 1..];
            return rest.trim_start_matches(['\r', '\n']).to_string();
        }
    }
    response.to_string()
}

fn write_executable_script(name: &str, content: &str) {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };

    let bin_dir = std::path::PathBuf::from(&home).join(".local").join("bin");
    if let Err(e) = std::fs::create_dir_all(&bin_dir) {
        eprintln!("[NarraTer IPC] Could not create ~/.local/bin: {}", e);
        return;
    }

    let script_path = bin_dir.join(name);
    if let Err(e) = std::fs::write(&script_path, content) {
        eprintln!("[NarraTer IPC] Failed to write {}: {}", name, e);
        return;
    }

    if let Ok(meta) = std::fs::metadata(&script_path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(&script_path, perms);
    }
}

/// Extra settings passed to claude via `--settings` at spawn (pty.rs):
/// Stop hook → `narrater notify-idle`, the authoritative idle signal.
pub fn claude_hooks_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(std::path::PathBuf::from(home).join(".local/share/narrater/claude-hooks.json"))
}

fn write_claude_hooks_settings() {
    let Some(path) = claude_hooks_path() else { return };
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("[NarraTer IPC] Could not create {}: {}", dir.display(), e);
            return;
        }
    }
    let settings = serde_json::json!({
        "hooks": {
            "Stop": [
                { "hooks": [ { "type": "command", "command": "narrater notify-idle" } ] }
            ]
        }
    });
    if let Err(e) = std::fs::write(&path, settings.to_string()) {
        eprintln!("[NarraTer IPC] Failed to write claude hooks settings: {}", e);
    }
}

fn write_narrater_script() {

    let script = r#"#!/usr/bin/env python3
"""narrater - communication between NarraTer agents

Usage:
  narrater send <target> <message>              sends and returns immediately
  narrater ask <target> <message> [--timeout N] sends and waits for the answer
  narrater reply <id> <answer>                  answers a received ask (the frame's #id)
  narrater broadcast <message>                  sends to all peers at once
  narrater inbox                                pulls (and drains) your pending messages
  narrater peers                                lists reachable agents
  narrater whoami                               shows your identity
"""
import socket, json, sys, os


def request(payload):
    sock = os.environ.get("NARRATER_SOCKET", "")
    if not sock:
        print("Error: NARRATER_SOCKET not set. Are you inside a NarraTer terminal?", file=sys.stderr)
        sys.exit(1)
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        s.connect(sock)
        s.sendall(json.dumps(payload).encode("utf-8"))
        s.shutdown(socket.SHUT_WR)
        r = b""
        while True:
            c = s.recv(4096)
            if not c:
                break
            r += c
        return r.decode("utf-8")
    except Exception as e:
        print(f"narrater error: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        s.close()


def main():
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help"):
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(0 if args else 1)

    mode = args[0]
    payload = {
        "from": os.environ.get("NARRATER_ID", ""),
        "token": os.environ.get("NARRATER_TOKEN", ""),
        "mode": mode,
    }

    if mode in ("send", "ask"):
        rest = args[1:]
        if "--timeout" in rest:
            i = rest.index("--timeout")
            try:
                payload["timeout_secs"] = int(rest[i + 1])
            except (IndexError, ValueError):
                print("Error: --timeout requires a number of seconds", file=sys.stderr)
                sys.exit(1)
            del rest[i:i + 2]
        if len(rest) < 2:
            print(f"Usage: narrater {mode} <target> <message>", file=sys.stderr)
            sys.exit(1)
        payload["to"] = rest[0]
        payload["msg"] = " ".join(rest[1:])
    elif mode == "reply":
        rest = args[1:]
        if len(rest) < 2:
            print("Usage: narrater reply <id> <answer>", file=sys.stderr)
            sys.exit(1)
        payload["msg_id"] = rest[0]
        payload["msg"] = " ".join(rest[1:])
    elif mode == "broadcast":
        if len(args) < 2:
            print("Usage: narrater broadcast <message>", file=sys.stderr)
            sys.exit(1)
        payload["msg"] = " ".join(args[1:])
    elif mode not in ("peers", "whoami", "inbox", "notify-idle"):
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(1)

    out = request(payload)
    if out:
        print(out, end="" if out.endswith("\n") else "\n")
    if out.startswith("Error"):
        sys.exit(1)


if __name__ == "__main__":
    main()
"#;

    write_executable_script("narrater", script);
}

/// MCP stdio server exposing narrater communication as native tools for AI
/// agents (claude --mcp-config). Pure bridge to the same Unix socket.
fn write_narrater_mcp_script() {
    let script = r#"#!/usr/bin/env python3
"""narrater-mcp - MCP server exposing communication between NarraTer agents"""
import json, os, socket, sys, threading

TOOLS = [
    {
        "name": "send_message",
        "description": "Sends a message to another agent on the NarraTer canvas (fire-and-forget; delivery happens when the target agent is idle). Use it to delegate tasks, notify, or answer a received message.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "to": {"type": "string", "description": "Label of the target agent (see list_peers)"},
                "msg": {"type": "string", "description": "Message to send"},
            },
            "required": ["to", "msg"],
        },
    },
    {
        "name": "ask_agent",
        "description": "Sends a question to another agent on the NarraTer canvas and waits for its answer (blocks until the agent answers via reply_message). Use it when you need the result to continue.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "to": {"type": "string", "description": "Label of the target agent (see list_peers)"},
                "msg": {"type": "string", "description": "Question or task"},
                "timeout_secs": {"type": "integer", "description": "Maximum wait time in seconds (default 120)"},
            },
            "required": ["to", "msg"],
        },
    },
    {
        "name": "reply_message",
        "description": "Answers a question received from another NarraTer agent (messages in the format '[narrater from X #id]: ...'). The answer goes straight to whoever asked, no return connection needed. Always prefer this tool over send_message when the received message carries a #id.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "The short id of the received message (the frame's '#a3f2', with or without '#')"},
                "msg": {"type": "string", "description": "Your answer"},
            },
            "required": ["id", "msg"],
        },
    },
    {
        "name": "broadcast_message",
        "description": "Sends the same message to every agent connected to you, at once (fire-and-forget). Useful for orchestrating multiple workers.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "msg": {"type": "string", "description": "Message to send to all peers"},
            },
            "required": ["msg"],
        },
    },
    {
        "name": "check_messages",
        "description": "Pulls (and drains) the pending messages in your NarraTer queue without waiting for automatic delivery. Use it in the middle of long tasks to see if someone called you. Questions pulled this way (with #id) must be answered with reply_message.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "list_peers",
        "description": "Lists the NarraTer canvas agents you can contact (connected to you by an edge), with each one's status.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "whoami",
        "description": "Shows your identity (id and label) on the NarraTer canvas.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "canvas_list_nodes",
        "description": "Lists all nodes on the NarraTer canvas (terminals, notes, texts etc.) with id, type, label and position. Use it before creating or editing notes to discover what already exists.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "canvas_create_note",
        "description": "Creates a note on the NarraTer canvas. By default it spawns next to your terminal. Use notes to publish persistent results visible to the user. Returns the id of the created note.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "content": {"type": "string", "description": "Note content"},
                "label": {"type": "string", "description": "Optional title"},
                "x": {"type": "number", "description": "Optional X position on the canvas"},
                "y": {"type": "number", "description": "Optional Y position on the canvas"},
            },
            "required": ["content"],
        },
    },
    {
        "name": "canvas_update_note",
        "description": "Appends to or replaces the content of an existing NarraTer canvas note, identified by id or label (see canvas_list_nodes).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Note id or label"},
                "content": {"type": "string", "description": "Content"},
                "mode": {"type": "string", "enum": ["append", "replace"], "description": "append (default) or replace"},
            },
            "required": ["id", "content"],
        },
    },
    {
        "name": "canvas_read_note",
        "description": "Reads the content of a NarraTer canvas note, identified by id or label (see canvas_list_nodes). Use it to resume context persisted in notes by you or by other agents.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Note id or label"},
            },
            "required": ["id"],
        },
    },
    {
        "name": "canvas_create_text",
        "description": "Creates a lightweight text block on the NarraTer canvas (no title; good for labels and short annotations). By default it spawns next to your terminal. Returns the created id.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "Block text"},
                "x": {"type": "number", "description": "Optional X position on the canvas"},
                "y": {"type": "number", "description": "Optional Y position on the canvas"},
            },
            "required": ["text"],
        },
    },
    {
        "name": "canvas_move_node",
        "description": "Moves a NarraTer canvas node (any type, identified by id or label) to position (x, y). Use canvas_list_nodes to see the current positions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Node id or label"},
                "x": {"type": "number", "description": "New X position"},
                "y": {"type": "number", "description": "New Y position"},
            },
            "required": ["id", "x", "y"],
        },
    },
    {
        "name": "canvas_connect_nodes",
        "description": "Connects two NarraTer canvas nodes with a directed edge source -> target. terminal->terminal creates a communication route between agents (agent-pipe); terminal<->note mirrors the terminal's output into the note (agent-note).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "source": {"type": "string", "description": "Id or label of the source node"},
                "target": {"type": "string", "description": "Id or label of the target node"},
            },
            "required": ["source", "target"],
        },
    },
]

MODE = {
    "send_message": "send",
    "ask_agent": "ask",
    "reply_message": "reply",
    "broadcast_message": "broadcast",
    "check_messages": "inbox",
    "list_peers": "peers",
    "whoami": "whoami",
}


def narrater_request(payload):
    sock_path = os.environ.get("NARRATER_SOCKET", "")
    if not sock_path:
        return "Error: NARRATER_SOCKET not set"
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        s.connect(sock_path)
        s.sendall(json.dumps(payload).encode("utf-8"))
        s.shutdown(socket.SHUT_WR)
        r = b""
        while True:
            c = s.recv(4096)
            if not c:
                break
            r += c
        return r.decode("utf-8")
    except Exception as e:
        return f"Error: {e}"
    finally:
        s.close()


_stdout_lock = threading.Lock()


def _write(obj):
    with _stdout_lock:
        sys.stdout.write(json.dumps(obj) + "\n")
        sys.stdout.flush()


def reply(msg_id, result):
    _write({"jsonrpc": "2.0", "id": msg_id, "result": result})


def notify_progress(token, progress, message):
    _write({"jsonrpc": "2.0", "method": "notifications/progress",
            "params": {"progressToken": token, "progress": progress, "message": message}})


def handle_tool_call(msg_id, params):
    name = params.get("name", "")
    args = params.get("arguments", {}) or {}
    ident = {
        "from": os.environ.get("NARRATER_ID", ""),
        "token": os.environ.get("NARRATER_TOKEN", ""),
    }
    if name.startswith("canvas_"):
        payload = {
            **ident,
            "mode": "canvas",
            "action": name[len("canvas_"):],
            "params": args,
        }
    else:
        mode = MODE.get(name)
        if not mode:
            reply(msg_id, {"content": [{"type": "text", "text": f"Error: unknown tool '{name}'"}], "isError": True})
            return
        payload = {**ident, "mode": mode}
        if mode in ("send", "ask"):
            payload["to"] = args.get("to", "")
            payload["msg"] = args.get("msg", "")
            if args.get("timeout_secs"):
                payload["timeout_secs"] = int(args["timeout_secs"])
        elif mode == "reply":
            payload["msg_id"] = args.get("id", "")
            payload["msg"] = args.get("msg", "")
        elif mode == "broadcast":
            payload["msg"] = args.get("msg", "")

    # Progress during long asks, so the caller doesn't look stuck
    done = threading.Event()
    token = (params.get("_meta") or {}).get("progressToken")
    if token is not None and name == "ask_agent":
        target = args.get("to", "?")

        def ping():
            waited = 0
            while not done.wait(10):
                waited += 10
                notify_progress(token, waited, f"waiting for {target}'s answer ({waited}s)")

        threading.Thread(target=ping, daemon=True).start()

    try:
        text = narrater_request(payload).strip() or "(no answer)"
    finally:
        done.set()
    reply(msg_id, {"content": [{"type": "text", "text": text}], "isError": text.startswith("Error")})


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        method = msg.get("method", "")
        msg_id = msg.get("id")
        if method == "initialize":
            reply(msg_id, {
                "protocolVersion": msg.get("params", {}).get("protocolVersion", "2024-11-05"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "narrater", "version": "1.0.0"},
            })
        elif method == "tools/list":
            reply(msg_id, {"tools": TOOLS})
        elif method == "tools/call":
            # One thread per call: a blocking ask doesn't stall the other tools
            threading.Thread(target=handle_tool_call, args=(msg_id, msg.get("params", {})), daemon=True).start()
        elif msg_id is not None:
            _write({"jsonrpc": "2.0", "id": msg_id, "error": {"code": -32601, "message": f"method not found: {method}"}})


if __name__ == "__main__":
    main()
"#;

    write_executable_script("narrater-mcp", script);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_and_colors() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m normal"), "red normal");
        assert_eq!(strip_ansi("\x1b[2J\x1b[1;1Hclean"), "clean");
    }

    #[test]
    fn strip_ansi_removes_osc_and_carriage_return() {
        assert_eq!(strip_ansi("\x1b]0;title\x07text"), "text");
        assert_eq!(strip_ansi("\x1b]8;;http://x\x1b\\link"), "link");
        assert_eq!(strip_ansi("line\r\n"), "line\n");
    }

    #[test]
    fn strip_ansi_removes_simple_escapes_and_preserves_utf8() {
        assert_eq!(strip_ansi("\x1b=\x1b>ok"), "ok");
        assert_eq!(strip_ansi("\x1b(Bacentuação ção"), "acentuação ção");
    }

    #[test]
    fn echo_cut_at_the_marker() {
        let resp = "earlier junk\n[narrater from planner]: what's the status?\nthe actual answer";
        assert_eq!(strip_injected_echo(resp, "planner", "what's the status?"), "the actual answer");
    }

    #[test]
    fn echo_cut_at_the_message_when_no_marker() {
        // Shells echo the raw command, no frame
        let resp = "ls -la\ntotal 42\nfile.txt";
        assert_eq!(strip_injected_echo(resp, "dev", "ls -la"), "total 42\nfile.txt");
    }

    #[test]
    fn route_via_edge_valid_grant_and_expired_grant() {
        use std::collections::{HashMap, HashSet};
        let mut connections = HashSet::new();
        let mut grants: HashMap<(String, String), Instant> = HashMap::new();

        assert!(!route_allowed(&connections, &grants, "a", "b"));

        connections.insert(("a".to_string(), "b".to_string()));
        assert!(route_allowed(&connections, &grants, "a", "b"));
        // Edges are directed: b→a stays forbidden
        assert!(!route_allowed(&connections, &grants, "b", "a"));

        // A freshly granted reply route opens b→a
        grants.insert(("b".to_string(), "a".to_string()), Instant::now());
        assert!(route_allowed(&connections, &grants, "b", "a"));

        // An expired grant forbids it again
        if let Some(old) = Instant::now().checked_sub(REPLY_GRANT_TTL + Duration::from_secs(1)) {
            grants.insert(("b".to_string(), "a".to_string()), old);
            assert!(!route_allowed(&connections, &grants, "b", "a"));
        }
    }

    #[test]
    fn echo_without_match_returns_everything() {
        // Line-wrap breaks the marker (known fragility; the reply path no
        // longer depends on this heuristic)
        let resp = "[narrater from plan\nner]: question\nanswer";
        assert_eq!(strip_injected_echo(resp, "planner", "long question that wrapped"), resp);
    }
}

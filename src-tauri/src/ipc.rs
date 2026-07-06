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
    enqueue_message, AskWaiter, PtyStateInner, PtyStatusEvent, QueuedMsg, ResponseListener,
    RunStatus, MAX_QUEUE_WAIT, REPLY_GRANT_TTL,
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
    /// mode "reply": id curto do ask sendo respondido (o `#a3f2` do frame)
    #[serde(default)]
    msg_id: Option<String>,
    /// mode "canvas": ação (list_nodes, create_note, update_note…)
    #[serde(default)]
    action: Option<String>,
    /// mode "canvas": argumentos da ação, repassados ao frontend como vieram
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
            let _ = stream.write_all("Erro: formato de requisição inválido\n".as_bytes()).await;
            return;
        }
    };

    let reply = match req.mode.as_deref() {
        Some("whoami") => handle_whoami(&req, &state),
        Some("peers") => handle_peers(&req, &state),
        Some("send") => handle_send(&req, &app, &state),
        Some("ask") => handle_ask(&req, &app, &state).await,
        Some("reply") => handle_reply(&req, &state),
        Some("broadcast") => handle_broadcast(&req, &app, &state),
        Some("inbox") => handle_inbox(&req, &app, &state),
        Some("notify-idle") => handle_notify_idle(&req, &app, &state),
        Some("canvas") => handle_canvas(&req, &app, &state).await,
        _ => "Uso: narrater send|ask <alvo> <mensagem> | narrater reply <id> <resposta> | narrater broadcast <mensagem> | narrater inbox | narrater peers | narrater whoami\n".to_string(),
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
    let to = req.to.clone().ok_or("Erro: alvo não informado\n")?;
    if req.from.is_empty() {
        return Err("Erro: NARRATER_ID não definido. Você está dentro de um terminal NarraTer?\n".into());
    }

    let inner = state.lock().unwrap();
    let target_id = inner
        .label_to_id
        .get(&to)
        .cloned()
        .or_else(|| inner.sessions.contains_key(&to).then(|| to.clone()))
        .ok_or_else(|| format!("Erro: nenhum agente chamado '{}'\n", to))?;

    // Rota válida: edge do canvas OU grant temporário de resposta (receber
    // mensagem de alguém autoriza responder por REPLY_GRANT_TTL).
    let route = (req.from.clone(), target_id.clone());
    let granted = inner
        .reply_grants
        .get(&route)
        .is_some_and(|t| t.elapsed() < REPLY_GRANT_TTL);
    if !inner.connections.contains(&route) && !granted {
        let from_label = label_of(&inner, &req.from);
        return Err(format!(
            "Erro: sem conexão de '{}' para '{}' — conecte os terminais no canvas\n",
            from_label, to
        ));
    }

    let from_label = label_of(&inner, &req.from);
    Ok((target_id, from_label))
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
        "(nenhum agente conectado — crie uma edge no canvas)\n".to_string()
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
        _ => return "Erro: mensagem vazia\n".to_string(),
    };

    let position = enqueue_message(state, app, &target_id, QueuedMsg {
        from_label,
        from_id: Some(req.from.clone()),
        msg,
        enqueued: Instant::now(),
        msg_id: None,
        delivered_tx: None,
    });
    let to = req.to.clone().unwrap_or_default();
    if position == 0 {
        format!("ok: mensagem para '{}' será entregue quando o agente estiver ocioso\n", to)
    } else {
        format!("ok: mensagem para '{}' enfileirada ({} na frente)\n", to, position)
    }
}

async fn handle_ask(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let (target_id, from_label) = match resolve_route(req, state) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let msg = match req.msg.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return "Erro: mensagem vazia\n".to_string(),
    };

    // Agentes AI respondem de propósito via `reply` (texto limpo, sem eco nem
    // ANSI, asks concorrentes não se misturam); shells não têm como chamar
    // reply organicamente, então mantêm o scraping do stdout como fallback.
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

/// Envia a mesma mensagem para todos os peers (edges de saída) de uma vez —
/// padrão orquestrador → workers. Fire-and-forget como o send.
fn handle_broadcast(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    if req.from.is_empty() {
        return "Erro: NARRATER_ID não definido. Você está dentro de um terminal NarraTer?\n".into();
    }
    let msg = match req.msg.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return "Erro: mensagem vazia\n".to_string(),
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
        return "Erro: nenhum agente conectado — crie edges no canvas\n".to_string();
    }

    for (target_id, _) in &targets {
        enqueue_message(state, app, target_id, QueuedMsg {
            from_label: from_label.clone(),
            from_id: Some(req.from.clone()),
            msg: msg.clone(),
            enqueued: Instant::now(),
            msg_id: None,
            delivered_tx: None,
        });
    }
    let mut labels: Vec<String> = targets.into_iter().map(|(_, l)| l).collect();
    labels.sort();
    format!("ok: mensagem enviada para {} agente(s): {}\n", labels.len(), labels.join(", "))
}

/// Puxa (e drena) as mensagens pendentes do chamador em vez de esperar a
/// injeção idle-gated — cobre o agente que fica ocupado por muito tempo.
fn handle_inbox(req: &IpcRequest, app: &AppHandle, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let msgs: Vec<QueuedMsg> = {
        let mut inner = state.lock().unwrap();
        if req.from.is_empty() || !inner.sessions.contains_key(&req.from) {
            return "Erro: sessão desconhecida — você está dentro de um terminal NarraTer?\n".to_string();
        }
        let msgs: Vec<QueuedMsg> = inner
            .inbox
            .remove(&req.from)
            .map(|q| q.into_iter().collect())
            .unwrap_or_default();
        // Puxar a mensagem também concede a rota de resposta, como na entrega
        for m in &msgs {
            if let Some(from_id) = m.from_id.clone() {
                inner.reply_grants.insert((req.from.clone(), from_id), Instant::now());
            }
        }
        msgs
    };

    if msgs.is_empty() {
        return "(nenhuma mensagem pendente)\n".to_string();
    }
    let _ = app.emit("pty_queue", crate::pty::PtyQueueEvent { id: req.from.clone(), pending: 0 });

    let lines: Vec<String> = msgs
        .into_iter()
        .map(|mut m| {
            // Um ask pendente foi "entregue" por pull: libera o chamador para
            // esperar o reply
            if let Some(tx) = m.delivered_tx.take() {
                let _ = tx.send(());
            }
            let id_part = m.msg_id.as_deref().map(|i| format!(" #{}", i)).unwrap_or_default();
            format!("de {}{}: {}", m.from_label, id_part, m.msg)
        })
        .collect();
    lines.join("\n") + "\n"
}

/// Ask com resposta explícita: entrega `[narrater de X #id]: msg` e espera o
/// alvo resolver o id via `narrater reply` / tool reply_message.
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

    let msg_id = {
        let mut inner = state.lock().unwrap();
        let id = new_ask_id(&inner.ask_waiters);
        inner.ask_waiters.insert(id.clone(), AskWaiter {
            responder_id: target_id.clone(),
            tx,
        });
        id
    };

    enqueue_message(state, app, &target_id, QueuedMsg {
        from_label,
        from_id: Some(req.from.clone()),
        msg,
        enqueued: Instant::now(),
        msg_id: Some(msg_id.clone()),
        delivered_tx: Some(delivered_tx),
    });

    if timeout(MAX_QUEUE_WAIT + Duration::from_secs(5), delivered_rx)
        .await
        .map(|r| r.is_err())
        .unwrap_or(true)
    {
        state.lock().unwrap().ask_waiters.remove(&msg_id);
        return "Erro: timeout aguardando a entrega da mensagem ao agente\n".to_string();
    }

    let max = req.timeout_secs.map(Duration::from_secs).unwrap_or(ASK_DEFAULT_TIMEOUT);
    match timeout(max, rx).await {
        Ok(Ok(text)) => {
            if text.ends_with('\n') { text } else { format!("{}\n", text) }
        }
        Ok(Err(_)) => "Erro: o agente encerrou sem responder\n".to_string(),
        Err(_) => {
            state.lock().unwrap().ask_waiters.remove(&msg_id);
            format!(
                "Erro: o agente não respondeu (reply) em {}s — ele pode ainda estar trabalhando; tente de novo com --timeout maior\n",
                max.as_secs()
            )
        }
    }
}

/// Ids curtos de 4 hex, únicos entre os asks pendentes — legíveis no frame
/// (`#a3f2`) e fáceis de digitar num `narrater reply`.
fn new_ask_id(pending: &std::collections::HashMap<String, AskWaiter>) -> String {
    loop {
        let id = uuid::Uuid::new_v4().simple().to_string()[..4].to_string();
        if !pending.contains_key(&id) {
            return id;
        }
    }
}

/// Responde a um ask pendente: resolve o oneshot registrado pelo id curto.
/// Sem checagem de edge — o reply é o canal de volta de uma pergunta que já
/// chegou até você; só o terminal que recebeu o ask pode respondê-lo.
fn handle_reply(req: &IpcRequest, state: &Arc<Mutex<PtyStateInner>>) -> String {
    let msg_id = match req.msg_id.as_deref() {
        Some(i) if !i.is_empty() => i.trim_start_matches('#').to_string(),
        _ => return "Erro: informe o id da mensagem (o #id do frame recebido)\n".to_string(),
    };
    let text = match req.msg.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => return "Erro: resposta vazia\n".to_string(),
    };

    let mut inner = state.lock().unwrap();
    if req.from.is_empty() || !inner.sessions.contains_key(&req.from) {
        return "Erro: sessão desconhecida — você está dentro de um terminal NarraTer?\n".to_string();
    }
    match inner.ask_waiters.get(&msg_id) {
        None => format!(
            "Erro: nenhuma pergunta pendente com id '{}' — ela expirou, já foi respondida, ou o id está errado\n",
            msg_id
        ),
        Some(w) if w.responder_id != req.from => {
            "Erro: essa pergunta não foi direcionada a você\n".to_string()
        }
        Some(_) => {
            let waiter = inner.ask_waiters.remove(&msg_id).unwrap();
            let _ = waiter.tx.send(text);
            "ok: resposta entregue\n".to_string()
        }
    }
}

/// Hook Stop do claude (`narrater notify-idle`): sinal autoritativo de fim de
/// turno. Marca a sessão Idle (a fila drena no próximo tick) e registra que
/// esta sessão tem hook — o timer de silêncio deixa de valer para ela.
fn handle_notify_idle(
    req: &IpcRequest,
    app: &AppHandle,
    state: &Arc<Mutex<PtyStateInner>>,
) -> String {
    {
        let mut inner = state.lock().unwrap();
        let Some(session) = inner.sessions.get_mut(&req.from) else {
            return "Erro: sessão desconhecida — você está dentro de um terminal NarraTer?\n".to_string();
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

/// Fallback para alvos shell: captura o stdout até o alvo assentar em Idle.
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
        return "Erro: timeout aguardando a entrega da mensagem ao agente\n".to_string();
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

/// Remove sequências de escape ANSI (CSI, OSC e escapes de 2 bytes) e
/// retornos de carro, deixando só o texto — o output cru de PTY vem cheio de
/// cores, redraws e movimentação de cursor.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: ESC [ ... byte final em 0x40-0x7E
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] ... terminado por BEL ou ST (ESC \)
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
                // Designação de charset: ESC ( X / ESC ) X
                Some('(') | Some(')') => {
                    chars.next();
                }
                // Escapes de 2 bytes (ESC =, ESC >, ESC 7, …): já consumido
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

/// Ponte agente → canvas: registra um waiter, emite `canvas_request` para o
/// frontend (que aplica a ação no store e responde via canvas_respond) e
/// espera o resultado. ACL v1: qualquer agente com sessão válida pode
/// manipular o canvas — as edges seguem governando só a comunicação
/// agente↔agente (ver docs/mcp-canvas-tools.md).
async fn handle_canvas(
    req: &IpcRequest,
    app: &AppHandle,
    state: &Arc<Mutex<PtyStateInner>>,
) -> String {
    let action = match req.action.as_deref() {
        Some(a) if !a.is_empty() => a.to_string(),
        _ => return "Erro: ação de canvas não informada\n".to_string(),
    };
    if req.from.is_empty() {
        return "Erro: NARRATER_ID não definido. Você está dentro de um terminal NarraTer?\n".into();
    }

    let req_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<String>();
    let from_label = {
        let mut inner = state.lock().unwrap();
        if !inner.sessions.contains_key(&req.from) {
            return "Erro: sessão desconhecida — você está dentro de um terminal NarraTer?\n".to_string();
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
            "Erro: timeout aguardando o canvas\n".to_string()
        }
    }
}

/// The target PTY echoes the injected line back — `[narrater de <label>]: ...`
/// for AI agents, the bare command for shells. Cut everything up to and
/// including that echo so the caller sees only the actual reply.
fn strip_injected_echo(response: &str, from_label: &str, msg: &str) -> String {
    let marker = format!("[narrater de {}]", from_label);
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

/// Settings extras passadas ao claude via `--settings` no spawn (pty.rs):
/// hook Stop → `narrater notify-idle`, o sinal autoritativo de idle.
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
"""narrater - comunicacao entre agentes NarraTer

Uso:
  narrater send <alvo> <mensagem>              envia e retorna imediatamente
  narrater ask <alvo> <mensagem> [--timeout N] envia e espera a resposta
  narrater reply <id> <resposta>               responde a um ask recebido (o #id do frame)
  narrater broadcast <mensagem>                envia para todos os peers de uma vez
  narrater inbox                               puxa (e drena) suas mensagens pendentes
  narrater peers                               lista agentes alcancaveis
  narrater whoami                              mostra sua identidade
"""
import socket, json, sys, os


def request(payload):
    sock = os.environ.get("NARRATER_SOCKET", "")
    if not sock:
        print("Erro: NARRATER_SOCKET nao definido. Voce esta dentro de um terminal NarraTer?", file=sys.stderr)
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
        print(f"narrater erro: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        s.close()


def main():
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help"):
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(0 if args else 1)

    mode = args[0]
    payload = {"from": os.environ.get("NARRATER_ID", ""), "mode": mode}

    if mode in ("send", "ask"):
        rest = args[1:]
        if "--timeout" in rest:
            i = rest.index("--timeout")
            try:
                payload["timeout_secs"] = int(rest[i + 1])
            except (IndexError, ValueError):
                print("Erro: --timeout requer um numero de segundos", file=sys.stderr)
                sys.exit(1)
            del rest[i:i + 2]
        if len(rest) < 2:
            print(f"Uso: narrater {mode} <alvo> <mensagem>", file=sys.stderr)
            sys.exit(1)
        payload["to"] = rest[0]
        payload["msg"] = " ".join(rest[1:])
    elif mode == "reply":
        rest = args[1:]
        if len(rest) < 2:
            print("Uso: narrater reply <id> <resposta>", file=sys.stderr)
            sys.exit(1)
        payload["msg_id"] = rest[0]
        payload["msg"] = " ".join(rest[1:])
    elif mode == "broadcast":
        if len(args) < 2:
            print("Uso: narrater broadcast <mensagem>", file=sys.stderr)
            sys.exit(1)
        payload["msg"] = " ".join(args[1:])
    elif mode not in ("peers", "whoami", "inbox", "notify-idle"):
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(1)

    out = request(payload)
    if out:
        print(out, end="" if out.endswith("\n") else "\n")
    if out.startswith("Erro"):
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
"""narrater-mcp - servidor MCP que expoe a comunicacao entre agentes NarraTer"""
import json, os, socket, sys, threading

TOOLS = [
    {
        "name": "send_message",
        "description": "Envia uma mensagem para outro agente do canvas NarraTer (fire-and-forget; a entrega ocorre quando o agente alvo estiver ocioso). Use para delegar tarefas, notificar ou responder a uma mensagem recebida.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "to": {"type": "string", "description": "Label do agente alvo (veja list_peers)"},
                "msg": {"type": "string", "description": "Mensagem a enviar"},
            },
            "required": ["to", "msg"],
        },
    },
    {
        "name": "ask_agent",
        "description": "Envia uma pergunta a outro agente do canvas NarraTer e espera a resposta dele (bloqueia ate o agente responder via reply_message). Use quando precisar do resultado para continuar.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "to": {"type": "string", "description": "Label do agente alvo (veja list_peers)"},
                "msg": {"type": "string", "description": "Pergunta ou tarefa"},
                "timeout_secs": {"type": "integer", "description": "Tempo maximo de espera em segundos (default 120)"},
            },
            "required": ["to", "msg"],
        },
    },
    {
        "name": "reply_message",
        "description": "Responde a uma pergunta recebida de outro agente NarraTer (mensagens no formato '[narrater de X #id]: ...'). A resposta chega direto a quem perguntou, sem precisar de conexao de volta. Sempre prefira esta tool a send_message quando a mensagem recebida tiver #id.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "O id curto da mensagem recebida (o '#a3f2' do frame, com ou sem '#')"},
                "msg": {"type": "string", "description": "Sua resposta"},
            },
            "required": ["id", "msg"],
        },
    },
    {
        "name": "broadcast_message",
        "description": "Envia a mesma mensagem para todos os agentes conectados a voce, de uma vez (fire-and-forget). Util para orquestrar varios workers.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "msg": {"type": "string", "description": "Mensagem a enviar a todos os peers"},
            },
            "required": ["msg"],
        },
    },
    {
        "name": "check_messages",
        "description": "Puxa (e drena) as mensagens pendentes na sua fila do NarraTer sem esperar a entrega automatica. Use no meio de tarefas longas para ver se alguem te chamou. Perguntas puxadas assim (com #id) devem ser respondidas com reply_message.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "list_peers",
        "description": "Lista os agentes do canvas NarraTer que voce pode contatar (conectados a voce por uma edge), com o status de cada um.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "whoami",
        "description": "Mostra sua identidade (id e label) no canvas NarraTer.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "canvas_list_nodes",
        "description": "Lista todos os nos do canvas NarraTer (terminais, notas, textos etc.) com id, tipo, label e posicao. Use antes de criar ou editar notas para descobrir o que ja existe.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "canvas_create_note",
        "description": "Cria uma nota no canvas NarraTer. Por padrao nasce ao lado do seu terminal. Use notas para publicar resultados persistentes visiveis ao usuario. Retorna o id da nota criada.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "content": {"type": "string", "description": "Conteudo da nota"},
                "label": {"type": "string", "description": "Titulo opcional"},
                "x": {"type": "number", "description": "Posicao X opcional no canvas"},
                "y": {"type": "number", "description": "Posicao Y opcional no canvas"},
            },
            "required": ["content"],
        },
    },
    {
        "name": "canvas_update_note",
        "description": "Anexa ou substitui o conteudo de uma nota existente do canvas NarraTer, identificada por id ou label (veja canvas_list_nodes).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Id ou label da nota"},
                "content": {"type": "string", "description": "Conteudo"},
                "mode": {"type": "string", "enum": ["append", "replace"], "description": "append (default) ou replace"},
            },
            "required": ["id", "content"],
        },
    },
    {
        "name": "canvas_read_note",
        "description": "Le o conteudo de uma nota do canvas NarraTer, identificada por id ou label (veja canvas_list_nodes). Use para retomar contexto persistido em notas por voce ou por outros agentes.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Id ou label da nota"},
            },
            "required": ["id"],
        },
    },
    {
        "name": "canvas_create_text",
        "description": "Cria um bloco de texto leve no canvas NarraTer (sem titulo; bom para rotulos e anotacoes curtas). Por padrao nasce ao lado do seu terminal. Retorna o id criado.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "Texto do bloco"},
                "x": {"type": "number", "description": "Posicao X opcional no canvas"},
                "y": {"type": "number", "description": "Posicao Y opcional no canvas"},
            },
            "required": ["text"],
        },
    },
    {
        "name": "canvas_move_node",
        "description": "Move um no do canvas NarraTer (qualquer tipo, identificado por id ou label) para a posicao (x, y). Use canvas_list_nodes para ver as posicoes atuais.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "description": "Id ou label do no"},
                "x": {"type": "number", "description": "Nova posicao X"},
                "y": {"type": "number", "description": "Nova posicao Y"},
            },
            "required": ["id", "x", "y"],
        },
    },
    {
        "name": "canvas_connect_nodes",
        "description": "Conecta dois nos do canvas NarraTer com uma edge direcionada source -> target. terminal->terminal cria rota de comunicacao entre agentes (agent-pipe); terminal<->nota espelha o output do terminal na nota (agent-note).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "source": {"type": "string", "description": "Id ou label do no de origem"},
                "target": {"type": "string", "description": "Id ou label do no de destino"},
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
        return "Erro: NARRATER_SOCKET nao definido"
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
        return f"Erro: {e}"
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
    if name.startswith("canvas_"):
        payload = {
            "from": os.environ.get("NARRATER_ID", ""),
            "mode": "canvas",
            "action": name[len("canvas_"):],
            "params": args,
        }
    else:
        mode = MODE.get(name)
        if not mode:
            reply(msg_id, {"content": [{"type": "text", "text": f"Erro: ferramenta desconhecida '{name}'"}], "isError": True})
            return
        payload = {"from": os.environ.get("NARRATER_ID", ""), "mode": mode}
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

    # Progress durante asks longos, para o chamador nao parecer travado
    done = threading.Event()
    token = (params.get("_meta") or {}).get("progressToken")
    if token is not None and name == "ask_agent":
        target = args.get("to", "?")

        def ping():
            waited = 0
            while not done.wait(10):
                waited += 10
                notify_progress(token, waited, f"aguardando resposta de {target} ({waited}s)")

        threading.Thread(target=ping, daemon=True).start()

    try:
        text = narrater_request(payload).strip() or "(sem resposta)"
    finally:
        done.set()
    reply(msg_id, {"content": [{"type": "text", "text": text}], "isError": text.startswith("Erro")})


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
            # Thread por chamada: um ask bloqueante nao trava as demais tools
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
    fn strip_ansi_remove_csi_e_cores() {
        assert_eq!(strip_ansi("\x1b[31mvermelho\x1b[0m normal"), "vermelho normal");
        assert_eq!(strip_ansi("\x1b[2J\x1b[1;1Hlimpo"), "limpo");
    }

    #[test]
    fn strip_ansi_remove_osc_e_carriage_return() {
        assert_eq!(strip_ansi("\x1b]0;titulo\x07texto"), "texto");
        assert_eq!(strip_ansi("\x1b]8;;http://x\x1b\\link"), "link");
        assert_eq!(strip_ansi("linha\r\n"), "linha\n");
    }

    #[test]
    fn strip_ansi_remove_escapes_simples_e_preserva_utf8() {
        assert_eq!(strip_ansi("\x1b=\x1b>ok"), "ok");
        assert_eq!(strip_ansi("\x1b(Bacentuação ção"), "acentuação ção");
    }

    #[test]
    fn echo_cortado_pelo_marcador() {
        let resp = "lixo anterior\n[narrater de planner]: qual o status?\na resposta real";
        assert_eq!(strip_injected_echo(resp, "planner", "qual o status?"), "a resposta real");
    }

    #[test]
    fn echo_cortado_pela_mensagem_quando_sem_marcador() {
        // Shells ecoam o comando cru, sem frame
        let resp = "ls -la\ntotal 42\narquivo.txt";
        assert_eq!(strip_injected_echo(resp, "dev", "ls -la"), "total 42\narquivo.txt");
    }

    #[test]
    fn echo_sem_match_retorna_tudo() {
        // Line-wrap quebra o marcador (fragilidade conhecida — ver PLANO_MCP.md;
        // o caminho reply da Fase 1 não depende mais deste heurístico)
        let resp = "[narrater de plan\nner]: pergunta\nresposta";
        assert_eq!(strip_injected_echo(resp, "planner", "pergunta longa que quebrou"), resp);
    }
}

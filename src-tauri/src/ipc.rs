use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::Deserialize;
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use crate::pty::{
    enqueue_message, PtyStateInner, QueuedMsg, ResponseListener, RunStatus, MAX_QUEUE_WAIT,
};

const ASK_DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

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
    /// "send" | "ask" | "peers" | "whoami"
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

pub async fn start_ipc_server(app: AppHandle, state: Arc<Mutex<PtyStateInner>>) {
    write_narrater_script();
    write_narrater_mcp_script();

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
        _ => "Uso: narrater send|ask <alvo> <mensagem> | narrater peers | narrater whoami\n".to_string(),
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

    if !inner.connections.contains(&(req.from.clone(), target_id.clone())) {
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
        msg,
        enqueued: Instant::now(),
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
        msg: msg.clone(),
        enqueued: Instant::now(),
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
    strip_injected_echo(&response, &from_label, &msg)
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

fn write_narrater_script() {

    let script = r#"#!/usr/bin/env python3
"""narrater - comunicacao entre agentes NarraTer

Uso:
  narrater send <alvo> <mensagem>              envia e retorna imediatamente
  narrater ask <alvo> <mensagem> [--timeout N] envia e espera a resposta
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
    elif mode not in ("peers", "whoami"):
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
import json, os, socket, sys

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
        "description": "Envia uma pergunta a outro agente do canvas NarraTer e espera a resposta dele (bloqueia ate o agente terminar). Use quando precisar do resultado para continuar.",
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
        "name": "list_peers",
        "description": "Lista os agentes do canvas NarraTer que voce pode contatar (conectados a voce por uma edge), com o status de cada um.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "whoami",
        "description": "Mostra sua identidade (id e label) no canvas NarraTer.",
        "inputSchema": {"type": "object", "properties": {}},
    },
]

MODE = {"send_message": "send", "ask_agent": "ask", "list_peers": "peers", "whoami": "whoami"}


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


def reply(msg_id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg_id, "result": result}) + "\n")
    sys.stdout.flush()


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
            params = msg.get("params", {})
            name = params.get("name", "")
            args = params.get("arguments", {}) or {}
            mode = MODE.get(name)
            if not mode:
                reply(msg_id, {"content": [{"type": "text", "text": f"Erro: ferramenta desconhecida '{name}'"}], "isError": True})
                continue
            payload = {"from": os.environ.get("NARRATER_ID", ""), "mode": mode}
            if mode in ("send", "ask"):
                payload["to"] = args.get("to", "")
                payload["msg"] = args.get("msg", "")
                if args.get("timeout_secs"):
                    payload["timeout_secs"] = int(args["timeout_secs"])
            text = narrater_request(payload).strip() or "(sem resposta)"
            reply(msg_id, {"content": [{"type": "text", "text": text}], "isError": text.startswith("Erro")})
        elif msg_id is not None:
            sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg_id, "error": {"code": -32601, "message": f"method not found: {method}"}}) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
"#;

    write_executable_script("narrater-mcp", script);
}

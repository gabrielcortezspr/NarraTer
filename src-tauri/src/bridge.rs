//! The bridge: how the `narrater` CLI of remote workspaces (SSH servers,
//! Docker containers) reaches the app — Maestri's "Bridge Port". A tiny HTTP
//! endpoint on 127.0.0.1 (the SSH reverse tunnel ends here) and, for Docker,
//! on the container network's gateway. The remote wrapper posts its raw
//! command line; it's parsed by `cli_core` exactly like the local CLI and run
//! through the same dispatch. Every request must carry a live terminal's
//! token, like the Unix socket.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::cli_core::{self, Outcome};
use crate::ipc::{dispatch, IpcCtx};

/// Where it listens first (Maestri's default); taken → any free port.
pub const DEFAULT_PORT: u16 = 7433;
const MAX_BODY: usize = 64 * 1024 * 1024;

static PORT: OnceLock<u16> = OnceLock::new();
static CTX: OnceLock<IpcCtx> = OnceLock::new();
/// Extra addresses (Docker gateways) already listening.
static EXTRA: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// The bridge's port, once it's listening.
pub fn port() -> Option<u16> {
    PORT.get().copied()
}

pub async fn serve(ctx: IpcCtx) {
    let _ = CTX.set(ctx);
    let listener = match TcpListener::bind(("127.0.0.1", DEFAULT_PORT)).await {
        Ok(l) => l,
        Err(_) => match TcpListener::bind(("127.0.0.1", 0)).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[NarraTer bridge] cannot listen: {}", e);
                return;
            }
        },
    };
    let Ok(addr) = listener.local_addr() else { return };
    let _ = PORT.set(addr.port());
    accept_loop(listener).await;
}

async fn accept_loop(listener: TcpListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle(stream));
            }
            Err(e) => eprintln!("[NarraTer bridge] accept error: {}", e),
        }
    }
}

/// Also listens on `ip` (a Docker network's gateway on this host), on the
/// bridge's port. Idempotent.
pub async fn ensure_listener(ip: &str) -> Result<(), String> {
    let port = port().ok_or("the bridge isn't running")?;
    {
        let mut extra = EXTRA.lock().unwrap();
        let set = extra.get_or_insert_with(HashSet::new);
        if set.contains(ip) {
            return Ok(());
        }
        set.insert(ip.to_string());
    }
    match TcpListener::bind((ip, port)).await {
        Ok(l) => {
            tokio::spawn(accept_loop(l));
            Ok(())
        }
        Err(e) => {
            EXTRA.lock().unwrap().get_or_insert_with(HashSet::new).remove(ip);
            Err(format!("the bridge can't listen on {}:{} for the container: {}", ip, port, e))
        }
    }
}

struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

async fn read_request(stream: &mut TcpStream) -> Option<HttpRequest> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let head_end = loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > 64 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let (method, path) = (first.next()?.to_string(), first.next()?.to_string());
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        return None;
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(len);
    Some(HttpRequest { method, path, headers, body })
}

async fn respond(stream: &mut TcpStream, code: u16, body: &str) {
    let reason = match code {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        code,
        reason,
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn handle(mut stream: TcpStream) {
    let Some(req) = read_request(&mut stream).await else { return };
    let Some(ctx) = CTX.get() else { return };
    let (code, body) = match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/ping") => (200, "ok\n".to_string()),
        ("POST", "/cli") => cli(ctx, &req).await,
        _ => (404, "not found\n".to_string()),
    };
    respond(&mut stream, code, &body).await;
}

fn nl(s: String) -> String {
    if s.ends_with('\n') { s } else { s + "\n" }
}

/// Splits `argc` NUL-terminated arguments off the body; the rest is stdin.
fn split_body(body: &[u8], argc: usize) -> Option<(Vec<String>, Vec<u8>)> {
    let mut args = Vec::with_capacity(argc);
    let mut rest = body;
    for _ in 0..argc {
        let end = rest.iter().position(|b| *b == 0)?;
        args.push(String::from_utf8_lossy(&rest[..end]).into_owned());
        rest = &rest[end + 1..];
    }
    Some((args, rest.to_vec()))
}

/// The caller is a live terminal with that token.
fn authentic(ctx: &IpcCtx, id: &str, token: &str) -> bool {
    !id.is_empty() && ctx.state.lock().unwrap().sessions.get(id).is_some_and(|s| s.token == token)
}

/// HTTP status: 200 print + exit 0; 201 print + exit 1 (an "Error…"
/// answer); 400 to stderr + exit 1 (usage).
async fn cli(ctx: &IpcCtx, req: &HttpRequest) -> (u16, String) {
    let id = req.header("X-Narrater-Id").unwrap_or_default().to_string();
    let token = req.header("X-Narrater-Token").unwrap_or_default().to_string();
    let home = req.header("X-Narrater-Home").unwrap_or_default().to_string();
    let argc: usize = req.header("X-Narrater-Argc").and_then(|v| v.parse().ok()).unwrap_or(0);
    let Some((argv, stdin_bytes)) = split_body(&req.body, argc) else {
        return (400, "Error: malformed request from the narrater wrapper\n".into());
    };
    let piped = req.header("X-Narrater-Stdin") == Some("1");
    // Parsed in its own scope: the environment borrows don't cross awaits
    let parsed = {
        let var = |k: &str| match k {
            "NARRATER_ID" => Some(id.clone()),
            "NARRATER_TOKEN" => Some(token.clone()),
            "HOME" => Some(home.clone()),
            _ => None,
        };
        let mut stdin = || piped.then(|| String::from_utf8_lossy(&stdin_bytes).into_owned());
        let mut env = cli_core::Env {
            var: &var,
            cwd: req.header("X-Narrater-Cwd").unwrap_or("/").into(),
            stdin: &mut stdin,
        };
        cli_core::parse(&argv, &mut env)
    };
    match parsed {
        Ok(Outcome::Print(text)) => (200, nl(text)),
        Err(e) => (400, nl(e)),
        Ok(Outcome::Debug) => (200, debug(ctx, &id, &token)),
        Ok(Outcome::Request(mut payload)) => {
            if !authentic(ctx, &id, &token) {
                // Same wording as the socket's check
                let bytes = payload.to_string().into_bytes();
                return (201, nl(dispatch(ctx, &bytes).await));
            }
            if let Err(e) = fetch_attachments(ctx, &id, &mut payload).await {
                return (201, nl(format!("Error: {}", e)));
            }
            let reply = dispatch(ctx, payload.to_string().as_bytes()).await;
            (if reply.starts_with("Error") { 201 } else { 200 }, reply)
        }
    }
}

/// `say --file/--attach` paths name files in the remote workspace: they're
/// copied here first.
async fn fetch_attachments(ctx: &IpcCtx, id: &str, payload: &mut serde_json::Value) -> Result<(), String> {
    if payload["mode"] != "chat" {
        return Ok(());
    }
    let scene = ctx.state.lock().unwrap().names.scene_of(id);
    let Some(env) = crate::workspaces::environment_of(&scene) else { return Ok(()) };
    let params = &mut payload["params"];
    let mut remote: Vec<String> = Vec::new();
    if let Some(f) = params["file"].as_str() {
        remote.push(f.to_string());
    }
    if let Some(list) = params["attach"].as_array() {
        remote.extend(list.iter().filter_map(|v| v.as_str().map(str::to_string)));
    }
    if remote.is_empty() {
        return Ok(());
    }
    let local = tauri::async_runtime::spawn_blocking(move || {
        remote.iter().map(|p| crate::environments::fetch(&env, p).map(|l| l.display().to_string())).collect::<Result<Vec<_>, _>>()
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut it = local.into_iter();
    if params["file"].is_string() {
        params["file"] = it.next().unwrap_or_default().into();
    }
    if params["attach"].is_array() {
        params["attach"] = it.collect::<Vec<_>>().into();
    }
    Ok(())
}

fn debug(ctx: &IpcCtx, id: &str, token: &str) -> String {
    let mut out = String::from("=== NarraTer CLI Diagnostics (remote) ===\n\n");
    out.push_str(&format!("{:<16} reachable (port {})\n", "Bridge", port().unwrap_or(0)));
    if authentic(ctx, id, token) {
        let inner = ctx.state.lock().unwrap();
        out.push_str(&format!(
            "{:<16} id: {} · name: {} · workspace: {}\n",
            "Identity",
            id,
            inner.label_of(id),
            inner.names.scene_of(id)
        ));
    } else {
        out.push_str(&format!(
            "{:<16} {} — restart this terminal from the canvas\n",
            "Identity",
            if id.is_empty() { "NARRATER_ID not set" } else { "unknown terminal or stale token" }
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_splits_args_then_stdin() {
        let body = b"say\0--progress\0\0hello\nworld";
        let (args, stdin) = split_body(body, 3).unwrap();
        assert_eq!(args, ["say", "--progress", ""]);
        assert_eq!(stdin, b"hello\nworld");
        assert!(split_body(b"a\0b", 2).is_none());
        assert_eq!(split_body(b"", 0).unwrap(), (vec![], vec![]));
    }
}

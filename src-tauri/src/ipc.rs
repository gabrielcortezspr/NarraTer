use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::os::unix::fs::PermissionsExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio::time::timeout;
use serde::Deserialize;

use crate::pty::{PtyStateInner, write_to_pty};

#[derive(Deserialize)]
struct IpcRequest {
    to: String,
    msg: String,
}

pub async fn start_ipc_server(state: Arc<Mutex<PtyStateInner>>) {
    write_narrater_script();

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
                tokio::spawn(handle_connection(stream, state_clone));
            }
            Err(e) => {
                eprintln!("[NarraTer IPC] Accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(
    mut stream: tokio::net::UnixStream,
    state: Arc<Mutex<PtyStateInner>>,
) {
    let mut buf = Vec::new();
    if stream.read_to_end(&mut buf).await.is_err() {
        return;
    }

    let req: IpcRequest = match serde_json::from_slice(&buf) {
        Ok(r) => r,
        Err(_) => {
            let _ = stream.write_all(b"Error: invalid request format").await;
            return;
        }
    };

    let target_id = {
        let inner = state.lock().unwrap();
        inner.label_to_id.get(&req.to).cloned()
    };

    let target_id = match target_id {
        Some(id) => id,
        None => {
            let msg = format!("Error: no connected agent named '{}'\n", req.to);
            let _ = stream.write_all(msg.as_bytes()).await;
            return;
        }
    };

    let (tx, mut rx) = mpsc::channel::<String>(256);
    {
        let mut inner = state.lock().unwrap();
        inner.response_listeners.insert(target_id.clone(), tx);
    }

    let message = format!("{}\n", req.msg);
    if !write_to_pty(&state, &target_id, &message) {
        let _ = stream.write_all(b"Error: failed to write to target agent\n").await;
        state.lock().unwrap().response_listeners.remove(&target_id);
        return;
    }

    // Collect response with idle detection: stop after 3s of silence
    let mut response = String::new();
    let idle = Duration::from_secs(3);

    loop {
        match timeout(idle, rx.recv()).await {
            Ok(Some(data)) => response.push_str(&data),
            Ok(None) | Err(_) => break,
        }
    }

    state.lock().unwrap().response_listeners.remove(&target_id);

    let _ = stream.write_all(response.as_bytes()).await;
}

fn write_narrater_script() {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };

    let bin_dir = std::path::PathBuf::from(&home).join(".local").join("bin");
    if let Err(e) = std::fs::create_dir_all(&bin_dir) {
        eprintln!("[NarraTer IPC] Could not create ~/.local/bin: {}", e);
        return;
    }

    let script_path = bin_dir.join("narrater");

    let script = r#"#!/usr/bin/env python3
"""narrater - NarraTer agent communication skill

Usage: narrater send <agent_name> <message>
"""
import socket, json, sys, os

def main():
    if len(sys.argv) < 4 or sys.argv[1] != "send":
        print("Usage: narrater send <agent_name> <message>", file=sys.stderr)
        sys.exit(1)
    sock = os.environ.get("NARRATER_SOCKET", "")
    if not sock:
        print("Error: NARRATER_SOCKET not set. Are you inside a NarraTer terminal?", file=sys.stderr)
        sys.exit(1)
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        s.connect(sock)
        s.sendall(json.dumps({"to": sys.argv[2], "msg": sys.argv[3]}).encode("utf-8"))
        s.shutdown(socket.SHUT_WR)
        r = b""
        while True:
            c = s.recv(4096)
            if not c:
                break
            r += c
        print(r.decode("utf-8"), end="")
    except Exception as e:
        print(f"narrater error: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        s.close()

if __name__ == "__main__":
    main()
"#;

    if let Err(e) = std::fs::write(&script_path, script) {
        eprintln!("[NarraTer IPC] Failed to write narrater script: {}", e);
        return;
    }

    if let Ok(meta) = std::fs::metadata(&script_path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(&script_path, perms);
    }
}

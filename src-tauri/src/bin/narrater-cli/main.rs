//! `narrater` — the CLI every agent on the canvas reaches through its shell
//! tool (Maestri's `maestri` binary). A thin client: the command line is
//! parsed by `cli_core` (shared with the app's bridge, which serves remote
//! workspaces), sent as one JSON request over the app's Unix socket
//! ($NARRATER_SOCKET), and the answer printed. All the logic lives in the app.
//!
//! The app copies this binary to ~/.local/bin/narrater at startup and points
//! $NARRATER_CLI at it. Only std + serde_json, so it starts in milliseconds.

#[path = "../../cli_core.rs"]
mod cli_core;

use std::io::{IsTerminal, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::process::exit;
use std::time::Duration;

use cli_core::Outcome;
use serde_json::{json, Map, Value};

const ENV_KEYS: &[&str] = &["NARRATER_SOCKET", "NARRATER_ID", "NARRATER_LABEL", "NARRATER_CLI"];

fn fail(msg: &str) -> ! {
    eprintln!("{}", msg);
    exit(1)
}

fn request_at(sock: &str, payload: &Value) -> String {
    let fail_reach = |e: std::io::Error| -> ! {
        fail(&format!("Error: cannot reach NarraTer at {} ({}). Run `narrater debug`.", sock, e))
    };
    let mut s = UnixStream::connect(sock).unwrap_or_else(|e| fail_reach(e));
    s.write_all(payload.to_string().as_bytes()).unwrap_or_else(|e| fail_reach(e));
    s.shutdown(std::net::Shutdown::Write).unwrap_or_else(|e| fail_reach(e));
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap_or_else(|e| fail_reach(e));
    String::from_utf8_lossy(&buf).into_owned()
}

/// Inside a persistent session (tmux) the shell outlives the app run that
/// started it: the current socket and token are published in tmux's global
/// environment under this terminal's key — prefer them.
fn published(name: &str) -> Option<String> {
    std::env::var_os("TMUX")?;
    let key = std::env::var("NARRATER_KEY").ok().filter(|k| !k.is_empty())?;
    let var = format!("{}_{}", name, key);
    let out = std::process::Command::new("tmux").args(["show-environment", "-g", &var]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.trim().strip_prefix(&format!("{}=", var)).map(str::to_string).filter(|v| !v.is_empty())
}

fn var(name: &str) -> Option<String> {
    match name {
        "NARRATER_SOCKET" | "NARRATER_TOKEN" => published(name).or_else(|| std::env::var(name).ok()),
        _ => std::env::var(name).ok(),
    }
}

fn request(payload: &Value) -> String {
    let sock = var("NARRATER_SOCKET").unwrap_or_default();
    if sock.is_empty() {
        fail("Error: NARRATER_SOCKET not set. Run this from a NarraTer terminal (`narrater debug` to diagnose).");
    }
    request_at(&sock, payload)
}

fn base(mode: &str) -> Map<String, Value> {
    let env = |k: &str| Value::String(var(k).unwrap_or_default());
    let mut m = Map::new();
    m.insert("from".into(), env("NARRATER_ID"));
    m.insert("token".into(), env("NARRATER_TOKEN"));
    m.insert("mode".into(), mode.into());
    m
}

/// Prints the answer; an answer starting with "Error" exits 1.
fn emit(out: &str) {
    if !out.is_empty() {
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(out.as_bytes());
        if !out.ends_with('\n') {
            let _ = stdout.write_all(b"\n");
        }
        let _ = stdout.flush();
    }
    if out.starts_with("Error") {
        exit(1);
    }
}

fn cmd_debug() {
    println!("=== NarraTer CLI Diagnostics ===\n");
    println!("--- Environment ---");
    for k in ENV_KEYS {
        println!("{:<16} {}", k, var(k).unwrap_or_else(|| "(not set)".into()));
    }
    if published("NARRATER_SOCKET").is_some() {
        println!("{:<16} persistent (tmux) — socket and token from the session's published values", "Session");
    }
    let token = if var("NARRATER_TOKEN").is_some_and(|t| !t.is_empty()) { "(set)" } else { "(not set)" };
    println!("{:<16} {}", "NARRATER_TOKEN", token);
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "(unknown)".into());
    println!("{:<16} {} (v{})", "CLI", exe, env!("CARGO_PKG_VERSION"));
    let sock = var("NARRATER_SOCKET").unwrap_or_default();
    println!("\n--- Transport probe ---");
    if sock.is_empty() {
        println!("Socket:          (no NARRATER_SOCKET — not inside a NarraTer terminal?)");
        return;
    }
    let Ok(meta) = std::fs::metadata(&sock) else {
        println!("Socket exists:   no");
        println!("Hint:            stale NARRATER_SOCKET (NarraTer restarted?) — restart this terminal from the canvas.");
        return;
    };
    println!("Socket exists:   yes");
    println!("Socket mode:     {:o}", meta.permissions().mode() & 0o777);
    println!("Socket owner:    {} (you are {})", meta.uid(), unsafe { libc::getuid() });
    let ping = (|| -> std::io::Result<String> {
        let mut s = UnixStream::connect(&sock)?;
        s.set_read_timeout(Some(Duration::from_secs(5)))?;
        s.set_write_timeout(Some(Duration::from_secs(5)))?;
        s.write_all(json!({ "mode": "ping" }).to_string().as_bytes())?;
        s.shutdown(std::net::Shutdown::Write)?;
        let mut buf = String::new();
        s.read_to_string(&mut buf)?;
        Ok(buf)
    })();
    match ping {
        Ok(answer) if answer.trim() == "ok" => println!("Ping:            OK"),
        Ok(_) => println!("Ping:            unexpected answer"),
        Err(e) => println!("Ping:            FAILED ({})", e),
    }
    if var("NARRATER_ID").is_some_and(|id| !id.is_empty()) {
        let out = request(&Value::Object(base("whoami")));
        println!("Identity:        {}", out.trim().replace('\n', " · "));
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut stdin = || {
        let mut text = String::new();
        (!std::io::stdin().is_terminal()).then(|| std::io::stdin().read_to_string(&mut text).ok()).flatten()?;
        Some(text)
    };
    let mut env = cli_core::Env {
        var: &var,
        cwd: std::env::current_dir().unwrap_or_default(),
        stdin: &mut stdin,
    };
    match cli_core::parse(&argv, &mut env) {
        Ok(Outcome::Print(text)) => println!("{}", text),
        Ok(Outcome::Request(payload)) => emit(&request(&payload)),
        Ok(Outcome::Debug) => cmd_debug(),
        Err(msg) => fail(&msg),
    }
}

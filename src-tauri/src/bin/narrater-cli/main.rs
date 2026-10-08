//! `narrater` — the CLI every agent on the canvas reaches through its shell
//! tool (Maestri's `maestri` binary). A thin client: it parses the command
//! line, sends one JSON request over the app's Unix socket
//! ($NARRATER_SOCKET) and prints the answer. All the logic lives in the app.
//!
//! The app copies this binary to ~/.local/bin/narrater at startup and points
//! $NARRATER_CLI at it. Only std + serde_json, so it starts in milliseconds.

mod help;

use std::io::{IsTerminal, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Component, PathBuf};
use std::process::exit;
use std::time::Duration;

use serde_json::{json, Map, Value};

const ENV_KEYS: &[&str] = &["NARRATER_SOCKET", "NARRATER_ID", "NARRATER_LABEL", "NARRATER_CLI"];
const ROUTINE_VALUE_FLAGS: &[&str] = &[
    "--id", "--command", "--every", "--daily", "--weekly", "--once", "--terminal", "--count", "--until", "--pre-run",
];
const ROUTINE_BOOL_FLAGS: &[&str] =
    &["--reminder", "--no-skip-if-busy", "--skip-if-busy", "--no-notify", "--notify", "--disabled"];

fn fail(msg: &str) -> ! {
    eprintln!("{}", msg);
    exit(1)
}

fn help_of(topic: &str) -> &'static str {
    help::topic(topic).unwrap_or(help::MAIN)
}

/// Positional arguments with flags popped out of them as they are read.
struct Args(Vec<String>);

impl Args {
    /// Removes `flag value`; None when the flag isn't there.
    fn value(&mut self, flag: &str) -> Option<String> {
        let i = self.0.iter().position(|a| a == flag)?;
        if i + 1 >= self.0.len() {
            fail(&format!("Error: {} requires a value", flag));
        }
        let value = self.0.remove(i + 1);
        self.0.remove(i);
        Some(value)
    }

    /// Removes a valueless flag; true when it was there.
    fn flag(&mut self, flag: &str) -> bool {
        match self.0.iter().position(|a| a == flag) {
            Some(i) => {
                self.0.remove(i);
                true
            }
            None => false,
        }
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn get(&self, i: usize) -> &str {
        &self.0[i]
    }

    fn joined(&self, from: usize) -> String {
        self.0[from..].join(" ")
    }
}

/// Decodes \n, \t and \\ (plus \r, \e, \xNN and \0 with `raw`). In raw
/// mode \n is Enter (\r); unknown escapes pass through untouched.
fn decode(text: &str, raw: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' || i + 1 >= chars.len() {
            out.push(c);
            i += 1;
            continue;
        }
        match chars[i + 1] {
            'n' => out.push(if raw { '\r' } else { '\n' }),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            'r' if raw => out.push('\r'),
            'e' if raw => out.push('\x1b'),
            '0' if raw => out.push('\0'),
            'x' if raw && i + 3 < chars.len() && chars[i + 2].is_ascii_hexdigit() && chars[i + 3].is_ascii_hexdigit() => {
                let hex: String = chars[i + 2..i + 4].iter().collect();
                out.push(char::from(u8::from_str_radix(&hex, 16).unwrap_or(0)));
                i += 4;
                continue;
            }
            n => {
                out.push(c);
                out.push(n);
            }
        }
        i += 2;
    }
    out
}

/// `~` expanded, made absolute against the cwd, `.`/`..` resolved lexically.
fn abs_path(p: &str) -> String {
    let expanded = match (p.strip_prefix('~'), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            PathBuf::from(home).join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(p),
    };
    let full = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir().unwrap_or_default().join(expanded)
    };
    let mut out = PathBuf::new();
    for comp in full.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out.display().to_string()
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

fn request(payload: &Value) -> String {
    let sock = std::env::var("NARRATER_SOCKET").unwrap_or_default();
    if sock.is_empty() {
        fail("Error: NARRATER_SOCKET not set. Run this from a NarraTer terminal (`narrater debug` to diagnose).");
    }
    request_at(&sock, payload)
}

fn base(mode: &str) -> Map<String, Value> {
    let env = |k: &str| Value::String(std::env::var(k).unwrap_or_default());
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

fn send(payload: Map<String, Value>) {
    emit(&request(&Value::Object(payload)));
}

/// A request in `mode` carrying `params`.
fn send_params(mode: &str, params: Value) {
    let mut p = base(mode);
    p.insert("params".into(), params);
    send(p);
}

fn canvas(action: &str, params: Value) {
    let mut p = base("canvas");
    p.insert("action".into(), action.into());
    p.insert("params".into(), params);
    send(p);
}

fn cmd_ask(mut a: Args) {
    let batch = a.value("--batch");
    let raw = a.value("--raw");
    let timeout = a.value("--timeout");
    let mut p;
    if let Some(batch) = batch {
        let targets: Value =
            serde_json::from_str(&batch).unwrap_or_else(|e| fail(&format!("Error: --batch is not valid JSON ({})", e)));
        if !targets.as_object().is_some_and(|m| !m.is_empty()) {
            fail(r#"Error: --batch takes a JSON object, e.g. '{"A": "prompt", "B": "prompt"}'"#);
        }
        p = base("batch");
        p.insert("batch".into(), targets);
    } else if let Some(raw) = raw {
        if a.len() != 1 {
            fail(r#"Usage: narrater ask "Agent" --raw "input""#);
        }
        p = base("raw");
        p.insert("to".into(), a.get(0).into());
        p.insert("msg".into(), decode(&raw, true).into());
    } else {
        if a.len() < 2 {
            fail(r#"Usage: narrater ask "Agent" "prompt"   (run `narrater list` for names)"#);
        }
        p = base("ask");
        p.insert("to".into(), a.get(0).into());
        p.insert("msg".into(), decode(&a.joined(1), false).into());
    }
    if let Some(t) = timeout {
        let secs: i64 = t.parse().unwrap_or_else(|_| fail("Error: --timeout takes a number of seconds"));
        p.insert("timeout_secs".into(), secs.into());
    }
    send(p);
}

fn cmd_note(mut a: Args) {
    if a.len() == 0 {
        fail(help_of("note"));
    }
    let sub = a.0.remove(0);
    match sub.as_str() {
        "create" => {
            let name = a.value("--name");
            let content = if a.len() > 0 { decode(&a.joined(0), false) } else { String::new() };
            canvas("note_create", json!({ "content": content, "name": name }));
        }
        "read" => {
            if a.len() == 0 {
                fail(r#"Usage: narrater note read "Note" [offset] [limit]"#);
            }
            let mut params = json!({ "name": a.get(0) });
            let num = |s: &str| s.parse::<i64>().unwrap_or_else(|_| fail("Error: offset and limit must be numbers"));
            if a.len() > 1 {
                params["offset"] = num(a.get(1)).into();
            }
            if a.len() > 2 {
                params["limit"] = num(a.get(2)).into();
            }
            canvas("note_read", params);
        }
        "write" => {
            if a.len() < 2 {
                fail(r#"Usage: narrater note write "Note" "content""#);
            }
            canvas("note_write", json!({ "name": a.get(0), "content": decode(&a.joined(1), false) }));
        }
        "edit" => {
            if a.len() != 3 {
                fail(r#"Usage: narrater note edit "Note" "old text" "new text""#);
            }
            canvas(
                "note_edit",
                json!({ "name": a.get(0), "old": decode(a.get(1), false), "new": decode(a.get(2), false) }),
            );
        }
        "delete" => {
            if a.len() != 1 {
                fail(r#"Usage: narrater note delete "Note""#);
            }
            canvas("note_delete", json!({ "name": a.get(0) }));
        }
        _ => fail(help_of("note")),
    }
}

fn portal_request(to: &str, verb: &str, args: &[String]) {
    let mut p = base("portal");
    p.insert("to".into(), to.into());
    p.insert("msg".into(), verb.into());
    p.insert("args".into(), json!(args));
    send(p);
}

fn cmd_portal(mut a: Args) {
    if a.len() == 0 {
        fail(help_of("portal"));
    }
    let mut verb = a.0.remove(0);
    match verb.as_str() {
        "create" => {
            let size = a.value("--size");
            if let Some(sim) = a.value("--simulator") {
                if a.len() > 1 {
                    fail(r#"Usage: narrater portal create --simulator ID ["Name"]"#);
                }
                let name = if a.len() > 0 { a.get(0) } else { "" };
                return canvas("portal_create", json!({ "simulator": sim, "name": name }));
            }
            if a.len() == 0 || a.len() > 2 {
                fail(r#"Usage: narrater portal create URL ["Name"] [--size WxH]"#);
            }
            let name = if a.len() > 1 { a.get(1) } else { "" };
            return canvas("portal_create", json!({ "url": a.get(0), "name": name, "size": size.unwrap_or_default() }));
        }
        "devices" => return portal_request("", "devices", &[]),
        "close" => {
            if a.len() != 1 {
                fail(r#"Usage: narrater portal close "Portal""#);
            }
            return canvas("portal_close", json!({ "name": a.get(0) }));
        }
        _ => {}
    }
    if a.len() == 0 {
        fail(&format!(r#"Usage: narrater portal {} "Portal" …   (run `narrater help portal`)"#, verb));
    }
    let name = a.0.remove(0);
    if verb == "edit" {
        let url = a.value("--url");
        match url {
            Some(url) if a.len() == 0 => {
                verb = "navigate".into();
                a.0 = vec![url];
            }
            _ => fail(r#"Usage: narrater portal edit "Portal" --url URL"#),
        }
    }
    portal_request(&name, &verb, &a.0);
}

fn cmd_recruit(mut a: Args) {
    if a.value("--workspace").is_some() {
        fail("Error: --workspace isn't supported yet — NarraTer runs one workspace at a time; recruit here or on a --floor");
    }
    let floor = a.value("--floor");
    let preset = a.value("--preset");
    let role = a.value("--role");
    let command = a.value("--command");
    let dir = a.value("--dir");
    let replace = a.value("--replace");
    if a.len() > 1 {
        fail(help_of("recruit"));
    }
    if a.len() == 0 && replace.is_none() {
        fail(r#"Usage: narrater recruit "Name" [--preset P] [--role R]   (invent a short codename)"#);
    }
    let name = if a.len() > 0 { a.get(0) } else { "" };
    canvas(
        "recruit",
        json!({
            "name": name,
            "preset": preset.unwrap_or_default(),
            "role": role.unwrap_or_default(),
            "command": command.unwrap_or_default(),
            "dir": dir.unwrap_or_default(),
            "replace": replace.unwrap_or_default(),
            "floor": floor.unwrap_or_default(),
        }),
    );
}

fn cmd_role(mut a: Args) {
    if a.len() == 0 {
        fail(help_of("role"));
    }
    let sub = a.0.remove(0);
    match (sub.as_str(), a.len()) {
        ("list", 0) => canvas("role_list", json!({})),
        ("show", 1) => canvas("role_show", json!({ "name": a.get(0) })),
        ("create" | "write", 2) => {
            canvas(&format!("role_{}", sub), json!({ "name": a.get(0), "prompt": decode(a.get(1), false) }))
        }
        ("edit", 3) => canvas(
            "role_edit",
            json!({ "name": a.get(0), "old": decode(a.get(1), false), "new": decode(a.get(2), false) }),
        ),
        ("assign", _) => {
            let none = a.flag("--none");
            match (none, a.len()) {
                (true, 1) => canvas("role_assign", json!({ "recruit": a.get(0), "none": true })),
                (false, 2) => canvas("role_assign", json!({ "recruit": a.get(0), "name": a.get(1) })),
                _ => fail(r#"Usage: narrater role assign "Recruit" "Role" | narrater role assign "Recruit" --none"#),
            }
        }
        ("delete", _) => {
            let force = a.flag("--force");
            if a.len() != 1 {
                fail(r#"Usage: narrater role delete "Name" [--force]"#);
            }
            canvas("role_delete", json!({ "name": a.get(0), "force": force }));
        }
        _ if a.0.iter().any(|x| x == "--scope") => fail("Error: NarraTer roles are global — drop --scope"),
        _ => fail(help_of("role")),
    }
}

fn cmd_say(mut a: Args) {
    let mut params = json!({ "verb": "say" });
    let mut attach = Vec::new();
    while let Some(v) = a.value("--attach") {
        attach.push(abs_path(&v));
    }
    if !attach.is_empty() {
        params["attach"] = json!(attach);
    }
    if a.flag("--progress") {
        params["progress"] = true.into();
    }
    if let Some(i) = a.0.iter().position(|x| x == "--file") {
        if i + 1 >= a.len() {
            fail("Error: --file needs a path");
        }
        params["file"] = abs_path(&a.0.remove(i + 1)).into();
        a.0.remove(i);
    }
    let mut text = a.joined(0);
    if text.is_empty() && params.get("file").is_none() && !std::io::stdin().is_terminal() {
        let _ = std::io::stdin().read_to_string(&mut text);
    }
    params["text"] = text.into();
    send_params("chat", params);
}

fn cmd_recall(mut a: Args) {
    let mut params = json!({ "verb": "recall" });
    for flag in ["--last", "--from", "--to"] {
        if let Some(v) = a.value(flag) {
            params[&flag[2..]] = v.into();
        }
    }
    if let Some(t) = a.value("--turns") {
        params["turns"] = t.parse::<i64>().unwrap_or_else(|_| fail("Error: --turns takes a number")).into();
    }
    if a.flag("--all") {
        params["all"] = true.into();
    }
    if a.flag("--json") {
        params["json"] = true.into();
    }
    if a.len() > 0 && a.get(0) == "attachment" {
        if a.len() != 2 {
            fail("Usage: narrater recall attachment ID");
        }
        params["thread"] = "attachment".into();
        params["id"] = a.get(1).into();
    } else if a.len() == 1 {
        params["thread"] = a.get(0).into();
    } else if a.len() > 1 {
        fail(help_of("recall"));
    }
    send_params("chat", params);
}

fn cmd_workspace(mut a: Args) {
    if a.len() == 0 {
        fail(help_of("workspace"));
    }
    let verb = a.0.remove(0);
    let mut params = json!({ "verb": verb });
    for flag in ["--dir", "--from", "--group", "--folder"] {
        if let Some(v) = a.value(flag) {
            params[&flag[2..]] = v.into();
        }
    }
    if a.flag("--root") {
        params["root"] = true.into();
    }
    match verb.as_str() {
        "list" if a.len() > 0 => fail("Usage: narrater workspace list"),
        "list" => {}
        "rename" => {
            if a.len() != 2 {
                fail(r#"Usage: narrater workspace rename "Current Name" "New Name""#);
            }
            params["name"] = a.get(0).into();
            params["new_name"] = a.get(1).into();
        }
        "create" | "move" => {
            if a.len() != 1 {
                fail(help_of("workspace"));
            }
            params["name"] = a.get(0).into();
        }
        _ => fail(help_of("workspace")),
    }
    send_params("workspace", params);
}

fn cmd_floor(mut a: Args) {
    if a.len() == 0 {
        fail(help_of("floor"));
    }
    let verb = a.0.remove(0);
    let one_name = |a: &Args, usage: &str| {
        if a.len() != 1 || a.get(0).starts_with("--") {
            fail(usage);
        }
        a.get(0).to_string()
    };
    match verb.as_str() {
        "list" => canvas("floor_list", json!({})),
        "status" => {
            let mut params = json!({});
            for flag in ["--state", "--progress", "--floor"] {
                if let Some(v) = a.value(flag) {
                    params[&flag[2..]] = v.into();
                }
            }
            if a.flag("--clear") {
                params["clear"] = true.into();
            }
            if a.len() > 1 || a.0.iter().any(|x| x.starts_with("--")) {
                fail(r#"Usage: narrater floor status ["Text"] [--state working|blocked|review|done] [--progress 2/5] [--floor "F"] [--clear]"#);
            }
            if a.len() == 1 {
                params["text"] = decode(a.get(0), false).into();
            }
            canvas("floor_status", params);
        }
        "land" => {
            let into = a.value("--into").unwrap_or_default();
            let delete_branch = a.flag("--delete-branch");
            let keep_floor = a.flag("--keep-floor");
            let name = one_name(&a, r#"Usage: narrater floor land "Name" [--into BRANCH] [--delete-branch] [--keep-floor]"#);
            canvas(
                "floor_land",
                json!({ "name": name, "into": into, "delete_branch": delete_branch, "keep_floor": keep_floor }),
            );
        }
        "delete" => {
            let keep_branch = a.flag("--keep-branch");
            let force = a.flag("--force");
            let name = one_name(&a, r#"Usage: narrater floor delete "Name" [--keep-branch] [--force]"#);
            canvas("floor_delete", json!({ "name": name, "keep_branch": keep_branch, "force": force }));
        }
        "create" => {
            let branch = a.value("--branch").unwrap_or_default();
            let pull_request = a.value("--pull-request").unwrap_or_default();
            let existing_branch = a.flag("--existing-branch");
            let no_git = a.flag("--no-git");
            let copy = a.flag("--copy");
            let copy_ground = a.flag("--copy-ground");
            if let Some(flag) = a.0.iter().find(|x| x.starts_with("--")) {
                fail(&format!("Error: unknown flag {} — run `narrater help floor`", flag));
            }
            let named = a.len() == 1 || (a.len() == 0 && !pull_request.is_empty());
            if !named {
                fail(r#"Usage: narrater floor create "Name" [--branch B] [--existing-branch] [--copy] [--no-git] [--copy-ground]
       narrater floor create ["Name"] --pull-request NUMBER|URL [--copy] [--copy-ground]"#);
            }
            if !pull_request.is_empty() && (!branch.is_empty() || existing_branch || no_git) {
                fail("Error: --branch, --existing-branch and --no-git don't apply with --pull-request");
            }
            canvas(
                "floor_create",
                json!({
                    "name": if a.len() == 1 { a.get(0) } else { "" },
                    "branch": branch,
                    "existing_branch": existing_branch,
                    "no_git": no_git,
                    "copy": copy,
                    "copy_ground": copy_ground,
                    "pull_request": pull_request,
                }),
            );
        }
        _ => fail(help_of("floor")),
    }
}

fn cmd_routine(mut a: Args) {
    if a.len() == 0 {
        fail(help_of("routine"));
    }
    let verb = a.0.remove(0);
    if !["list", "show", "create", "edit", "enable", "disable", "run", "delete"].contains(&verb.as_str()) {
        fail(help_of("routine"));
    }
    let mut params = json!({ "verb": verb });
    if let Some(new_name) = a.value("--name") {
        if verb != "edit" {
            fail("Error: --name renames a routine in `routine edit`; the name is the first argument");
        }
        params["new_name"] = new_name.into();
    }
    let key = |flag: &str| flag[2..].replace('-', "_");
    for flag in ROUTINE_VALUE_FLAGS {
        if let Some(v) = a.value(flag) {
            params[key(flag)] = v.into();
        }
    }
    for flag in ROUTINE_BOOL_FLAGS {
        if a.flag(flag) {
            params[key(flag)] = true.into();
        }
    }
    if let Some(c) = params.get("command").and_then(Value::as_str).map(|c| decode(c, false)) {
        params["command"] = c.into();
    }
    if let Some(c) = params.get("count").and_then(Value::as_str).map(str::to_string) {
        params["count"] = c.trim().parse::<i64>().unwrap_or_else(|_| fail("Error: --count takes a number")).into();
    }
    if let Some(flag) = a.0.iter().find(|x| x.starts_with("--")) {
        fail(&format!("Error: unknown flag {} — run `narrater help routine`", flag));
    }
    if verb == "list" {
        if a.len() > 0 {
            fail("Usage: narrater routine list");
        }
    } else if a.len() != 1 {
        fail(&format!(r#"Usage: narrater routine {} "Name" …   (run `narrater help routine`)"#, verb));
    } else {
        params["name"] = a.get(0).into();
    }
    send_params("routine", params);
}

fn cmd_debug() {
    println!("=== NarraTer CLI Diagnostics ===\n");
    println!("--- Environment ---");
    for k in ENV_KEYS {
        println!("{:<16} {}", k, std::env::var(k).unwrap_or_else(|_| "(not set)".into()));
    }
    let token = if std::env::var("NARRATER_TOKEN").is_ok_and(|t| !t.is_empty()) { "(set)" } else { "(not set)" };
    println!("{:<16} {}", "NARRATER_TOKEN", token);
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "(unknown)".into());
    println!("{:<16} {} (v{})", "CLI", exe, env!("CARGO_PKG_VERSION"));
    let sock = std::env::var("NARRATER_SOCKET").unwrap_or_default();
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
    if std::env::var("NARRATER_ID").is_ok_and(|id| !id.is_empty()) {
        let out = request(&Value::Object(base("whoami")));
        println!("Identity:        {}", out.trim().replace('\n', " · "));
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || ["-h", "--help", "help"].contains(&argv[0].as_str()) {
        let topic = if argv.first().map(String::as_str) == Some("help") { argv.get(1) } else { None };
        println!("{}", topic.map_or(help::MAIN, |t| help_of(t)));
        return;
    }
    let cmd = argv[0].clone();
    let a = Args(argv[1..].to_vec());
    if a.0.iter().any(|x| x == "--help" || x == "-h") {
        println!("{}", help_of(&cmd));
        return;
    }
    match cmd.as_str() {
        "list" => canvas("list", json!({})),
        "ask" => cmd_ask(a),
        "check" => {
            if a.len() != 1 {
                fail(r#"Usage: narrater check "Agent""#);
            }
            let mut p = base("check");
            p.insert("to".into(), a.get(0).into());
            send(p);
        }
        "note" => cmd_note(a),
        "connect" => {
            if a.len() != 2 {
                fail(r#"Usage: narrater connect "From" "To""#);
            }
            canvas("connect", json!({ "from": a.get(0), "to": a.get(1) }));
        }
        "portal" => cmd_portal(a),
        "recruit" => cmd_recruit(a),
        "dismiss" => {
            if a.len() != 1 {
                fail(r#"Usage: narrater dismiss "Name""#);
            }
            canvas("dismiss", json!({ "name": a.get(0) }));
        }
        "preset" => {
            if a.0 != ["list"] {
                fail("Usage: narrater preset list");
            }
            canvas("preset_list", json!({}));
        }
        "role" => cmd_role(a),
        "notify" => {
            if a.len() == 0 {
                fail(r#"Usage: narrater notify "message""#);
            }
            canvas("notify", json!({ "message": decode(&a.joined(0), false) }));
        }
        "routine" => cmd_routine(a),
        "workspace" => cmd_workspace(a),
        "say" => cmd_say(a),
        "recall" => cmd_recall(a),
        "floor" => cmd_floor(a),
        "debug" => cmd_debug(),
        "version" | "--version" => println!("narrater {}", env!("CARGO_PKG_VERSION")),
        _ => fail(&format!("Unknown command '{}'. Run `narrater help`.", cmd)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_prompt_escapes() {
        assert_eq!(decode(r"a\nb\tc", false), "a\nb\tc");
        assert_eq!(decode(r"keep \\n literal", false), r"keep \n literal");
        assert_eq!(decode(r"no raw \e here", false), r"no raw \e here");
        assert_eq!(decode(r"trailing \", false), r"trailing \");
    }

    #[test]
    fn decodes_raw_keys() {
        assert_eq!(decode(r"2\n", true), "2\r");
        assert_eq!(decode(r"\e[A\x03\0", true), "\x1b[A\x03\0");
        assert_eq!(decode(r"\xZZ", true), r"\xZZ");
        assert_eq!(decode(r"\x4", true), r"\x4");
    }

    #[test]
    fn flags_pop_out_of_positionals() {
        let mut a = Args(["Dev", "--raw", "x", "--json"].map(String::from).to_vec());
        assert_eq!(a.value("--raw").as_deref(), Some("x"));
        assert!(a.flag("--json"));
        assert!(!a.flag("--json"));
        assert_eq!(a.0, ["Dev"]);
        let mut a = Args(["--attach", "a", "--attach", "b"].map(String::from).to_vec());
        assert_eq!((a.value("--attach"), a.value("--attach"), a.value("--attach")), (Some("a".into()), Some("b".into()), None));
    }

    #[test]
    fn paths_are_absolute_and_normalized() {
        assert_eq!(abs_path("/a/b/../c/./d"), "/a/c/d");
        let home = std::env::var("HOME").unwrap();
        assert_eq!(abs_path("~/x"), format!("{}/x", home));
        assert!(abs_path("rel").starts_with('/'));
    }

    #[test]
    fn every_command_has_help() {
        for t in ["ask", "check", "note", "list", "connect", "say", "recall", "portal", "recruit", "dismiss", "role",
                  "preset", "notify", "workspace", "floor", "routine", "debug"] {
            assert!(help::topic(t).is_some(), "{}", t);
        }
        assert!(help::MAIN.contains("narrater floor status"));
    }
}

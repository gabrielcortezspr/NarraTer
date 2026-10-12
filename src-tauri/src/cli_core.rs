//! The `narrater` command line → one request for the app. Shared by the
//! local CLI binary (which sends it over the Unix socket) and by the bridge
//! that serves agents in remote workspaces (SSH, Docker), whose `narrater`
//! is a small curl wrapper posting its raw argv — like Maestri's, the logic
//! stays in the app.
//!
//! Only std + serde_json: the binary includes this file with `#[path]`.

use std::path::{Component, PathBuf};

use serde_json::{json, Map, Value};

#[path = "cli_help.rs"]
pub mod help;

const ROUTINE_VALUE_FLAGS: &[&str] = &[
    "--id", "--command", "--every", "--daily", "--weekly", "--once", "--terminal", "--count", "--until", "--pre-run",
];
const ROUTINE_BOOL_FLAGS: &[&str] =
    &["--reminder", "--no-skip-if-busy", "--skip-if-busy", "--no-notify", "--notify", "--disabled"];

/// What a command line amounts to.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Printed as is (help, version), exit 0
    Print(String),
    /// A request for the app (the JSON the Unix socket takes)
    Request(Value),
    /// `narrater debug` — each transport prints its own diagnostics
    Debug,
}

/// Errors go to stderr with exit status 1.
pub type Parsed = Result<Outcome, String>;

/// Where the command runs: its environment, directory and stdin.
pub struct Env<'a> {
    pub var: &'a dyn Fn(&str) -> Option<String>,
    pub cwd: PathBuf,
    /// Called at most once, only by `say` without text: stdin when it isn't a tty
    pub stdin: &'a mut dyn FnMut() -> Option<String>,
}

fn help_of(topic: &str) -> String {
    help::topic(topic).unwrap_or(help::MAIN).to_string()
}

/// Positional arguments with flags popped out of them as they are read.
struct Args(Vec<String>);

impl Args {
    /// Removes `flag value`; None when the flag isn't there.
    fn value(&mut self, flag: &str) -> Result<Option<String>, String> {
        let Some(i) = self.0.iter().position(|a| a == flag) else { return Ok(None) };
        if i + 1 >= self.0.len() {
            return Err(format!("Error: {} requires a value", flag));
        }
        let value = self.0.remove(i + 1);
        self.0.remove(i);
        Ok(Some(value))
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
pub fn decode(text: &str, raw: bool) -> String {
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
fn abs_path(env: &Env, p: &str) -> String {
    let expanded = match (p.strip_prefix('~'), (env.var)("HOME")) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            PathBuf::from(home).join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(p),
    };
    let full = if expanded.is_absolute() { expanded } else { env.cwd.join(expanded) };
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

struct Cli<'a, 'b> {
    env: &'a mut Env<'b>,
}

impl Cli<'_, '_> {
    fn base(&self, mode: &str) -> Map<String, Value> {
        let var = |k: &str| Value::String((self.env.var)(k).unwrap_or_default());
        let mut m = Map::new();
        m.insert("from".into(), var("NARRATER_ID"));
        m.insert("token".into(), var("NARRATER_TOKEN"));
        m.insert("mode".into(), mode.into());
        m
    }

    fn send(&self, payload: Map<String, Value>) -> Parsed {
        Ok(Outcome::Request(Value::Object(payload)))
    }

    /// A request in `mode` carrying `params`.
    fn send_params(&self, mode: &str, params: Value) -> Parsed {
        let mut p = self.base(mode);
        p.insert("params".into(), params);
        self.send(p)
    }

    fn canvas(&self, action: &str, params: Value) -> Parsed {
        let mut p = self.base("canvas");
        p.insert("action".into(), action.into());
        p.insert("params".into(), params);
        self.send(p)
    }

    fn ask(&self, mut a: Args) -> Parsed {
        let batch = a.value("--batch")?;
        let raw = a.value("--raw")?;
        let timeout = a.value("--timeout")?;
        let mut p;
        if let Some(batch) = batch {
            let targets: Value =
                serde_json::from_str(&batch).map_err(|e| format!("Error: --batch is not valid JSON ({})", e))?;
            if !targets.as_object().is_some_and(|m| !m.is_empty()) {
                return Err(r#"Error: --batch takes a JSON object, e.g. '{"A": "prompt", "B": "prompt"}'"#.into());
            }
            p = self.base("batch");
            p.insert("batch".into(), targets);
        } else if let Some(raw) = raw {
            if a.len() != 1 {
                return Err(r#"Usage: narrater ask "Agent" --raw "input""#.into());
            }
            p = self.base("raw");
            p.insert("to".into(), a.get(0).into());
            p.insert("msg".into(), decode(&raw, true).into());
        } else {
            if a.len() < 2 {
                return Err(r#"Usage: narrater ask "Agent" "prompt"   (run `narrater list` for names)"#.into());
            }
            p = self.base("ask");
            p.insert("to".into(), a.get(0).into());
            p.insert("msg".into(), decode(&a.joined(1), false).into());
        }
        if let Some(t) = timeout {
            let secs: i64 = t.parse().map_err(|_| "Error: --timeout takes a number of seconds".to_string())?;
            p.insert("timeout_secs".into(), secs.into());
        }
        self.send(p)
    }

    fn note(&self, mut a: Args) -> Parsed {
        if a.len() == 0 {
            return Err(help_of("note"));
        }
        let sub = a.0.remove(0);
        match sub.as_str() {
            "create" => {
                let name = a.value("--name")?;
                let content = if a.len() > 0 { decode(&a.joined(0), false) } else { String::new() };
                self.canvas("note_create", json!({ "content": content, "name": name }))
            }
            "read" => {
                if a.len() == 0 {
                    return Err(r#"Usage: narrater note read "Note" [offset] [limit]"#.into());
                }
                let mut params = json!({ "name": a.get(0) });
                let num = |s: &str| s.parse::<i64>().map_err(|_| "Error: offset and limit must be numbers".to_string());
                if a.len() > 1 {
                    params["offset"] = num(a.get(1))?.into();
                }
                if a.len() > 2 {
                    params["limit"] = num(a.get(2))?.into();
                }
                self.canvas("note_read", params)
            }
            "write" => {
                if a.len() < 2 {
                    return Err(r#"Usage: narrater note write "Note" "content""#.into());
                }
                self.canvas("note_write", json!({ "name": a.get(0), "content": decode(&a.joined(1), false) }))
            }
            "edit" => {
                if a.len() != 3 {
                    return Err(r#"Usage: narrater note edit "Note" "old text" "new text""#.into());
                }
                self.canvas(
                    "note_edit",
                    json!({ "name": a.get(0), "old": decode(a.get(1), false), "new": decode(a.get(2), false) }),
                )
            }
            "delete" => {
                if a.len() != 1 {
                    return Err(r#"Usage: narrater note delete "Note""#.into());
                }
                self.canvas("note_delete", json!({ "name": a.get(0) }))
            }
            _ => Err(help_of("note")),
        }
    }

    fn portal_request(&self, to: &str, verb: &str, args: &[String]) -> Parsed {
        let mut p = self.base("portal");
        p.insert("to".into(), to.into());
        p.insert("msg".into(), verb.into());
        p.insert("args".into(), json!(args));
        self.send(p)
    }

    fn portal(&self, mut a: Args) -> Parsed {
        if a.len() == 0 {
            return Err(help_of("portal"));
        }
        let mut verb = a.0.remove(0);
        match verb.as_str() {
            "create" => {
                let size = a.value("--size")?;
                if let Some(flag) = a.0.iter().find(|x| x.starts_with("--")) {
                    return Err(format!("Error: unknown flag {} — run `narrater help portal`", flag));
                }
                if a.len() == 0 || a.len() > 2 {
                    return Err(r#"Usage: narrater portal create URL ["Name"] [--size WxH]"#.into());
                }
                let name = if a.len() > 1 { a.get(1) } else { "" };
                return self
                    .canvas("portal_create", json!({ "url": a.get(0), "name": name, "size": size.unwrap_or_default() }));
            }
            "close" => {
                if a.len() != 1 {
                    return Err(r#"Usage: narrater portal close "Portal""#.into());
                }
                return self.canvas("portal_close", json!({ "name": a.get(0) }));
            }
            _ => {}
        }
        if a.len() == 0 {
            return Err(format!(r#"Usage: narrater portal {} "Portal" …   (run `narrater help portal`)"#, verb));
        }
        let name = a.0.remove(0);
        if verb == "edit" {
            match a.value("--url")? {
                Some(url) if a.len() == 0 => {
                    verb = "navigate".into();
                    a.0 = vec![url];
                }
                _ => return Err(r#"Usage: narrater portal edit "Portal" --url URL"#.into()),
            }
        }
        self.portal_request(&name, &verb, &a.0)
    }

    fn recruit(&self, mut a: Args) -> Parsed {
        let workspace = a.value("--workspace")?;
        let floor = a.value("--floor")?;
        let preset = a.value("--preset")?;
        let role = a.value("--role")?;
        let command = a.value("--command")?;
        let dir = a.value("--dir")?;
        let replace = a.value("--replace")?;
        if a.len() > 1 {
            return Err(help_of("recruit"));
        }
        if a.len() == 0 && replace.is_none() {
            return Err(r#"Usage: narrater recruit "Name" [--preset P] [--role R]   (invent a short codename)"#.into());
        }
        let name = if a.len() > 0 { a.get(0) } else { "" };
        self.canvas(
            "recruit",
            json!({
                "name": name,
                "preset": preset.unwrap_or_default(),
                "role": role.unwrap_or_default(),
                "command": command.unwrap_or_default(),
                "dir": dir.unwrap_or_default(),
                "replace": replace.unwrap_or_default(),
                "floor": floor.unwrap_or_default(),
                "workspace": workspace.unwrap_or_default(),
            }),
        )
    }

    fn role(&self, mut a: Args) -> Parsed {
        if a.len() == 0 {
            return Err(help_of("role"));
        }
        let sub = a.0.remove(0);
        match (sub.as_str(), a.len()) {
            ("list", 0) => self.canvas("role_list", json!({})),
            ("show", 1) => self.canvas("role_show", json!({ "name": a.get(0) })),
            ("create" | "write", 2) => {
                self.canvas(&format!("role_{}", sub), json!({ "name": a.get(0), "prompt": decode(a.get(1), false) }))
            }
            ("edit", 3) => self.canvas(
                "role_edit",
                json!({ "name": a.get(0), "old": decode(a.get(1), false), "new": decode(a.get(2), false) }),
            ),
            ("assign", _) => {
                let none = a.flag("--none");
                match (none, a.len()) {
                    (true, 1) => self.canvas("role_assign", json!({ "recruit": a.get(0), "none": true })),
                    (false, 2) => self.canvas("role_assign", json!({ "recruit": a.get(0), "name": a.get(1) })),
                    _ => Err(r#"Usage: narrater role assign "Recruit" "Role" | narrater role assign "Recruit" --none"#.into()),
                }
            }
            ("delete", _) => {
                let force = a.flag("--force");
                if a.len() != 1 {
                    return Err(r#"Usage: narrater role delete "Name" [--force]"#.into());
                }
                self.canvas("role_delete", json!({ "name": a.get(0), "force": force }))
            }
            _ if a.0.iter().any(|x| x == "--scope") => Err("Error: NarraTer roles are global — drop --scope".into()),
            _ => Err(help_of("role")),
        }
    }

    fn say(&mut self, mut a: Args) -> Parsed {
        let mut params = json!({ "verb": "say" });
        let mut attach = Vec::new();
        while let Some(v) = a.value("--attach")? {
            attach.push(abs_path(self.env, &v));
        }
        if !attach.is_empty() {
            params["attach"] = json!(attach);
        }
        if a.flag("--progress") {
            params["progress"] = true.into();
        }
        if let Some(i) = a.0.iter().position(|x| x == "--file") {
            if i + 1 >= a.len() {
                return Err("Error: --file needs a path".into());
            }
            params["file"] = abs_path(self.env, &a.0.remove(i + 1)).into();
            a.0.remove(i);
        }
        let mut text = a.joined(0);
        if text.is_empty() && params.get("file").is_none() {
            text = (self.env.stdin)().unwrap_or_default();
        }
        params["text"] = text.into();
        self.send_params("chat", params)
    }

    fn recall(&self, mut a: Args) -> Parsed {
        let mut params = json!({ "verb": "recall" });
        for flag in ["--last", "--from", "--to"] {
            if let Some(v) = a.value(flag)? {
                params[&flag[2..]] = v.into();
            }
        }
        if let Some(t) = a.value("--turns")? {
            params["turns"] = t.parse::<i64>().map_err(|_| "Error: --turns takes a number".to_string())?.into();
        }
        if a.flag("--all") {
            params["all"] = true.into();
        }
        if a.flag("--json") {
            params["json"] = true.into();
        }
        if a.len() > 0 && a.get(0) == "attachment" {
            if a.len() != 2 {
                return Err("Usage: narrater recall attachment ID".into());
            }
            params["thread"] = "attachment".into();
            params["id"] = a.get(1).into();
        } else if a.len() == 1 {
            params["thread"] = a.get(0).into();
        } else if a.len() > 1 {
            return Err(help_of("recall"));
        }
        self.send_params("chat", params)
    }

    fn workspace(&self, mut a: Args) -> Parsed {
        if a.len() == 0 {
            return Err(help_of("workspace"));
        }
        let verb = a.0.remove(0);
        let mut params = json!({ "verb": verb });
        for flag in ["--dir", "--from", "--group", "--folder"] {
            if let Some(v) = a.value(flag)? {
                params[&flag[2..]] = v.into();
            }
        }
        if a.flag("--root") {
            params["root"] = true.into();
        }
        match verb.as_str() {
            "list" if a.len() > 0 => return Err("Usage: narrater workspace list".into()),
            "list" => {}
            "rename" => {
                if a.len() != 2 {
                    return Err(r#"Usage: narrater workspace rename "Current Name" "New Name""#.into());
                }
                params["name"] = a.get(0).into();
                params["new_name"] = a.get(1).into();
            }
            "create" | "move" => {
                if a.len() != 1 {
                    return Err(help_of("workspace"));
                }
                params["name"] = a.get(0).into();
            }
            _ => return Err(help_of("workspace")),
        }
        self.send_params("workspace", params)
    }

    fn floor(&self, mut a: Args) -> Parsed {
        if a.len() == 0 {
            return Err(help_of("floor"));
        }
        let verb = a.0.remove(0);
        let one_name = |a: &Args, usage: &str| -> Result<String, String> {
            if a.len() != 1 || a.get(0).starts_with("--") {
                return Err(usage.into());
            }
            Ok(a.get(0).to_string())
        };
        match verb.as_str() {
            "list" => self.canvas("floor_list", json!({})),
            "status" => {
                let mut params = json!({});
                for flag in ["--state", "--progress", "--floor"] {
                    if let Some(v) = a.value(flag)? {
                        params[&flag[2..]] = v.into();
                    }
                }
                if a.flag("--clear") {
                    params["clear"] = true.into();
                }
                if a.len() > 1 || a.0.iter().any(|x| x.starts_with("--")) {
                    return Err(r#"Usage: narrater floor status ["Text"] [--state working|blocked|review|done] [--progress 2/5] [--floor "F"] [--clear]"#.into());
                }
                if a.len() == 1 {
                    params["text"] = decode(a.get(0), false).into();
                }
                self.canvas("floor_status", params)
            }
            "land" => {
                let into = a.value("--into")?.unwrap_or_default();
                let delete_branch = a.flag("--delete-branch");
                let keep_floor = a.flag("--keep-floor");
                let name = one_name(&a, r#"Usage: narrater floor land "Name" [--into BRANCH] [--delete-branch] [--keep-floor]"#)?;
                self.canvas(
                    "floor_land",
                    json!({ "name": name, "into": into, "delete_branch": delete_branch, "keep_floor": keep_floor }),
                )
            }
            "delete" => {
                let keep_branch = a.flag("--keep-branch");
                let force = a.flag("--force");
                let name = one_name(&a, r#"Usage: narrater floor delete "Name" [--keep-branch] [--force]"#)?;
                self.canvas("floor_delete", json!({ "name": name, "keep_branch": keep_branch, "force": force }))
            }
            "create" => {
                let branch = a.value("--branch")?.unwrap_or_default();
                let pull_request = a.value("--pull-request")?.unwrap_or_default();
                let existing_branch = a.flag("--existing-branch");
                let no_git = a.flag("--no-git");
                let copy = a.flag("--copy");
                let copy_ground = a.flag("--copy-ground");
                if let Some(flag) = a.0.iter().find(|x| x.starts_with("--")) {
                    return Err(format!("Error: unknown flag {} — run `narrater help floor`", flag));
                }
                let named = a.len() == 1 || (a.len() == 0 && !pull_request.is_empty());
                if !named {
                    return Err(r#"Usage: narrater floor create "Name" [--branch B] [--existing-branch] [--copy] [--no-git] [--copy-ground]
       narrater floor create ["Name"] --pull-request NUMBER|URL [--copy] [--copy-ground]"#.into());
                }
                if !pull_request.is_empty() && (!branch.is_empty() || existing_branch || no_git) {
                    return Err("Error: --branch, --existing-branch and --no-git don't apply with --pull-request".into());
                }
                self.canvas(
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
                )
            }
            _ => Err(help_of("floor")),
        }
    }

    fn routine(&self, mut a: Args) -> Parsed {
        if a.len() == 0 {
            return Err(help_of("routine"));
        }
        let verb = a.0.remove(0);
        if !["list", "show", "create", "edit", "enable", "disable", "run", "delete"].contains(&verb.as_str()) {
            return Err(help_of("routine"));
        }
        let mut params = json!({ "verb": verb });
        if let Some(new_name) = a.value("--name")? {
            if verb != "edit" {
                return Err("Error: --name renames a routine in `routine edit`; the name is the first argument".into());
            }
            params["new_name"] = new_name.into();
        }
        let key = |flag: &str| flag[2..].replace('-', "_");
        for flag in ROUTINE_VALUE_FLAGS {
            if let Some(v) = a.value(flag)? {
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
            params["count"] = c.trim().parse::<i64>().map_err(|_| "Error: --count takes a number".to_string())?.into();
        }
        if let Some(flag) = a.0.iter().find(|x| x.starts_with("--")) {
            return Err(format!("Error: unknown flag {} — run `narrater help routine`", flag));
        }
        if verb == "list" {
            if a.len() > 0 {
                return Err("Usage: narrater routine list".into());
            }
        } else if a.len() != 1 {
            return Err(format!(r#"Usage: narrater routine {} "Name" …   (run `narrater help routine`)"#, verb));
        } else {
            params["name"] = a.get(0).into();
        }
        self.send_params("routine", params)
    }
}

/// Parses a `narrater` command line (without the program name).
pub fn parse(argv: &[String], env: &mut Env) -> Parsed {
    if argv.is_empty() || ["-h", "--help", "help"].contains(&argv[0].as_str()) {
        let topic = if argv.first().map(String::as_str) == Some("help") { argv.get(1) } else { None };
        return Ok(Outcome::Print(topic.map_or(help::MAIN.to_string(), |t| help_of(t))));
    }
    let cmd = argv[0].as_str();
    let a = Args(argv[1..].to_vec());
    if a.0.iter().any(|x| x == "--help" || x == "-h") {
        return Ok(Outcome::Print(help_of(cmd)));
    }
    let mut cli = Cli { env };
    match cmd {
        "list" => cli.canvas("list", json!({})),
        "ask" => cli.ask(a),
        "check" => {
            if a.len() != 1 {
                return Err(r#"Usage: narrater check "Agent""#.into());
            }
            let mut p = cli.base("check");
            p.insert("to".into(), a.get(0).into());
            cli.send(p)
        }
        "note" => cli.note(a),
        "connect" => {
            if a.len() != 2 {
                return Err(r#"Usage: narrater connect "From" "To""#.into());
            }
            cli.canvas("connect", json!({ "from": a.get(0), "to": a.get(1) }))
        }
        "portal" => cli.portal(a),
        "recruit" => cli.recruit(a),
        "dismiss" => {
            if a.len() != 1 {
                return Err(r#"Usage: narrater dismiss "Name""#.into());
            }
            cli.canvas("dismiss", json!({ "name": a.get(0) }))
        }
        "preset" => {
            if a.0 != ["list"] {
                return Err("Usage: narrater preset list".into());
            }
            cli.canvas("preset_list", json!({}))
        }
        "role" => cli.role(a),
        "notify" => {
            if a.len() == 0 {
                return Err(r#"Usage: narrater notify "message""#.into());
            }
            cli.canvas("notify", json!({ "message": decode(&a.joined(0), false) }))
        }
        "routine" => cli.routine(a),
        "workspace" => cli.workspace(a),
        "say" => cli.say(a),
        "recall" => cli.recall(a),
        "floor" => cli.floor(a),
        "debug" => Ok(Outcome::Debug),
        "version" | "--version" => Ok(Outcome::Print(format!("narrater {}", env!("CARGO_PKG_VERSION")))),
        _ => Err(format!("Unknown command '{}'. Run `narrater help`.", cmd)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Parsed {
        let vars = |k: &str| match k {
            "NARRATER_ID" => Some("t1".to_string()),
            "NARRATER_TOKEN" => Some("tok".to_string()),
            "HOME" => Some("/home/u".to_string()),
            _ => None,
        };
        let mut stdin = || Some("from stdin".to_string());
        let mut env = Env { var: &vars, cwd: PathBuf::from("/work"), stdin: &mut stdin };
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &mut env)
    }

    fn request(args: &[&str]) -> Value {
        match run(args) {
            Ok(Outcome::Request(v)) => v,
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn decodes_prompt_escapes() {
        assert_eq!(decode(r"a\nb\tc", false), "a\nb\tc");
        assert_eq!(decode(r"keep \\n literal", false), r"keep \n literal");
        assert_eq!(decode(r"no raw \e here", false), r"no raw \e here");
        assert_eq!(decode(r"trailing \", false), r"trailing \");
        assert_eq!(decode(r"2\n", true), "2\r");
        assert_eq!(decode(r"\e[A\x03\0", true), "\x1b[A\x03\0");
        assert_eq!(decode(r"\xZZ", true), r"\xZZ");
    }

    #[test]
    fn builds_requests_with_identity() {
        let v = request(&["ask", "Dev", "do", "it\\n"]);
        assert_eq!(v["mode"], "ask");
        assert_eq!(v["from"], "t1");
        assert_eq!(v["token"], "tok");
        assert_eq!(v["msg"], "do it\n");
        let v = request(&["floor", "create", "--pull-request", "42"]);
        assert_eq!(v["params"]["pull_request"], "42");
        assert_eq!(v["params"]["name"], "");
    }

    #[test]
    fn say_paths_and_stdin_follow_the_callers_environment() {
        let v = request(&["say", "--attach", "~/a.pdf", "--attach", "rel/../b.png", "done"]);
        assert_eq!(v["params"]["attach"], json!(["/home/u/a.pdf", "/work/b.png"]));
        assert_eq!(v["params"]["text"], "done");
        assert_eq!(request(&["say"])["params"]["text"], "from stdin");
        assert_eq!(run(&["say", "--file"]), Err("Error: --file needs a path".into()));
    }

    #[test]
    fn help_errors_and_debug() {
        assert!(matches!(run(&[]), Ok(Outcome::Print(s)) if s.contains("narrater list")));
        assert!(matches!(run(&["help", "floor"]), Ok(Outcome::Print(s)) if s.contains("--pull-request")));
        assert_eq!(run(&["debug"]), Ok(Outcome::Debug));
        assert!(run(&["ask", "Dev", "x", "--timeout", "abc"]).unwrap_err().contains("--timeout"));
        assert!(run(&["bogus"]).unwrap_err().contains("Unknown command"));
        assert!(run(&["floor", "create", "X", "--bogus"]).unwrap_err().contains("unknown flag"));
        assert!(run(&["portal", "create", "--simulator", "Pixel_7"]).unwrap_err().contains("unknown flag --simulator"));
    }

    #[test]
    fn every_command_has_help() {
        for t in ["ask", "check", "note", "list", "connect", "say", "recall", "portal", "recruit", "dismiss", "role",
                  "preset", "notify", "workspace", "floor", "routine", "debug"] {
            assert!(help::topic(t).is_some(), "{}", t);
        }
    }
}

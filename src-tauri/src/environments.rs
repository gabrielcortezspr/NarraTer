//! Remote workspaces: a workspace can point at an SSH server or a running
//! Docker container, and its agents run there (Maestri's SSH workspaces and
//! Docker containers). Like Maestri:
//!
//! - nothing is installed as a daemon: OpenSSH (key-based, non-interactive)
//!   or `docker exec` carry everything;
//! - the remote side gets the skills, the role files and a small `narrater`
//!   wrapper (sh + curl) that posts its command line to the app's bridge —
//!   through a reverse tunnel (SSH) or the container's network gateway;
//! - an optional persistent session keeps the agent alive in tmux when the
//!   app quits or the connection drops; reconnecting reattaches to it.
//!
//! The container is never created, started, stopped or removed here, and no
//! mounts are added: the project must already be inside it.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Where a workspace's terminals run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Environment {
    Local,
    Ssh {
        host: String,
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        user: Option<String>,
        /// Keep agents alive in tmux across disconnects and app restarts
        #[serde(default)]
        persistent: bool,
    },
    Docker {
        container: String,
        #[serde(default)]
        context: Option<String>,
        #[serde(default)]
        user: Option<String>,
        /// Shell used to start things (default /bin/sh)
        #[serde(default)]
        shell: Option<String>,
        #[serde(default)]
        persistent: bool,
    },
}

const SSH_OPTS: &[&str] = &[
    "-o", "BatchMode=yes",
    "-o", "StrictHostKeyChecking=accept-new",
    "-o", "ConnectTimeout=10",
    "-o", "ServerAliveInterval=30",
];

/// The skills and the wrapper every remote home gets.
const SKILL_DIRS: &[&str] = &[".claude/skills", ".agents/skills"];

impl Environment {
    pub fn is_remote(&self) -> bool {
        !matches!(self, Environment::Local)
    }

    pub fn persistent(&self) -> bool {
        match self {
            Environment::Local => false,
            Environment::Ssh { persistent, .. } | Environment::Docker { persistent, .. } => *persistent,
        }
    }

    /// "ssh deploy@build-box", "docker app-dev" — for messages.
    pub fn describe(&self) -> String {
        match self {
            Environment::Local => "this machine".into(),
            Environment::Ssh { host, user, port, .. } => format!(
                "ssh {}{}{}",
                user.as_ref().map_or(String::new(), |u| format!("{}@", u)),
                host,
                port.filter(|p| *p != 22).map_or(String::new(), |p| format!(":{}", p))
            ),
            Environment::Docker { container, context, .. } => format!(
                "docker {}{}",
                container,
                context.as_ref().map_or(String::new(), |c| format!(" (context {})", c))
            ),
        }
    }

    /// Validates what the user typed.
    pub fn validate(&self) -> Result<(), String> {
        let bad = |s: &str| s.trim().is_empty() || s.starts_with('-') || s.chars().any(|c| c.is_whitespace() || c.is_control());
        match self {
            Environment::Local => Ok(()),
            Environment::Ssh { host, user, .. } => {
                if bad(host) {
                    return Err("the SSH host is empty or invalid".into());
                }
                if user.as_deref().is_some_and(|u| !u.is_empty() && bad(u)) {
                    return Err("the SSH user is invalid".into());
                }
                Ok(())
            }
            Environment::Docker { container, context, user, shell, .. } => {
                if bad(container) {
                    return Err("pick a running container".into());
                }
                for (what, v) in [("context", context), ("user", user), ("shell", shell)] {
                    if v.as_deref().is_some_and(|x| !x.is_empty() && bad(x)) {
                        return Err(format!("the Docker {} is invalid", what));
                    }
                }
                Ok(())
            }
        }
    }

    fn ssh_target(&self) -> Vec<String> {
        let Environment::Ssh { host, port, user, .. } = self else { return vec![] };
        let mut v = Vec::new();
        if let Some(p) = port {
            v.extend(["-p".to_string(), p.to_string()]);
        }
        v.push(match user.as_deref().filter(|u| !u.is_empty()) {
            Some(u) => format!("{}@{}", u, host),
            None => host.clone(),
        });
        v
    }

    fn docker_prefix(&self) -> Vec<String> {
        let Environment::Docker { context, .. } = self else { return vec![] };
        let mut v = vec!["docker".to_string()];
        if let Some(c) = context.as_deref().filter(|c| !c.is_empty()) {
            v.extend(["--context".to_string(), c.to_string()]);
        }
        v
    }

    /// `docker exec` flags for the configured user.
    fn docker_exec(&self, tty: bool) -> Vec<String> {
        let Environment::Docker { container, user, .. } = self else { return vec![] };
        let mut v = self.docker_prefix();
        v.push("exec".into());
        v.push(if tty { "-it" } else { "-i" }.into());
        if let Some(u) = user.as_deref().filter(|u| !u.is_empty()) {
            v.extend(["-u".to_string(), u.to_string()]);
        }
        v.push(container.clone());
        v
    }

    fn shell(&self) -> String {
        match self {
            Environment::Docker { shell: Some(s), .. } if !s.is_empty() => s.clone(),
            _ => "/bin/sh".into(),
        }
    }

    /// Cache key: what makes two environments the same place.
    fn key(&self) -> String {
        serde_json::to_string(&match self {
            Environment::Ssh { host, port, user, .. } => serde_json::json!(["ssh", host, port, user]),
            Environment::Docker { container, context, user, .. } => serde_json::json!(["docker", container, context, user]),
            Environment::Local => serde_json::json!(["local"]),
        })
        .unwrap_or_default()
    }
}

/// POSIX single-quoting.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A remote path as a shell word: `~/x` keeps expanding to the remote home.
fn sh_path(p: &str) -> String {
    if p == "~" {
        "\"$HOME\"".into()
    } else if let Some(rest) = p.strip_prefix("~/") {
        format!("\"$HOME\"/{}", sh_quote(rest))
    } else {
        sh_quote(p)
    }
}

/// `~` expanded against the remote home.
fn expand_remote(p: &str, home: &str) -> String {
    if p == "~" {
        home.to_string()
    } else if let Some(rest) = p.strip_prefix("~/") {
        format!("{}/{}", home.trim_end_matches('/'), rest)
    } else {
        p.to_string()
    }
}

/// A heredoc writing `content` to `path` (a shell word), with a delimiter
/// the content can't contain.
fn write_file(path: &str, content: &str) -> String {
    let mut delim = "NARRATER_EOF".to_string();
    while content.contains(&delim) {
        delim.push('_');
    }
    format!("cat > {} <<'{}'\n{}\n{}\n", path, delim, content.trim_end_matches('\n'), delim)
}

/// Runs a shell script in the environment (read by `sh -s` from stdin, so
/// any size and any login shell works). Returns stdout (raw bytes).
pub fn run_bytes(env: &Environment, script: &str) -> Result<Vec<u8>, String> {
    let mut cmd = match env {
        Environment::Local => {
            let mut c = Command::new("sh");
            c.arg("-s");
            c
        }
        Environment::Ssh { .. } => {
            let mut c = Command::new("ssh");
            c.args(SSH_OPTS).args(env.ssh_target()).arg("sh -s");
            c
        }
        Environment::Docker { .. } => {
            let argv = env.docker_exec(false);
            let mut c = Command::new(&argv[0]);
            c.args(&argv[1..]).args([env.shell().as_str(), "-s"]);
            c
        }
    };
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match env {
            Environment::Ssh { .. } => format!("ssh isn't available: {}", e),
            Environment::Docker { .. } => format!("the Docker CLI isn't available: {}", e),
            Environment::Local => e.to_string(),
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(script.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(match (env, out.status.code()) {
            (Environment::Ssh { .. }, Some(255)) => format!(
                "can't connect to {} — Maestri-style SSH needs key-based, non-interactive login (try `ssh {}` in a terminal): {}",
                env.describe(),
                env.ssh_target().join(" "),
                err
            ),
            _ if err.is_empty() => format!("failed with status {}", out.status.code().unwrap_or(-1)),
            _ => err,
        })
    }
}

pub fn run(env: &Environment, script: &str) -> Result<String, String> {
    run_bytes(env, script).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// What setup learned about a remote environment.
#[derive(Serialize, Clone, Debug, Default)]
pub struct RemoteInfo {
    pub home: String,
    pub user: String,
    pub host: String,
    pub has_bash: bool,
    pub has_curl: bool,
    pub has_tmux: bool,
}

static SETUP: Mutex<Option<HashMap<String, RemoteInfo>>> = Mutex::new(None);

/// Our tmux settings (local and remote sessions run on their own tmux
/// server): no status bar, no ESC delay for the agents' TUIs, mouse wheel
/// scrolls the history.
pub const TMUX_CONF: &str = "set -g status off\nset -g escape-time 0\nset -g mouse on\nset -g history-limit 50000\n";

/// The wrapper agents call as `narrater` in a remote workspace.
pub const WRAPPER: &str = include_str!("../assets/narrater-remote.sh");

/// Installs the wrapper and the skills in the remote home (once per app run
/// and environment, unless `force`) and checks the requirements.
pub fn ensure_setup(env: &Environment, skills: &[(&str, &str)], force: bool) -> Result<RemoteInfo, String> {
    let key = env.key();
    if !force {
        if let Some(info) = SETUP.lock().unwrap().as_ref().and_then(|m| m.get(&key)) {
            return Ok(info.clone());
        }
    }
    let mut script = String::from("set -e\nmkdir -p \"$HOME/.narrater/bin\" \"$HOME/.narrater/launch\"\n");
    script.push_str(&write_file("\"$HOME/.narrater/bin/narrater\"", WRAPPER));
    script.push_str("chmod +x \"$HOME/.narrater/bin/narrater\"\n");
    script.push_str(&write_file("\"$HOME/.narrater/tmux.conf\"", TMUX_CONF));
    for base in SKILL_DIRS {
        for (name, content) in skills {
            let dir = format!("\"$HOME\"/{}", sh_quote(&format!("{}/{}", base, name)));
            script.push_str(&format!("mkdir -p {}\n", dir));
            script.push_str(&write_file(&format!("{}/SKILL.md", dir), content));
        }
    }
    script.push_str(
        "printf 'home=%s\\nuser=%s\\nhost=%s\\n' \"$HOME\" \"$(id -un 2>/dev/null || whoami)\" \"$(hostname 2>/dev/null || uname -n)\"\n\
         for c in bash curl tmux; do command -v \"$c\" >/dev/null 2>&1 && printf 'has=%s\\n' \"$c\"; done; true\n",
    );
    let out = run(env, &script)?;
    let mut info = RemoteInfo::default();
    for line in out.lines() {
        match line.split_once('=') {
            Some(("home", v)) => info.home = v.to_string(),
            Some(("user", v)) => info.user = v.to_string(),
            Some(("host", v)) => info.host = v.to_string(),
            Some(("has", "bash")) => info.has_bash = true,
            Some(("has", "curl")) => info.has_curl = true,
            Some(("has", "tmux")) => info.has_tmux = true,
            _ => {}
        }
    }
    if info.home.is_empty() {
        return Err(format!("{} didn't report a home directory — is it writable?", env.describe()));
    }
    if !info.has_curl {
        return Err(format!("{} has no curl — the narrater CLI needs it there (and /bin/sh, bash)", env.describe()));
    }
    SETUP.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, info.clone());
    Ok(info)
}

/// Checks the environment end to end (connection, home, curl, bash, tmux).
pub fn test(env: &Environment, skills: &[(&str, &str)]) -> Result<String, String> {
    env.validate()?;
    let info = ensure_setup(env, skills, true)?;
    let tick = |b: bool| if b { "✓" } else { "✗" };
    let mut msg = format!(
        "connected to {} as {} (home {}) · curl {} · bash {} · tmux {}",
        if info.host.is_empty() { env.describe() } else { info.host.clone() },
        info.user,
        info.home,
        tick(info.has_curl),
        tick(info.has_bash),
        tick(info.has_tmux)
    );
    if env.persistent() && !info.has_tmux {
        msg.push_str(" — persistent sessions need tmux there; terminals will start without one");
    }
    Ok(msg)
}

/// Checks the way back: from the environment, the CLI must reach the bridge
/// (`bridge`: `unix:` path the SSH tunnel will create, or `tcp:host:port`).
pub fn probe_bridge(env: &Environment, bridge: &str, port: u16) -> Result<(), String> {
    let out = match (env, bridge.strip_prefix("unix:")) {
        (Environment::Ssh { .. }, Some(sock)) => {
            let out = Command::new("ssh")
                .args(SSH_OPTS)
                .args(["-o", "ExitOnForwardFailure=yes", "-R", &format!("{}:127.0.0.1:{}", sock, port)])
                .args(env.ssh_target())
                .arg(format!("curl -s --max-time 5 --unix-socket {} http://narrater/ping; rm -f {}", sock, sock))
                .output()
                .map_err(|e| format!("ssh isn't available: {}", e))?;
            let err = String::from_utf8_lossy(&out.stderr);
            if err.contains("forwarding failed") {
                return Err(format!(
                    "{} refuses the reverse tunnel the narrater CLI needs to reach NarraTer — enable \
                     `AllowTcpForwarding yes` and `AllowStreamLocalForwarding yes` in its sshd_config",
                    env.describe()
                ));
            }
            String::from_utf8_lossy(&out.stdout).into_owned()
        }
        _ => {
            let addr = bridge.strip_prefix("tcp:").unwrap_or(bridge);
            run(env, &format!("curl -s --max-time 5 http://{}/ping\n", addr)).unwrap_or_default()
        }
    };
    if out.trim() == "ok" {
        Ok(())
    } else {
        Err(format!(
            "{} can't reach NarraTer's bridge ({}) — the narrater CLI wouldn't work there{}",
            env.describe(),
            bridge,
            if matches!(env, Environment::Docker { .. }) { "; check that the container's network can reach the host" } else { "" }
        ))
    }
}

/// Removes what setup installed (the workspace stopped pointing here).
pub fn cleanup(env: &Environment, skills: &[(&str, &str)]) -> Result<(), String> {
    let mut script = String::from("rm -rf \"$HOME/.narrater/bin\" \"$HOME/.narrater/launch\"\n");
    for base in SKILL_DIRS {
        for (name, _) in skills {
            script.push_str(&format!("rm -rf \"$HOME\"/{}\n", sh_quote(&format!("{}/{}", base, name))));
        }
    }
    SETUP.lock().unwrap().get_or_insert_with(HashMap::new).remove(&env.key());
    run(env, &script).map(|_| ())
}

/// Everything a remote terminal needs to start.
pub struct Launch<'a> {
    pub id: &'a str,
    pub token: &'a str,
    pub label: &'a str,
    pub scene: &'a str,
    /// The project directory, in remote terms (may start with `~`)
    pub cwd: Option<&'a str>,
    pub role: Option<&'a crate::pty::RoleSpec>,
    pub program: &'a str,
    pub args: &'a [String],
    /// How the remote CLI reaches the bridge: `unix:/path` or `tcp:host:port`
    pub bridge: &'a str,
}

/// A terminal's key in remote file and tmux session names.
pub fn session_key(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// Writes the role files and the launcher for one terminal; returns the
/// project directory (remote, `~` expanded) for routines.
pub fn prepare_launch(env: &Environment, info: &RemoteInfo, l: &Launch) -> Result<String, String> {
    let (script, project) = launch_script(env, info, l)?;
    run(env, &script)?;
    Ok(project)
}

/// The script `prepare_launch` runs remotely, and the project directory.
fn launch_script(env: &Environment, info: &RemoteInfo, l: &Launch) -> Result<(String, String), String> {
    let key = session_key(l.id);
    let project = expand_remote(l.cwd.filter(|c| !c.trim().is_empty()).unwrap_or("~"), &info.home);
    let mut script = String::from("set -e\nmkdir -p \"$HOME/.narrater/launch\"\n");

    // Role: files in <project>/.narrater/roles/<key>/, the agent starts there
    let mut start_dir = sh_path(&project);
    if let Some(role) = l.role.filter(|r| !r.instructions.trim().is_empty()) {
        let (rel, files) = crate::roles::role_files(role, Path::new(&project), l.program)?;
        let dir = format!("{}/{}", project.trim_end_matches('/'), rel);
        script.push_str(&format!("mkdir -p {}\n", sh_quote(&dir)));
        let ignore = format!("{}/.narrater/.gitignore", project.trim_end_matches('/'));
        script.push_str(&format!("[ -e {0} ] || printf '*\\n' > {0}\n", sh_quote(&ignore)));
        for (file, content) in files {
            let path = format!("{}/{}", dir, file);
            if let Some(parent) = Path::new(&path).parent() {
                script.push_str(&format!("mkdir -p {}\n", sh_quote(&parent.display().to_string())));
            }
            script.push_str(&write_file(&sh_quote(&path), &content));
        }
        start_dir = sh_quote(&dir);
    }

    let exports = [
        ("NARRATER_ID", l.id),
        ("NARRATER_TOKEN", l.token),
        ("NARRATER_LABEL", l.label),
        ("NARRATER_SCENE", l.scene),
        ("NARRATER_BRIDGE", l.bridge),
        ("NARRATER_KEY", key.as_str()),
        ("TERM_PROGRAM", "NarraTer"),
        ("COLORTERM", "truecolor"),
    ];
    let mut run_sh = String::new();
    for (k, v) in exports {
        run_sh.push_str(&format!("export {}={}\n", k, sh_quote(v)));
    }
    run_sh.push_str("export TERM=xterm-256color\n");
    run_sh.push_str("export NARRATER_CLI=\"$HOME/.narrater/bin/narrater\"\n");
    run_sh.push_str("export PATH=\"$HOME/.narrater/bin:$HOME/.local/bin:$PATH\"\n");
    run_sh.push_str(&format!("cd {} 2>/dev/null || cd \"$HOME\"\n", start_dir));
    let argv: Vec<String> = std::iter::once(l.program.to_string()).chain(l.args.iter().cloned()).map(|a| sh_quote(&a)).collect();
    run_sh.push_str(&format!("exec {}\n", argv.join(" ")));
    script.push_str(&write_file(&format!("\"$HOME/.narrater/launch/{}.run\"", key), &run_sh));

    // The entry point: through tmux when persistent (a reconnect reattaches,
    // and the new bridge/token are published for the wrapper), plain otherwise
    let mut launch = String::new();
    if env.persistent() {
        launch.push_str(&format!(
            "if command -v tmux >/dev/null 2>&1; then\n\
             \ttmux -L narrater -f \"$HOME/.narrater/tmux.conf\" start-server 2>/dev/null || true\n\
             \ttmux -L narrater set-environment -g NARRATER_BRIDGE_{key} {bridge} 2>/dev/null || true\n\
             \ttmux -L narrater set-environment -g NARRATER_TOKEN_{key} {token} 2>/dev/null || true\n\
             \texec tmux -L narrater -f \"$HOME/.narrater/tmux.conf\" new-session -A -s narrater-{key} \"sh $HOME/.narrater/launch/{key}.run\"\n\
             fi\n",
            key = key,
            bridge = sh_quote(l.bridge),
            token = sh_quote(l.token),
        ));
    }
    launch.push_str(&format!("exec sh \"$HOME/.narrater/launch/{}.run\"\n", key));
    script.push_str(&write_file(&format!("\"$HOME/.narrater/launch/{}.sh\"", key), &launch));
    Ok((script, project))
}

/// The local command whose PTY becomes the remote terminal.
pub fn pty_command(env: &Environment, id: &str, remote_socket: Option<&str>, bridge_port: u16) -> Vec<String> {
    let key = session_key(id);
    match env {
        Environment::Local => vec![],
        Environment::Ssh { .. } => {
            let mut v = vec!["ssh".to_string(), "-tt".to_string()];
            v.extend(SSH_OPTS.iter().map(|s| s.to_string()));
            v.extend(["-o".into(), "ExitOnForwardFailure=yes".into()]);
            if let Some(sock) = remote_socket {
                v.extend(["-R".into(), format!("{}:127.0.0.1:{}", sock, bridge_port)]);
            }
            v.extend(env.ssh_target());
            v.push(format!("sh .narrater/launch/{}.sh", key));
            v
        }
        Environment::Docker { .. } => {
            let mut v = env.docker_exec(true);
            v.extend([env.shell(), "-c".into(), format!("exec sh \"$HOME/.narrater/launch/{}.sh\"", key)]);
            v
        }
    }
}

/// The container's gateway on the host (where the bridge listens for it);
/// None with host networking.
pub fn docker_gateway(env: &Environment) -> Result<Option<String>, String> {
    let Environment::Docker { container, .. } = env else { return Ok(None) };
    let mut argv = env.docker_prefix();
    argv.extend([
        "inspect".into(),
        "-f".into(),
        "{{range .NetworkSettings.Networks}}{{.Gateway}} {{end}}|{{.HostConfig.NetworkMode}}".into(),
        container.clone(),
    ]);
    let out = Command::new(&argv[0]).args(&argv[1..]).output().map_err(|e| format!("the Docker CLI isn't available: {}", e))?;
    if !out.status.success() {
        return Err(format!("container '{}': {}", container, String::from_utf8_lossy(&out.stderr).trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (gateways, mode) = text.trim().split_once('|').unwrap_or((text.trim(), ""));
    if mode == "host" {
        return Ok(None);
    }
    Ok(gateways.split_whitespace().find(|g| !g.is_empty()).map(str::to_string))
}

/// Hosts named in ~/.ssh/config (no wildcards), for the SSH sheet's picker.
pub fn ssh_hosts() -> Vec<String> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    let text = std::fs::read_to_string(home.join(".ssh/config")).unwrap_or_default();
    let mut out: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter_map(|l| {
            let (k, v) = l.split_once(char::is_whitespace)?;
            k.eq_ignore_ascii_case("host").then_some(v)
        })
        .flat_map(|v| v.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .filter(|h| !h.contains(['*', '?', '!']))
        .collect();
    out.dedup();
    out
}

#[derive(Serialize, Debug, PartialEq)]
pub struct ContainerInfo {
    pub name: String,
    pub image: String,
    pub status: String,
}

/// Running containers in a Docker context, for the Docker sheet's picker.
pub fn docker_containers(context: Option<&str>) -> Result<Vec<ContainerInfo>, String> {
    let mut cmd = Command::new("docker");
    if let Some(c) = context.filter(|c| !c.is_empty()) {
        cmd.args(["--context", c]);
    }
    let out = cmd
        .args(["ps", "--format", "{{.Names}}\t{{.Image}}\t{{.Status}}"])
        .output()
        .map_err(|e| format!("the Docker CLI isn't available: {}", e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            Some(ContainerInfo {
                name: it.next()?.to_string(),
                image: it.next().unwrap_or_default().to_string(),
                status: it.next().unwrap_or_default().to_string(),
            })
        })
        .collect())
}

pub fn docker_contexts() -> Vec<String> {
    Command::new("docker")
        .args(["context", "ls", "--format", "{{.Name}}"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Persistent sessions running in the environment (`narrater-<key>`).
pub fn sessions(env: &Environment) -> Result<Vec<String>, String> {
    let out = run(env, "tmux -L narrater ls -F '#{session_name}' 2>/dev/null || true\n")?;
    Ok(out.lines().filter(|l| l.starts_with("narrater-")).map(str::to_string).collect())
}

/// Ends persistent sessions (all of them with `name` None).
pub fn end_sessions(env: &Environment, name: Option<&str>) -> Result<(), String> {
    let script = match name {
        Some(n) => format!("tmux -L narrater kill-session -t {} 2>/dev/null || true\n", sh_quote(n)),
        None => "tmux -L narrater kill-server 2>/dev/null || true\n".to_string(),
    };
    run(env, &script).map(|_| ())
}

/// Runs a routine's pre-run script in a remote project; its stdout, or Err
/// on a non-zero exit (the fire is skipped).
pub fn pre_run(env: &Environment, script: &str, cwd: &str, vars: &[(&str, String)]) -> Result<String, String> {
    let mut s = format!("cd {} || exit 1\n", sh_path(cwd));
    for (k, v) in vars {
        s.push_str(&format!("export {}={}\n", k, sh_quote(v)));
    }
    s.push_str(script);
    s.push('\n');
    run(env, &s).map(|o| o.trim_end().to_string())
}

/// Fetches a file from the environment into a local temp file (attachments
/// a remote agent sends with `narrater say --file/--attach`).
pub fn fetch(env: &Environment, remote: &str) -> Result<std::path::PathBuf, String> {
    let bytes = run_bytes(env, &format!("cat -- {}\n", sh_path(remote)))
        .map_err(|e| format!("can't read {} on {}: {}", remote, env.describe(), e))?;
    let dir = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("narrater/remote-drops");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = Path::new(remote).file_name().map_or("file".into(), |n| n.to_string_lossy().into_owned());
    let path = dir.join(format!("{}-{}", uuid::Uuid::new_v4().simple(), name));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh() -> Environment {
        Environment::Ssh { host: "box".into(), port: Some(2222), user: Some("dev".into()), persistent: true }
    }

    #[test]
    fn serializes_with_a_kind_tag() {
        let v = serde_json::to_value(ssh()).unwrap();
        assert_eq!(v["kind"], "ssh");
        let back: Environment = serde_json::from_value(serde_json::json!({ "kind": "docker", "container": "app" })).unwrap();
        assert_eq!(back, Environment::Docker { container: "app".into(), context: None, user: None, shell: None, persistent: false });
        assert!(serde_json::from_value::<Environment>(serde_json::json!({ "kind": "local" })).unwrap() == Environment::Local);
    }

    #[test]
    fn validates_and_describes() {
        assert!(ssh().validate().is_ok());
        assert!(Environment::Ssh { host: "-oProxyCommand=x".into(), port: None, user: None, persistent: false }.validate().is_err());
        assert!(Environment::Ssh { host: "a b".into(), port: None, user: None, persistent: false }.validate().is_err());
        assert_eq!(ssh().describe(), "ssh dev@box:2222");
    }

    #[test]
    fn quoting_and_paths() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sh_path("~/proj x"), "\"$HOME\"/'proj x'");
        assert_eq!(sh_path("/srv/app"), "'/srv/app'");
        assert_eq!(expand_remote("~/p", "/home/dev"), "/home/dev/p");
        let w = write_file("f", "a\nNARRATER_EOF\nb");
        assert!(w.contains("<<'NARRATER_EOF_'"), "{}", w);
    }

    #[test]
    fn pty_commands() {
        let v = pty_command(&ssh(), "terminal-ab12", Some("/tmp/n.sock"), 7433);
        assert_eq!(v[0], "ssh");
        assert!(v.contains(&"-tt".to_string()));
        assert!(v.windows(2).any(|w| w[0] == "-R" && w[1] == "/tmp/n.sock:127.0.0.1:7433"));
        assert!(v.windows(2).any(|w| w[0] == "-p" && w[1] == "2222"));
        assert!(v.contains(&"dev@box".to_string()));
        assert_eq!(v.last().unwrap(), "sh .narrater/launch/terminal_ab12.sh");
        let d = Environment::Docker { container: "app".into(), context: Some("remote".into()), user: Some("node".into()), shell: None, persistent: false };
        let v = pty_command(&d, "t-1", None, 7433);
        assert_eq!(v[..7], ["docker", "--context", "remote", "exec", "-it", "-u", "node"].map(String::from));
        assert_eq!(v[7], "app");
        assert_eq!(v[v.len() - 1], "exec sh \"$HOME/.narrater/launch/t_1.sh\"");
    }

    /// The whole remote flow, with this machine standing in for the remote
    /// (Environment::Local runs the same scripts through `sh -s`).
    #[test]
    fn setup_and_launch_scripts_run() {
        let home = std::env::temp_dir().join(format!("narrater-envtest-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let script = format!("export HOME={}\n", sh_quote(&home.display().to_string()));
        // Setup's script, run with HOME pointed at the temp dir
        let env = Environment::Local;
        let skills = [("narrater", "# skill")];
        let mut setup = String::from("set -e\nmkdir -p \"$HOME/.narrater/bin\"\n");
        setup.push_str(&write_file("\"$HOME/.narrater/bin/narrater\"", WRAPPER));
        run(&env, &format!("{}{}", script, setup)).unwrap();
        assert!(home.join(".narrater/bin/narrater").exists());
        let _ = skills;

        let info = RemoteInfo { home: home.display().to_string(), ..Default::default() };
        let role = crate::pty::RoleSpec { key: "rev".into(), name: Some("Reviewer".into()), instructions: "Review.".into(), orchestrator: false };
        let args = vec!["-c".to_string(), "echo \"$NARRATER_LABEL in $(basename \"$PWD\")\"".to_string()];
        let launch = Launch {
            id: "t-1", token: "tok", label: "Dev's", scene: "S", cwd: Some("~/proj"), role: Some(&role),
            program: "sh", args: &args, bridge: "tcp:127.0.0.1:1",
        };
        // The launch script writes under $HOME — run it with the temp HOME
        let (launch_sh, project) = launch_script(&env, &info, &launch).unwrap();
        run(&env, &format!("{}{}", script, launch_sh)).unwrap();
        assert_eq!(project, format!("{}/proj", home.display()));
        assert!(home.join("proj/.narrater/roles/rev/CLAUDE.md").exists());
        let out = Command::new("sh").arg(home.join(".narrater/launch/t_1.sh")).env("HOME", &home).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Dev's in rev");
        let _ = std::fs::remove_dir_all(&home);
    }
}

// ---- Local persistent sessions ----------------------------------------------

/// The tmux server local agents run on (separate from the user's own).
pub const LOCAL_TMUX_SOCKET: &str = "narrater-local";

/// tmux on this machine, if installed (looked up once per run).
pub fn local_tmux() -> Option<std::path::PathBuf> {
    static FOUND: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths).map(|d| d.join("tmux")).find(|p| p.is_file())
            })
        })
        .clone()
}

fn local_dir() -> std::path::PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("narrater")
}

/// Our tmux config file (written when it changed).
pub fn local_tmux_conf() -> std::path::PathBuf {
    let path = local_dir().join("tmux.conf");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(TMUX_CONF) {
        let _ = std::fs::create_dir_all(local_dir());
        let _ = std::fs::write(&path, TMUX_CONF);
    }
    path
}

/// Writes the script a persistent local terminal runs inside tmux: its
/// identity, directory and agent command.
pub fn local_launch(key: &str, exports: &[(&str, &str)], dir: &Path, argv: &[String]) -> Result<std::path::PathBuf, String> {
    let launch = local_dir().join("launch");
    std::fs::create_dir_all(&launch).map_err(|e| e.to_string())?;
    let mut run = String::new();
    for (k, v) in exports {
        run.push_str(&format!("export {}={}\n", k, sh_quote(v)));
    }
    run.push_str(&format!("cd {} 2>/dev/null || cd \"$HOME\"\n", sh_quote(&dir.display().to_string())));
    let argv: Vec<String> = argv.iter().map(|a| sh_quote(a)).collect();
    run.push_str(&format!("exec {}\n", argv.join(" ")));
    let path = launch.join(format!("{}.run", key));
    std::fs::write(&path, run).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
    Ok(path)
}

/// Runs a tmux command on our local server (best effort).
fn local_tmux_cmd(tmux: &Path, args: &[&str]) -> bool {
    Command::new(tmux)
        .args(["-L", LOCAL_TMUX_SOCKET])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A reattached shell keeps its old environment: the app's current socket
/// and the terminal's new token are published for the CLI to prefer.
pub fn local_publish(tmux: &Path, key: &str, socket: &str, token: &str) {
    local_tmux_cmd(tmux, &["set-environment", "-g", &format!("NARRATER_SOCKET_{}", key), socket]);
    local_tmux_cmd(tmux, &["set-environment", "-g", &format!("NARRATER_TOKEN_{}", key), token]);
}

/// Ends a local persistent session (its terminal was removed for good).
pub fn local_kill(tmux: &Path, session: &str) {
    local_tmux_cmd(tmux, &["kill-session", "-t", session]);
}

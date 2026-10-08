//! Floors with git isolation: a floor of a scene can be backed by its own
//! checkout of the project, so agents there work without touching the
//! ground checkout. Two kinds, like Maestri:
//!
//! - a **git worktree** on its own branch (the default — cheap, and the
//!   branch lives in the project from the start);
//! - a **copy** (`--copy`): a full clone of the repository made instantly by
//!   the filesystem (reflinks on btrfs/XFS — Maestri's APFS clonefile), so it
//!   can check out the very branch Ground has; landing fetches its work back.
//!
//! A floor can also be made from an open pull/merge request (`--pull-request`),
//! asking the project's host through its CLI (`gh`, `glab`).
//!
//! Checkouts live outside the project, in ~/.local/share/narrater/floors/.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PullRequestInfo {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// "github" | "gitlab"
    pub provider: String,
    /// The floor has the pull request's own branch (pushes reach it); false
    /// for a fork, where the floor has a copy of its commits (`pr-N`)
    pub same_repo: bool,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct FloorInfo {
    /// Where the floor's terminals work (None: shares the ground directory)
    pub dir: Option<String>,
    /// The checkout root: a worktree, or a copy of the repository
    pub worktree: Option<String>,
    pub branch: Option<String>,
    /// The checkout is a full copy of the repository, not a worktree
    pub copy: bool,
    pub pull_request: Option<PullRequestInfo>,
    /// Default floor name ("#42 Title" for a pull request)
    pub name: Option<String>,
    /// Why the floor isn't isolated, when it isn't
    pub note: Option<String>,
}

impl FloorInfo {
    fn plain(why: &str) -> Self {
        FloorInfo { dir: None, worktree: None, branch: None, copy: false, pull_request: None, name: None, note: Some(why.to_string()) }
    }
}

#[derive(Serialize, Debug)]
pub struct FloorStatus {
    /// Uncommitted changes in the checkout
    pub dirty: bool,
    pub branch: Option<String>,
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() || c == '/' || c == '.' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches(|c| c == '-' || c == '/' || c == '.').to_string()
}

fn expand(dir: &str) -> PathBuf {
    match (dir.strip_prefix("~"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(dir),
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git is not available: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Like `git`, but keeps stdout on failure (merge-tree reports conflicts
/// with exit status 1 and the file list on stdout).
fn git_status(dir: &Path, args: &[&str]) -> Result<(bool, String), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git is not available: {}", e))?;
    Ok((out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_string()))
}

fn floors_root() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("narrater/floors")
}

/// A floor's checkout is a copy of the repository (its own `.git`
/// directory) rather than a worktree (a `.git` file pointing back).
fn is_copy(checkout: &Path) -> bool {
    checkout.join(".git").is_dir()
}

/// The project's repository: toplevel of the scene's directory, with commits.
fn project_repo(root: Option<&str>) -> Result<(PathBuf, PathBuf), &'static str> {
    let Some(root) = root.map(expand).filter(|r| r.is_dir()) else {
        return Err("this scene has no working directory (set one in the sidebar, or `narrater workspace create --dir`)");
    };
    let Ok(top) = git(&root, &["rev-parse", "--show-toplevel"]).map(PathBuf::from) else {
        return Err("the scene's directory is not a git repository");
    };
    if git(&top, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_err() {
        return Err("the repository has no commits yet");
    }
    Ok((root, top))
}

/// A free directory for a floor's checkout: floors/<scene>/<name>[-N].
fn checkout_path(scene: &str, name: &str) -> Result<PathBuf, String> {
    let base = floors_root().join(slug(scene).replace('/', "-"));
    let leaf = match slug(name).replace('/', "-") {
        l if l.is_empty() => "floor".to_string(),
        l => l,
    };
    let mut path = base.join(&leaf);
    let mut n = 2;
    while path.exists() {
        path = base.join(format!("{}-{}", leaf, n));
        n += 1;
    }
    std::fs::create_dir_all(&base).map_err(|e| format!("cannot create {}: {}", base.display(), e))?;
    Ok(path)
}

/// How `--copy` clones: "always" refuses a disk that can't clone instantly
/// (tests use "auto", which falls back to a plain copy).
const REFLINK: &str = "always";

/// Copies the whole repository (working tree, untracked files, .git) to
/// `dest`, exactly as it is right now — only where the filesystem can do it
/// instantly. The copy forgets the project's worktrees: those are Ground's.
fn copy_repo(top: &Path, dest: &Path, reflink: &str) -> Result<(), String> {
    let out = Command::new("cp")
        .arg("-a")
        .arg(format!("--reflink={}", reflink))
        .arg(top)
        .arg(dest)
        .output()
        .map_err(|e| format!("cp is not available: {}", e))?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(dest);
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(if err.contains("clone") || err.contains("reflink") || err.contains("not supported") {
            "--copy needs a disk that clones instantly (btrfs, XFS or bcachefs reflinks) and this one can't — \
             drop --copy to get a worktree floor"
                .to_string()
        } else {
            format!("cannot copy the repository: {}", err.trim())
        });
    }
    let _ = std::fs::remove_dir_all(dest.join(".git/worktrees"));
    Ok(())
}

/// The floor's checkout: a worktree of `branch` (created from HEAD when
/// `create`), or a copy of the repository with `branch` checked out.
fn make_checkout(top: &Path, path: &Path, branch: &str, create: bool, copy: bool, reflink: &str) -> Result<(), String> {
    let p = path.display().to_string();
    if copy {
        copy_repo(top, path, reflink)?;
        let args: &[&str] = if create { &["checkout", "-q", "-b", branch] } else { &["checkout", "-q", branch] };
        return git(path, args).map(|_| ()).map_err(|e| {
            let _ = std::fs::remove_dir_all(path);
            format!("cannot check out '{}' in the copy: {}", branch, e)
        });
    }
    let r = if create { git(top, &["worktree", "add", "-b", branch, &p]) } else { git(top, &["worktree", "add", &p, branch]) };
    r.map(|_| ()).map_err(|e| {
        if e.contains("is already used by worktree") || e.contains("is already checked out") {
            format!(
                "branch '{}' is already checked out in another worktree (a floor or the ground checkout) — \
                 pick another --branch, or add --copy for a copy that can share it",
                branch
            )
        } else {
            format!("git worktree add failed: {}", e)
        }
    })
}

/// A scene rooted in a subdirectory of the repo works in that same
/// subdirectory of the floor's checkout.
fn floor_dir(root: &Path, top: &Path, checkout: &Path) -> PathBuf {
    let rel = root
        .canonicalize()
        .ok()
        .and_then(|r| top.canonicalize().ok().and_then(|t| r.strip_prefix(&t).ok().map(Path::to_path_buf)));
    rel.filter(|r| !r.as_os_str().is_empty()).map_or(checkout.to_path_buf(), |r| checkout.join(r))
}

/// Prepares a floor for scene `scene` rooted at `root`. Isolated (worktree,
/// or a copy with `copy`) when the root is in a git repository and `no_git`
/// is off; otherwise a plain floor sharing the ground directory, with the
/// reason.
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    root: Option<&str>,
    scene: &str,
    name: &str,
    branch: Option<&str>,
    existing_branch: bool,
    no_git: bool,
    copy: bool,
    reflink: &str,
) -> Result<FloorInfo, String> {
    if no_git {
        if copy {
            return Err("--copy and --no-git don't go together".into());
        }
        return Ok(FloorInfo::plain("plain floor (--no-git): it shares the ground directory"));
    }
    if let Some(env) = crate::workspaces::environment_of(scene) {
        if copy {
            return Err(format!("--copy isn't available in a remote workspace ({})", env.describe()));
        }
        return Ok(FloorInfo::plain(&format!(
            "plain floor: this workspace runs on {} — floors there share its directory",
            env.describe()
        )));
    }
    let (root, top) = match project_repo(root) {
        Ok(r) => r,
        Err(why) if copy => return Err(format!("--copy needs a git repository: {}", why)),
        Err(why) => return Ok(FloorInfo::plain(&format!("plain floor: {}", why))),
    };

    let branch = branch.map(str::to_string).unwrap_or_else(|| slug(name));
    if branch.is_empty() {
        return Err("the floor needs a name that makes a valid branch".into());
    }
    let exists = git(&top, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{}", branch)]).is_ok();
    match (existing_branch, exists) {
        (true, false) => return Err(format!("branch '{}' doesn't exist (drop --existing-branch to create it)", branch)),
        (false, true) => return Err(format!("branch '{}' already exists — pass --existing-branch to use it, or --branch NAME", branch)),
        _ => {}
    }

    let path = checkout_path(scene, name)?;
    make_checkout(&top, &path, &branch, !existing_branch, copy, reflink)?;
    Ok(FloorInfo {
        dir: Some(floor_dir(&root, &top, &path).display().to_string()),
        worktree: Some(path.display().to_string()),
        branch: Some(branch),
        copy,
        pull_request: None,
        name: None,
        note: None,
    })
}

// ---- Pull requests -------------------------------------------------------

/// What the host says about a pull/merge request.
#[derive(Debug, Clone, PartialEq)]
struct PrData {
    number: u64,
    title: String,
    url: String,
    open: bool,
    head_branch: String,
    head_sha: String,
    same_repo: bool,
    provider: &'static str,
}

/// `42`, `#42`, or a link (…/pull/42, …/-/merge_requests/42) → (number, link).
fn parse_pr_ref(reference: &str) -> Result<(u64, Option<String>), String> {
    let r = reference.trim();
    let bad = || format!("'{}' isn't a pull request — pass its number (42, #42) or its link", reference);
    if let Ok(n) = r.trim_start_matches('#').parse::<u64>() {
        return Ok((n, None));
    }
    if !r.starts_with("http://") && !r.starts_with("https://") {
        return Err(bad());
    }
    let parts: Vec<&str> = r.trim_end_matches('/').split('/').collect();
    let n = parts
        .windows(2)
        .find(|w| w[0] == "pull" || w[0] == "pulls" || w[0] == "merge_requests")
        .and_then(|w| w[1].split(['#', '?']).next()?.parse::<u64>().ok())
        .ok_or_else(bad)?;
    Ok((n, Some(r.to_string())))
}

/// "owner/repo" (or "group/sub/repo") of a remote URL or a pull request link.
fn repo_path_of(url: &str) -> Option<String> {
    let u = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let after_host = if let Some(rest) = u.split("://").nth(1) {
        rest.split_once('/')?.1.to_string()
    } else {
        // scp-like: git@host:owner/repo
        u.split_once(':')?.1.to_string()
    };
    let path = after_host.split("/-/").next()?.split("/pull/").next()?.to_string();
    Some(path.trim_matches('/').to_string()).filter(|p| p.contains('/'))
}

fn remotes(top: &Path) -> Vec<(String, String)> {
    git(top, &["remote", "-v"])
        .unwrap_or_default()
        .lines()
        .filter(|l| l.ends_with("(fetch)"))
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.next()?.to_string()))
        })
        .collect()
}

/// The remote the pull request targets: the one whose URL names the same
/// repository as its link, else `origin`, else the first.
fn pr_remote(top: &Path, pr_url: &str) -> Result<String, String> {
    let list = remotes(top);
    let want = repo_path_of(pr_url).map(|p| p.to_lowercase());
    if let Some(want) = want {
        if let Some((name, _)) = list.iter().find(|(_, url)| repo_path_of(url).is_some_and(|p| p.to_lowercase() == want)) {
            return Ok(name.clone());
        }
    }
    list.iter()
        .find(|(n, _)| n == "origin")
        .or(list.first())
        .map(|(n, _)| n.clone())
        .ok_or_else(|| "the project has no git remote to fetch the pull request from".to_string())
}

/// Which host CLI answers for this project: from the link's host, else the
/// remotes' hosts. GitHub unless something says GitLab.
fn provider_for(top: &Path, link: Option<&str>) -> &'static str {
    let hosts: Vec<String> = match link {
        Some(l) => vec![l.to_lowercase()],
        None => remotes(top).into_iter().map(|(_, u)| u.to_lowercase()).collect(),
    };
    if hosts.iter().any(|h| h.contains("gitlab")) {
        "gitlab"
    } else {
        "github"
    }
}

/// Runs a host CLI in the project; Err with its own message on failure.
fn host_cli(program: &str, top: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program).current_dir(top).args(args).output().map_err(|e| {
        let cli = Path::new(program).file_name().map_or(program.to_string(), |f| f.to_string_lossy().into_owned());
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "the project's host has no provider that can list pull requests here: `{}` isn't installed \
                 (install it and sign in with `{} auth login`)",
                cli, cli
            )
        } else {
            format!("cannot run {}: {}", cli, e)
        }
    })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let err = if err.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { err };
        Err(err)
    }
}

/// Asks the host about a pull request. `program` is the CLI to run (`gh` /
/// `glab`, or a stub in tests).
fn pr_lookup(top: &Path, provider: &'static str, program: &str, reference: &str) -> Result<PrData, String> {
    let (number, link) = parse_pr_ref(reference)?;
    let target = link.clone().unwrap_or_else(|| number.to_string());
    let s = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    if provider == "gitlab" {
        let out = host_cli(program, top, &["mr", "view", &target, "--output", "json"])
            .map_err(|e| format!("glab couldn't read merge request !{}: {}", number, e))?;
        let v: serde_json::Value = serde_json::from_str(&out).map_err(|e| format!("unexpected answer from glab: {}", e))?;
        return Ok(PrData {
            number: v.get("iid").and_then(|x| x.as_u64()).unwrap_or(number),
            title: s(&v, "title"),
            url: s(&v, "web_url"),
            open: s(&v, "state") == "opened",
            head_branch: s(&v, "source_branch"),
            head_sha: s(&v, "sha"),
            same_repo: v.get("source_project_id") == v.get("target_project_id"),
            provider,
        });
    }
    let out = host_cli(
        program,
        top,
        &["pr", "view", &target, "--json", "number,title,url,state,headRefName,headRefOid,isCrossRepository"],
    )
    .map_err(|e| format!("gh couldn't read pull request #{}: {}", number, e))?;
    let v: serde_json::Value = serde_json::from_str(&out).map_err(|e| format!("unexpected answer from gh: {}", e))?;
    Ok(PrData {
        number: v.get("number").and_then(|x| x.as_u64()).unwrap_or(number),
        title: s(&v, "title"),
        url: s(&v, "url"),
        open: s(&v, "state") == "OPEN",
        head_branch: s(&v, "headRefName"),
        head_sha: s(&v, "headRefOid"),
        same_repo: v.get("isCrossRepository").and_then(|x| x.as_bool()) == Some(false),
        provider,
    })
}

/// Pull requests being made into floors right now ("<repo>#<n>").
static PR_IN_FLIGHT: Mutex<Option<HashSet<String>>> = Mutex::new(None);

struct InFlight(String);

impl InFlight {
    fn claim(key: String) -> Option<Self> {
        let mut guard = PR_IN_FLIGHT.lock().unwrap();
        guard.get_or_insert_with(HashSet::new).insert(key.clone()).then_some(InFlight(key))
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        if let Some(set) = PR_IN_FLIGHT.lock().unwrap().as_mut() {
            set.remove(&self.0);
        }
    }
}

/// Puts the pull request's commits on a local branch and returns its name:
/// its own branch (tracking the remote) when it lives in the project's
/// repository, `pr-N` otherwise. Refuses a local branch that went its own way
/// or pushes somewhere else, and a pull request that moved while fetched.
fn bring_pr_branch(top: &Path, remote: &str, pr: &PrData) -> Result<String, String> {
    let fetched = if pr.same_repo {
        let tracking = format!("refs/remotes/{}/{}", remote, pr.head_branch);
        git(top, &["fetch", "-q", remote, &format!("+refs/heads/{}:{}", pr.head_branch, tracking)])
            .map_err(|e| format!("cannot fetch '{}' from {}: {}", pr.head_branch, remote, e))?;
        rev(top, &tracking)
    } else {
        let pr_ref = if pr.provider == "gitlab" {
            format!("refs/merge-requests/{}/head", pr.number)
        } else {
            format!("refs/pull/{}/head", pr.number)
        };
        git(top, &["fetch", "-q", remote, &pr_ref]).map_err(|e| format!("cannot fetch {} from {}: {}", pr_ref, remote, e))?;
        rev(top, "FETCH_HEAD")
    }
    .ok_or("the fetch brought nothing")?;
    if !pr.head_sha.is_empty() && fetched != pr.head_sha {
        return Err(format!(
            "pull request #{} changed while it was fetched (the host says {}, the fetch got {}) — try again",
            pr.number,
            &pr.head_sha[..pr.head_sha.len().min(10)],
            &fetched[..fetched.len().min(10)]
        ));
    }

    let branch = if pr.same_repo { pr.head_branch.clone() } else { format!("pr-{}", pr.number) };
    let local_ref = format!("refs/heads/{}", branch);
    let Some(local) = rev(top, &local_ref) else {
        if pr.same_repo {
            git(top, &["branch", "-q", "--track", &branch, &format!("{}/{}", remote, pr.head_branch)])?;
        } else {
            git(top, &["branch", "-q", &branch, &fetched])?;
        }
        return Ok(branch);
    };

    if pr.same_repo {
        let up_remote = git(top, &["config", &format!("branch.{}.remote", branch)]).ok();
        let up_merge = git(top, &["config", &format!("branch.{}.merge", branch)]).ok();
        let expected = format!("refs/heads/{}", pr.head_branch);
        if up_remote.is_some() && (up_remote.as_deref() != Some(remote) || up_merge.as_deref() != Some(expected.as_str())) {
            return Err(format!(
                "your local branch '{}' pushes somewhere else ({}/{}) — commits from the floor wouldn't reach the pull request",
                branch,
                up_remote.unwrap_or_default(),
                up_merge.unwrap_or_default().trim_start_matches("refs/heads/")
            ));
        }
    }
    let is_ancestor = |a: &str, b: &str| git(top, &["merge-base", "--is-ancestor", a, b]).is_ok();
    if local == fetched || is_ancestor(&fetched, &local) {
        // Same commit, or local commits on top not pushed yet: keep them
    } else if is_ancestor(&local, &fetched) {
        if checkouts(top)?.iter().any(|(_, b)| b.as_deref() == Some(branch.as_str())) {
            return Err(format!("your local branch '{}' is checked out and behind the pull request — pull it there first", branch));
        }
        git(top, &["update-ref", &local_ref, &fetched, &local])?;
    } else {
        return Err(format!(
            "your local branch '{}' went its own way (it has commits the pull request doesn't, and misses some of its own) — \
             rename or reconcile it first",
            branch
        ));
    }
    if pr.same_repo {
        let _ = git(top, &["branch", "-q", "--set-upstream-to", &format!("{}/{}", remote, pr.head_branch), &branch]);
    }
    Ok(branch)
}

/// Prepares a floor that reviews an open pull request. `taken` lists the
/// pull request numbers already on floors of this scene.
#[allow(clippy::too_many_arguments)]
pub fn prepare_pr(
    root: Option<&str>,
    scene: &str,
    name: Option<&str>,
    reference: &str,
    copy: bool,
    taken: &[u64],
    program: Option<&str>,
    reflink: &str,
) -> Result<FloorInfo, String> {
    if let Some(env) = crate::workspaces::environment_of(scene) {
        return Err(format!("pull request floors need a local checkout — this workspace runs on {}", env.describe()));
    }
    let (root, top) = project_repo(root).map_err(|why| format!("a pull request floor needs a git repository: {}", why))?;
    let (_, link) = parse_pr_ref(reference)?;
    let provider = provider_for(&top, link.as_deref());
    let cli = program.unwrap_or(if provider == "gitlab" { "glab" } else { "gh" });
    let pr = pr_lookup(&top, provider, cli, reference)?;
    if !pr.open {
        return Err(format!("pull request #{} isn't open", pr.number));
    }
    if taken.contains(&pr.number) {
        return Err(format!("pull request #{} is already on a floor", pr.number));
    }
    let _guard = InFlight::claim(format!("{}#{}", top.display(), pr.number))
        .ok_or_else(|| format!("pull request #{} is already being made into a floor", pr.number))?;
    if pr.head_branch.is_empty() && pr.same_repo {
        return Err("the host didn't say which branch the pull request comes from".into());
    }

    let remote = pr_remote(&top, &pr.url)?;
    let branch = bring_pr_branch(&top, &remote, &pr)?;
    let floor_name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let title: String = pr.title.chars().take(40).collect();
            format!("#{} {}", pr.number, title).trim().to_string()
        });
    let path = checkout_path(scene, &floor_name)?;
    make_checkout(&top, &path, &branch, false, copy, reflink).map_err(|e| {
        if e.contains("already checked out") {
            format!("'{}' is checked out on Ground — add --copy to review it on a copy", branch)
        } else {
            e
        }
    })?;
    let note = (!pr.same_repo).then(|| {
        format!(
            "the pull request comes from a fork: the floor has a copy of its commits on '{}' — commits made there can't reach it",
            branch
        )
    });
    Ok(FloorInfo {
        dir: Some(floor_dir(&root, &top, &path).display().to_string()),
        worktree: Some(path.display().to_string()),
        branch: Some(branch),
        copy,
        pull_request: Some(PullRequestInfo {
            number: pr.number,
            title: pr.title,
            url: pr.url,
            provider: pr.provider.to_string(),
            same_repo: pr.same_repo,
        }),
        name: Some(floor_name),
        note,
    })
}

/// Where a pull request floor's pull request stands, in one line:
/// "OPEN · review: APPROVED · checks: 5 passed, 1 failing".
pub fn pr_status(checkout: &Path, pr: &PullRequestInfo, program: Option<&str>) -> Result<String, String> {
    let n = pr.number.to_string();
    if pr.provider == "gitlab" {
        let out = host_cli(program.unwrap_or("glab"), checkout, &["mr", "view", &n, "--output", "json"])?;
        let v: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
        let field = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("?").to_string();
        let pipeline = v.pointer("/head_pipeline/status").and_then(|x| x.as_str()).unwrap_or("none");
        return Ok(format!("{} · merge status: {} · pipeline: {}", field("state"), field("detailed_merge_status"), pipeline));
    }
    let out = host_cli(
        program.unwrap_or("gh"),
        checkout,
        &["pr", "view", &pr.url, "--json", "state,reviewDecision,statusCheckRollup,mergeable"],
    )?;
    let v: serde_json::Value = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    let field = |k: &str| v.get(k).and_then(|x| x.as_str()).filter(|s| !s.is_empty()).unwrap_or("none").to_string();
    let (mut passed, mut failing, mut pending) = (0, 0, 0);
    for c in v.get("statusCheckRollup").and_then(|x| x.as_array()).into_iter().flatten() {
        let verdict = c.get("conclusion").or(c.get("state")).and_then(|x| x.as_str()).unwrap_or("");
        match verdict {
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => passed += 1,
            "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" => failing += 1,
            _ => pending += 1,
        }
    }
    let checks = if passed + failing + pending == 0 {
        "no checks".to_string()
    } else {
        [(passed, "passed"), (failing, "failing"), (pending, "pending")]
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, w)| format!("{} {}", n, w))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Ok(format!("{} · review: {} · checks: {} · mergeable: {}", field("state"), field("reviewDecision"), checks, field("mergeable")))
}

/// Merges the floor's pull request on the host (the user's explicit click).
pub fn pr_merge(checkout: &Path, pr: &PullRequestInfo, program: Option<&str>) -> Result<String, String> {
    if pr.provider == "gitlab" {
        host_cli(program.unwrap_or("glab"), checkout, &["mr", "merge", &pr.number.to_string(), "--yes"])?;
    } else {
        host_cli(program.unwrap_or("gh"), checkout, &["pr", "merge", &pr.url, "--merge"])?;
    }
    Ok(format!("pull request #{} merged on the host", pr.number))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn floor_prepare(
    root: Option<String>,
    scene: String,
    name: String,
    branch: Option<String>,
    existing_branch: bool,
    no_git: bool,
    copy: Option<bool>,
    pull_request: Option<String>,
    taken_prs: Option<Vec<u64>>,
) -> Result<FloorInfo, String> {
    let copy = copy.unwrap_or(false);
    match pull_request.filter(|p| !p.trim().is_empty()) {
        Some(reference) => {
            if branch.is_some() || existing_branch || no_git {
                return Err("--branch, --existing-branch and --no-git don't apply to a pull request floor".into());
            }
            prepare_pr(root.as_deref(), &scene, Some(&name), &reference, copy, &taken_prs.unwrap_or_default(), None, REFLINK)
        }
        None => prepare(root.as_deref(), &scene, &name, branch.as_deref(), existing_branch, no_git, copy, REFLINK),
    }
}

#[tauri::command]
pub fn floor_pr_status(worktree: String, pull_request: PullRequestInfo) -> Result<String, String> {
    pr_status(Path::new(&worktree), &pull_request, None)
}

#[tauri::command]
pub fn floor_pr_merge(worktree: String, pull_request: PullRequestInfo) -> Result<String, String> {
    pr_merge(Path::new(&worktree), &pull_request, None)
}

#[tauri::command]
pub fn floor_status(worktree: String) -> Result<FloorStatus, String> {
    let wt = PathBuf::from(&worktree);
    if !wt.is_dir() {
        return Ok(FloorStatus { dirty: false, branch: None });
    }
    let porcelain = git(&wt, &["status", "--porcelain"])?;
    Ok(FloorStatus {
        dirty: !porcelain.is_empty(),
        branch: git(&wt, &["rev-parse", "--abbrev-ref", "HEAD"]).ok(),
    })
}

/// Removes a floor's checkout directory: `git worktree remove` for a
/// worktree, a plain delete for a copy (only under the floors directory).
fn remove_checkout(checkout: &Path, force: bool) -> Result<(), String> {
    if is_copy(checkout) {
        if !force {
            if let Some(dirty) = uncommitted(checkout, true)? {
                return Err(format!("the copy has uncommitted work ({})", dirty));
            }
        }
        let root = floors_root();
        let inside = checkout.canonicalize().ok().zip(root.canonicalize().ok()).is_some_and(|(c, r)| c.starts_with(r));
        if !inside {
            return Err(format!("refusing to delete {}: it isn't under {}", checkout.display(), root.display()));
        }
        return std::fs::remove_dir_all(checkout).map_err(|e| e.to_string());
    }
    let wt = checkout.display().to_string();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.extend(["--force", "--force"]);
    }
    args.push(&wt);
    git(checkout, &args).map(|_| ())
}

/// Removes a floor's checkout (the branch and its commits stay in the repo).
/// Refuses a dirty checkout unless `force`.
#[tauri::command]
pub fn floor_remove(worktree: String, force: bool) -> Result<(), String> {
    let wt = PathBuf::from(&worktree);
    if !wt.exists() {
        return Ok(());
    }
    remove_checkout(&wt, force).map_err(|e| format!("cannot remove the floor's checkout: {}", e))
}

/// Uncommitted work in a checkout, as "N file(s): a, b, c…" (None: clean).
/// `untracked` counts new files too (they'd be lost with a floor's checkout).
fn uncommitted(dir: &Path, untracked: bool) -> Result<Option<String>, String> {
    let flag = if untracked { "--untracked-files=normal" } else { "--untracked-files=no" };
    let out = git(dir, &["status", "--porcelain", flag])?;
    let files: Vec<&str> = out.lines().map(|l| l.get(3..).unwrap_or(l)).collect();
    if files.is_empty() {
        return Ok(None);
    }
    let shown = files.iter().take(5).copied().collect::<Vec<_>>().join(", ");
    let more = if files.len() > 5 { ", …" } else { "" };
    Ok(Some(format!("{} file(s): {}{}", files.len(), shown, more)))
}

/// Checkouts of the repository: (path, branch) per worktree, the main
/// checkout (the ground floor) first.
fn checkouts(dir: &Path) -> Result<Vec<(PathBuf, Option<String>)>, String> {
    let out = git(dir, &["worktree", "list", "--porcelain"])?;
    let mut list = Vec::new();
    for block in out.split("\n\n") {
        let mut path = None;
        let mut branch = None;
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(p));
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                branch = Some(b.to_string());
            }
        }
        if let Some(p) = path {
            list.push((p, branch));
        }
    }
    Ok(list)
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn rev(dir: &Path, refname: &str) -> Option<String> {
    git(dir, &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", refname)]).ok()
}

/// Merges commit `tip` (named `what` in messages) into branch `into`.
/// Refuses, changing nothing, on a conflict (checked first with
/// `merge-tree`) or when `into` is checked out on a floor. In the ground
/// checkout it's a regular `git merge`; a branch checked out nowhere is moved
/// by ref (fast-forward or a merge commit).
fn merge_commit_into(ground: &Path, tip: &str, what: &str, into: &str) -> Result<String, String> {
    let Some(old) = rev(ground, &format!("refs/heads/{}", into)) else {
        return Err(format!("branch '{}' doesn't exist", into));
    };
    let is_ancestor = |a: &str, b: &str| git(ground, &["merge-base", "--is-ancestor", a, b]).is_ok();
    if is_ancestor(tip, &old) {
        return Ok(format!("'{}' already contains '{}' — nothing to merge", into, what));
    }
    let (clean, out) = git_status(ground, &["merge-tree", "--write-tree", "--name-only", "--no-messages", &old, tip])?;
    if !clean {
        let files: Vec<&str> = out.lines().skip(1).filter(|l| !l.is_empty()).collect();
        if files.is_empty() {
            return Err(format!("cannot merge '{}' into '{}': {}", what, into, out));
        }
        return Err(format!(
            "merging '{}' into '{}' would conflict in {} — nothing was changed; resolve it on the floor (merge '{}' there and commit), then land again",
            what, into, files.join(", "), into
        ));
    }
    let tree = out.lines().next().unwrap_or_default().to_string();
    let ff = is_ancestor(&old, tip);

    let ground_branch = checkouts(ground)?.first().and_then(|(_, b)| b.clone());
    if ground_branch.as_deref() == Some(into) {
        if let Some(dirty) = uncommitted(ground, false)? {
            return Err(format!("the ground checkout has uncommitted changes ({}) — commit or stash them, then land again", dirty));
        }
        let msg = format!("Merge floor branch '{}'", what);
        if let Err(e) = git(ground, &["merge", "--no-edit", "-m", &msg, tip]) {
            let _ = git(ground, &["merge", "--abort"]);
            return Err(format!("git merge failed: {}", e));
        }
    } else {
        if let Some((path, _)) = checkouts(ground)?.iter().skip(1).find(|(_, b)| b.as_deref() == Some(into)) {
            return Err(format!("'{}' is checked out in another worktree ({}) — land into a branch that isn't", into, path.display()));
        }
        let new = if ff {
            tip.to_string()
        } else {
            let msg = format!("Merge floor branch '{}' into {}", what, into);
            git(ground, &["commit-tree", &tree, "-p", &old, "-p", tip, "-m", &msg])
                .map_err(|e| format!("cannot create the merge commit: {}", e))?
        };
        git(ground, &["update-ref", &format!("refs/heads/{}", into), &new, &old])
            .map_err(|e| format!("cannot move '{}': {}", into, e))?;
    }
    Ok(format!("merged '{}' into '{}'{}", what, into, if ff { " (fast-forward)" } else { "" }))
}

fn merge_into(ground: &Path, branch: &str, into: &str) -> Result<String, String> {
    let tip = rev(ground, &format!("refs/heads/{}", branch)).ok_or(format!("branch '{}' doesn't exist", branch))?;
    merge_commit_into(ground, &tip, branch, into)
}

/// For a copy floor: fetches its branch back into the project's branch of
/// the same name (created when missing, merged when it exists).
fn transfer_copy(ground: &Path, copy: &Path, branch: &str) -> Result<String, String> {
    let src = copy.display().to_string();
    git(ground, &["fetch", "-q", &src, &format!("refs/heads/{}", branch)])
        .map_err(|e| format!("cannot fetch '{}' from the floor's copy: {}", branch, e))?;
    let tip = rev(ground, "FETCH_HEAD").ok_or("the fetch brought nothing")?;
    if rev(ground, &format!("refs/heads/{}", branch)).is_none() {
        git(ground, &["branch", "-q", branch, &tip])?;
        return Ok(format!("brought '{}' into the project", branch));
    }
    merge_commit_into(ground, &tip, &format!("{} (floor copy)", branch), branch)
}

/// Lands an isolated floor: its branch is in the project (a copy's is
/// fetched back first), optionally merged into `into`; then the floor's
/// checkout goes, unless `keep_floor`; the branch stays unless
/// `delete_branch`. Refused, with nothing changed, when the floor or the
/// ground has uncommitted work or a merge would conflict. `root` is the
/// scene's directory (a copy doesn't know where Ground is).
pub fn land(
    worktree: &str,
    branch: &str,
    into: Option<&str>,
    delete_branch: bool,
    keep_floor: bool,
    root: Option<&str>,
) -> Result<String, String> {
    if delete_branch && into.is_none() {
        return Err("--delete-branch needs --into: the branch is the floor's only copy of its work until it's merged".into());
    }
    if delete_branch && keep_floor {
        return Err("--delete-branch and --keep-floor don't go together: the kept floor still has the branch checked out".into());
    }
    let wt = PathBuf::from(worktree);
    if !wt.is_dir() {
        return Err(format!("the floor's checkout is gone ({}) — delete the floor instead", worktree));
    }
    if into == Some(branch) {
        return Err(format!("'{}' is the floor's own branch — --into takes the branch to merge it into", branch));
    }
    if let Some(dirty) = uncommitted(&wt, true)? {
        return Err(format!("the floor has uncommitted work ({}) — ask its agent to commit it, then land again", dirty));
    }
    let copy = is_copy(&wt);
    let ground = if copy {
        project_repo(root).map_err(|why| format!("cannot find the project to land the copy into: {}", why))?.1
    } else {
        checkouts(&wt)?.into_iter().next().map(|(p, _)| p).ok_or("cannot find the project's main checkout")?
    };
    if same_path(&ground, &wt) {
        return Err("that is the ground checkout, not a floor".into());
    }
    if into.is_some() || copy {
        if let Some(dirty) = uncommitted(&ground, false)? {
            return Err(format!("the ground checkout has uncommitted changes ({}) — commit or stash them, then land again", dirty));
        }
    }

    let mut parts = Vec::new();
    if copy {
        parts.push(transfer_copy(&ground, &wt, branch)?);
    }
    if let Some(into) = into {
        parts.push(merge_into(&ground, branch, into)?);
    }
    if !keep_floor {
        remove_checkout(&wt, false).map_err(|e| {
            format!("{}landed, but the floor's checkout could not be removed: {}", parts.first().map_or(String::new(), |p| format!("{}; ", p)), e)
        })?;
        parts.push("floor removed".into());
    }
    if delete_branch {
        match git(&ground, &["branch", "-d", branch]) {
            Ok(_) => parts.push(format!("branch '{}' deleted", branch)),
            Err(e) => parts.push(format!("branch '{}' kept ({})", branch, e)),
        }
    } else {
        parts.push(format!("branch '{}' stays in the project", branch));
    }
    Ok(parts.join("; "))
}

/// Deletes a floor's checkout and (unless `keep_branch`) its branch. Without
/// `force`, a checkout with uncommitted work is an error and nothing changes;
/// with it, the floor goes anyway. Returns what happened.
pub fn delete(worktree: &str, branch: Option<&str>, keep_branch: bool, force: bool) -> Result<String, String> {
    let wt = PathBuf::from(worktree);
    let mut parts = Vec::new();
    let copy = is_copy(&wt);
    let ground = if wt.is_dir() {
        let ground = if copy { None } else { checkouts(&wt)?.into_iter().next().map(|(p, _)| p) };
        match remove_checkout(&wt, force) {
            Ok(_) => parts.push(if copy { "copy removed" } else { "worktree removed" }.to_string()),
            Err(e) if !force => {
                return Err(format!(
                    "the floor's checkout can't be removed: {} — nothing was changed (--force removes it anyway, discarding that work)",
                    e
                ))
            }
            Err(e) => parts.push(format!("checkout left on disk ({})", e)),
        }
        ground
    } else {
        parts.push("checkout was already gone".to_string());
        None
    };
    match (branch, copy) {
        (Some(branch), true) => parts.push(format!("its copy of '{}' went with it; the project's branches are untouched", branch)),
        (Some(branch), false) if !keep_branch => match ground {
            Some(g) => {
                let sha = rev(&g, &format!("refs/heads/{}", branch));
                match git(&g, &["branch", "-D", branch]) {
                    Ok(_) => parts.push(format!(
                        "branch '{}' deleted{}",
                        branch,
                        sha.map_or(String::new(), |s| format!(
                            " (was {} — `git branch {} {}` brings it back)",
                            &s[..s.len().min(10)],
                            branch,
                            &s[..s.len().min(10)]
                        ))
                    )),
                    Err(e) => parts.push(format!("branch '{}' kept ({})", branch, e)),
                }
            }
            None => parts.push(format!("branch '{}' kept (the repository wasn't reachable)", branch)),
        },
        (Some(branch), false) => parts.push(format!("branch '{}' stays in the project", branch)),
        (None, _) => {}
    }
    Ok(parts.join("; "))
}

#[tauri::command]
pub fn floor_land(
    worktree: String,
    branch: String,
    into: Option<String>,
    delete_branch: bool,
    keep_floor: bool,
    root: Option<String>,
) -> Result<String, String> {
    land(&worktree, &branch, into.as_deref().filter(|s| !s.is_empty()), delete_branch, keep_floor, root.as_deref())
}

#[tauri::command]
pub fn floor_delete(worktree: String, branch: Option<String>, keep_branch: bool, force: bool) -> Result<String, String> {
    delete(&worktree, branch.as_deref(), keep_branch, force)
}

/// Local branches of the project (for the land target picker), the ground
/// checkout's branch first. `root` finds the project for a copy floor.
#[tauri::command]
pub fn floor_branches(worktree: String, root: Option<String>) -> Result<Vec<String>, String> {
    let wt = PathBuf::from(&worktree);
    let repo = if is_copy(&wt) { project_repo(root.as_deref()).map_err(str::to_string)?.1 } else { wt };
    let ground_branch = checkouts(&repo)?.into_iter().next().and_then(|(_, b)| b);
    let out = git(&repo, &["for-each-ref", "--format=%(refname:short)", "refs/heads/"])?;
    let mut list: Vec<String> = out.lines().map(str::to_string).collect();
    if let Some(g) = ground_branch {
        list.retain(|b| *b != g);
        list.insert(0, g);
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("narrater-floor-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("app")).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
            vec!["commit", "-q", "--allow-empty", "-m", "init"],
        ] {
            assert!(Command::new("git").arg("-C").arg(&dir).args(&args).status().unwrap().success());
        }
        dir
    }

    #[test]
    fn slugs_make_branch_names() {
        assert_eq!(slug("Refactor Auth!"), "refactor-auth");
        assert_eq!(slug("feat/Idea X"), "feat/idea-x");
        assert_eq!(slug("  "), "");
    }

    #[test]
    fn plain_floor_without_git() {
        let info = prepare(Some("/tmp"), "s", "Scratch", None, false, true, false, "auto").unwrap();
        assert_eq!(info.dir, None);
        assert!(info.note.unwrap().contains("--no-git"));
        let info = prepare(None, "s", "Scratch", None, false, false, false, "auto").unwrap();
        assert!(info.note.unwrap().contains("no working directory"));
    }

    #[test]
    fn isolated_floor_lifecycle() {
        let repo = repo();
        let scene = format!("test-{}", uuid::Uuid::new_v4().simple());
        // Scene rooted at a subdirectory of the repo
        let root = repo.join("app");
        let info = prepare(Some(root.to_str().unwrap()), &scene, "Exp One", None, false, false, false, "auto").unwrap();
        assert_eq!(info.branch.as_deref(), Some("exp-one"));
        // The branch is checked out there now — another floor can't take it
        assert!(prepare(Some(root.to_str().unwrap()), &scene, "Y", Some("exp-one"), true, false, false, "auto")
            .unwrap_err().contains("already checked out"));
        let wt = PathBuf::from(info.worktree.clone().unwrap());
        assert!(info.dir.unwrap().ends_with("/app"));
        assert!(wt.join(".git").exists());

        // Same branch again → refused; --existing-branch on a new name → refused
        assert!(prepare(Some(root.to_str().unwrap()), &scene, "Exp One", None, false, false, false, "auto").unwrap_err().contains("already exists"));
        assert!(prepare(Some(root.to_str().unwrap()), &scene, "X", Some("nope"), true, false, false, "auto").unwrap_err().contains("doesn't exist"));

        // Dirty worktree: remove refuses, force works
        std::fs::write(wt.join("new.txt"), "x").unwrap();
        assert!(floor_status(wt.display().to_string()).unwrap().dirty);
        assert!(floor_remove(wt.display().to_string(), false).is_err());
        floor_remove(wt.display().to_string(), true).unwrap();
        assert!(!wt.exists());
        // The branch survives the floor
        assert!(git(&repo, &["rev-parse", "--verify", "--quiet", "refs/heads/exp-one"]).is_ok());

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(floors_root().join(slug(&scene)));
    }

    fn land_(w: &str, b: &str, into: Option<&str>, del: bool, keep: bool) -> Result<String, String> {
        land(w, b, into, del, keep, None)
    }

    fn commit(dir: &Path, file: &str, text: &str) {
        std::fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", file]).unwrap();
        git(dir, &["commit", "-q", "-m", file]).unwrap();
    }

    fn floor(repo: &Path, scene: &str, name: &str) -> PathBuf {
        let info = prepare(Some(repo.to_str().unwrap()), scene, name, None, false, false, false, "auto").unwrap();
        PathBuf::from(info.worktree.unwrap())
    }

    #[test]
    fn land_merges_and_removes_the_floor() {
        let repo = repo();
        let scene = format!("test-{}", uuid::Uuid::new_v4().simple());
        let wt = floor(&repo, &scene, "Feature");
        let w = wt.display().to_string();

        // Uncommitted work on the floor → refused, nothing changes
        std::fs::write(wt.join("a.txt"), "a").unwrap();
        assert!(land_(&w, "feature", Some("main"), false, false).unwrap_err().contains("uncommitted work"));
        git(&wt, &["add", "a.txt"]).unwrap();
        git(&wt, &["commit", "-q", "-m", "a"]).unwrap();

        // Flag combinations that make no sense
        assert!(land_(&w, "feature", None, true, false).unwrap_err().contains("needs --into"));
        assert!(land_(&w, "feature", Some("main"), true, true).unwrap_err().contains("don't go together"));
        assert!(land_(&w, "feature", Some("feature"), false, false).unwrap_err().contains("own branch"));

        // Dirty ground (tracked file) → refused
        commit(&repo, "g.txt", "1");
        std::fs::write(repo.join("g.txt"), "2").unwrap();
        assert!(land_(&w, "feature", Some("main"), false, false).unwrap_err().contains("ground checkout has uncommitted"));
        git(&repo, &["checkout", "--", "g.txt"]).unwrap();

        // --keep-floor merges (ground diverged → merge commit) and keeps the worktree
        let out = land_(&w, "feature", Some("main"), false, true).unwrap();
        assert!(out.contains("merged 'feature' into 'main'") && !out.contains("fast-forward"), "{}", out);
        assert!(repo.join("a.txt").exists() && wt.exists());
        // More work, landed again: this time the floor goes and the branch with it
        commit(&wt, "b.txt", "b");
        let out = land_(&w, "feature", Some("main"), true, false).unwrap();
        assert!(out.contains("merged") && out.contains("floor removed") && out.contains("deleted"), "{}", out);
        assert!(repo.join("b.txt").exists() && !wt.exists());
        assert!(rev(&repo, "refs/heads/feature").is_none());

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(floors_root().join(slug(&scene)));
    }

    #[test]
    fn land_into_a_branch_checked_out_nowhere_and_conflicts() {
        let repo = repo();
        let scene = format!("test-{}", uuid::Uuid::new_v4().simple());
        git(&repo, &["branch", "release"]).unwrap();
        let wt = floor(&repo, &scene, "Fix");
        let w = wt.display().to_string();
        commit(&wt, "f.txt", "floor");

        // release isn't checked out: moved by ref, the ground checkout untouched
        let out = land_(&w, "fix", Some("release"), false, true).unwrap();
        assert!(out.contains("fast-forward"), "{}", out);
        assert_eq!(rev(&repo, "refs/heads/release"), rev(&repo, "refs/heads/fix"));
        assert!(!repo.join("f.txt").exists());

        // Both sides change the same file → refused, naming it; main unmoved
        commit(&repo, "f.txt", "ground");
        let before = rev(&repo, "refs/heads/main");
        let err = land_(&w, "fix", Some("main"), false, false).unwrap_err();
        assert!(err.contains("conflict") && err.contains("f.txt"), "{}", err);
        assert_eq!(rev(&repo, "refs/heads/main"), before);
        assert!(wt.exists());

        // Without --into: just removes the floor, the branch stays
        let out = land_(&w, "fix", None, false, false).unwrap();
        assert!(out.contains("stays"), "{}", out);
        assert!(!wt.exists() && rev(&repo, "refs/heads/fix").is_some());

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(floors_root().join(slug(&scene)));
    }

    #[test]
    fn delete_removes_worktree_and_branch() {
        let repo = repo();
        let scene = format!("test-{}", uuid::Uuid::new_v4().simple());
        let wt = floor(&repo, &scene, "Scratch");
        let w = wt.display().to_string();
        commit(&wt, "s.txt", "s");
        std::fs::write(wt.join("s.txt"), "dirty").unwrap();

        // Dirty: refused without --force, nothing changed
        assert!(delete(&w, Some("scratch"), false, false).unwrap_err().contains("--force"));
        assert!(wt.exists());
        // --keep-branch with --force: worktree goes, branch stays
        let out = delete(&w, Some("scratch"), true, true).unwrap();
        assert!(out.contains("worktree removed") && out.contains("stays"), "{}", out);
        assert!(!wt.exists() && rev(&repo, "refs/heads/scratch").is_some());

        // A clean floor: branch deleted, with the sha to bring it back
        let wt = floor(&repo, &scene, "Other");
        commit(&wt, "o.txt", "o");
        let out = delete(&wt.display().to_string(), Some("other"), false, false).unwrap();
        assert!(out.contains("branch 'other' deleted (was "), "{}", out);
        assert!(rev(&repo, "refs/heads/other").is_none());

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(floors_root().join(slug(&scene)));
    }

    /// A fake `gh` answering `pr view` with `json` (and logging its args).
    fn fake_gh(dir: &Path, json: &str) -> String {
        let path = dir.join("fake-gh");
        std::fs::write(&path, format!("#!/bin/sh\necho \"$@\" >> \"{}\"\ncat <<'JSON'\n{}\nJSON\n", dir.join("gh.log").display(), json)).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        // A test forking in parallel may still hold the write fd for a
        // moment ("text file busy"): wait until the stub can run
        for _ in 0..200 {
            match Command::new(&path).arg("probe").output() {
                Err(e) if e.raw_os_error() == Some(26) => std::thread::sleep(std::time::Duration::from_millis(5)),
                _ => break,
            }
        }
        path.display().to_string()
    }

    /// A project cloned from a bare "host" repo that has branch `fix-login`
    /// one commit ahead of main, plus a fork-style ref refs/pull/7/head.
    fn hosted() -> (PathBuf, PathBuf, String, String) {
        let host = std::env::temp_dir().join(format!("narrater-host-{}", uuid::Uuid::new_v4()));
        let seed = repo();
        git(&seed, &["checkout", "-q", "-b", "fix-login"]).unwrap();
        commit(&seed, "login.txt", "fixed");
        let fix = rev(&seed, "HEAD").unwrap();
        git(&seed, &["checkout", "-q", "-b", "forked", "main"]).unwrap();
        commit(&seed, "fork.txt", "from a fork");
        let fork = rev(&seed, "HEAD").unwrap();
        git(&seed, &["checkout", "-q", "main"]).unwrap();
        assert!(Command::new("git").args(["clone", "-q", "--bare"]).arg(&seed).arg(&host).status().unwrap().success());
        git(&host, &["update-ref", "refs/pull/7/head", &fork]).unwrap();
        git(&host, &["branch", "-D", "forked"]).unwrap();
        let project = std::env::temp_dir().join(format!("narrater-proj-{}", uuid::Uuid::new_v4()));
        assert!(Command::new("git").args(["clone", "-q"]).arg(&host).arg(&project).status().unwrap().success());
        git(&project, &["config", "user.email", "t@t"]).unwrap();
        git(&project, &["config", "user.name", "t"]).unwrap();
        let _ = std::fs::remove_dir_all(&seed);
        (host, project, fix, fork)
    }

    #[test]
    fn parses_pull_request_references() {
        assert_eq!(parse_pr_ref("42").unwrap(), (42, None));
        assert_eq!(parse_pr_ref("#42").unwrap().0, 42);
        assert_eq!(parse_pr_ref("https://github.com/acme/app/pull/42").unwrap().0, 42);
        assert_eq!(parse_pr_ref("https://github.com/acme/app/pull/42/files#diff").unwrap().0, 42);
        assert_eq!(parse_pr_ref("https://gitlab.com/g/sub/app/-/merge_requests/9").unwrap().0, 9);
        assert!(parse_pr_ref("abc").is_err());
        assert_eq!(repo_path_of("git@github.com:Acme/App.git").as_deref(), Some("Acme/App"));
        assert_eq!(repo_path_of("https://github.com/acme/app/pull/42").as_deref(), Some("acme/app"));
        assert_eq!(repo_path_of("https://gitlab.com/g/sub/app/-/merge_requests/9").as_deref(), Some("g/sub/app"));
    }

    #[test]
    fn pull_request_floor_from_the_same_repo() {
        let (host, project, fix, _) = hosted();
        let scene = format!("test-{}", uuid::Uuid::new_v4().simple());
        let p = project.to_str().unwrap();
        let open = |sha: &str| format!(
            r#"{{"number":12,"title":"Fix the login","url":"https://github.com/acme/app/pull/12","state":"OPEN","headRefName":"fix-login","headRefOid":"{}","isCrossRepository":false}}"#, sha);
        let gh = fake_gh(&project.join(".git"), &open(&fix));

        // Closed, already on a floor, moved while fetched → refused
        std::fs::create_dir_all(project.join(".git/closed")).unwrap();
        let closed = fake_gh(&project.join(".git/closed"), &open(&fix).replace("OPEN", "MERGED"));
        assert!(prepare_pr(Some(p), &scene, None, "12", false, &[], Some(&closed), "auto").unwrap_err().contains("isn't open"));
        assert!(prepare_pr(Some(p), &scene, None, "#12", false, &[12], Some(&gh), "auto").unwrap_err().contains("already on a floor"));
        std::fs::create_dir_all(project.join(".git/moved")).unwrap();
        let moved = fake_gh(&project.join(".git/moved"), &open(&"0".repeat(40)));
        assert!(prepare_pr(Some(p), &scene, None, "12", false, &[], Some(&moved), "auto").unwrap_err().contains("changed while it was fetched"));

        let info = prepare_pr(Some(p), &scene, None, "https://github.com/acme/app/pull/12", false, &[], Some(&gh), "auto").unwrap();
        assert_eq!(info.name.as_deref(), Some("#12 Fix the login"));
        assert_eq!(info.branch.as_deref(), Some("fix-login"));
        assert!(info.pull_request.as_ref().unwrap().same_repo && info.note.is_none());
        let wt = PathBuf::from(info.worktree.unwrap());
        assert_eq!(rev(&wt, "HEAD").as_deref(), Some(fix.as_str()));
        // The branch tracks the pull request's branch: pushes go to it
        assert_eq!(git(&project, &["rev-parse", "--abbrev-ref", "fix-login@{upstream}"]).unwrap(), "origin/fix-login");
        commit(&wt, "more.txt", "review fix");
        git(&wt, &["push", "-q"]).unwrap();
        assert_eq!(rev(&host, "refs/heads/fix-login"), rev(&wt, "HEAD"));

        delete(&wt.display().to_string(), Some("fix-login"), false, false).unwrap();
        // A local branch that pushes somewhere else is refused
        git(&project, &["branch", "-q", "fix-login", "main"]).unwrap();
        git(&project, &["config", "branch.fix-login.remote", "origin"]).unwrap();
        git(&project, &["config", "branch.fix-login.merge", "refs/heads/main"]).unwrap();
        let gh2 = fake_gh(&project.join(".git"), &open(&rev(&host, "refs/heads/fix-login").unwrap()));
        assert!(prepare_pr(Some(p), &scene, None, "12", false, &[], Some(&gh2), "auto").unwrap_err().contains("pushes somewhere else"));
        // …and one that went its own way
        git(&project, &["config", "--unset", "branch.fix-login.remote"]).unwrap();
        git(&project, &["config", "--unset", "branch.fix-login.merge"]).unwrap();
        git(&project, &["checkout", "-q", "fix-login"]).unwrap();
        commit(&project, "own.txt", "diverged");
        git(&project, &["checkout", "-q", "main"]).unwrap();
        assert!(prepare_pr(Some(p), &scene, None, "12", false, &[], Some(&gh2), "auto").unwrap_err().contains("went its own way"));

        let _ = std::fs::remove_dir_all(&host);
        let _ = std::fs::remove_dir_all(&project);
        let _ = std::fs::remove_dir_all(floors_root().join(slug(&scene)));
    }

    #[test]
    fn pull_request_floor_from_a_fork_gets_a_copy_of_its_commits() {
        let (host, project, _, fork) = hosted();
        let scene = format!("test-{}", uuid::Uuid::new_v4().simple());
        let gh = fake_gh(&project.join(".git"), &format!(
            r#"{{"number":7,"title":"From a fork","url":"https://github.com/acme/app/pull/7","state":"OPEN","headRefName":"main","headRefOid":"{}","isCrossRepository":true}}"#, fork));
        let info = prepare_pr(Some(project.to_str().unwrap()), &scene, Some("Review fork"), "7", false, &[], Some(&gh), "auto").unwrap();
        assert_eq!(info.branch.as_deref(), Some("pr-7"));
        assert_eq!(info.name.as_deref(), Some("Review fork"));
        assert!(info.note.unwrap().contains("fork"));
        assert!(!info.pull_request.unwrap().same_repo);
        assert_eq!(rev(Path::new(&info.worktree.unwrap()), "HEAD").as_deref(), Some(fork.as_str()));

        let _ = std::fs::remove_dir_all(&host);
        let _ = std::fs::remove_dir_all(&project);
        let _ = std::fs::remove_dir_all(floors_root().join(slug(&scene)));
    }

    #[test]
    fn pull_request_status_line() {
        let dir = std::env::temp_dir().join(format!("narrater-prs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let gh = fake_gh(&dir, r#"{"state":"OPEN","reviewDecision":"APPROVED","mergeable":"MERGEABLE","statusCheckRollup":[
            {"conclusion":"SUCCESS"},{"conclusion":"FAILURE"},{"state":"PENDING"},{"conclusion":"SKIPPED"}]}"#);
        let pr = PullRequestInfo { number: 3, title: "t".into(), url: "https://github.com/a/b/pull/3".into(), provider: "github".into(), same_repo: true };
        assert_eq!(
            pr_status(&dir, &pr, Some(&gh)).unwrap(),
            "OPEN · review: APPROVED · checks: 2 passed, 1 failing, 1 pending · mergeable: MERGEABLE"
        );
        assert!(pr_merge(&dir, &pr, Some(&gh)).unwrap().contains("merged"));
        assert!(std::fs::read_to_string(dir.join("gh.log")).unwrap().contains("pr merge https://github.com/a/b/pull/3 --merge"));
        let missing = pr_status(&dir, &pr, Some("/nonexistent/gh")).unwrap_err();
        assert!(missing.contains("no provider"), "{}", missing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_floor_shares_grounds_branch_and_lands_back() {
        let repo = repo();
        let scene = format!("test-{}", uuid::Uuid::new_v4().simple());
        let r = repo.to_str().unwrap();
        // Only a disk that clones instantly takes --copy (this test's /tmp may or may not)
        if let Err(e) = prepare(Some(r), &scene, "Probe", Some("main"), true, false, true, "always") {
            assert!(e.contains("clones instantly"), "{}", e);
        }
        let _ = std::fs::remove_dir_all(floors_root().join(slug(&scene)));
        std::fs::write(repo.join("untracked.txt"), "local").unwrap();

        // The copy checks out main — the branch Ground has
        let info = prepare(Some(r), &scene, "Same Branch", Some("main"), true, false, true, "auto").unwrap();
        assert!(info.copy);
        let c = PathBuf::from(info.worktree.unwrap());
        assert!(is_copy(&c) && c.join("untracked.txt").exists());
        assert!(!c.join(".git/worktrees").exists());
        commit(&c, "work.txt", "done on the copy");
        std::fs::remove_file(repo.join("untracked.txt")).unwrap();
        let w = c.display().to_string();
        // Untracked files on the copy are uncommitted work: land refuses
        assert!(land(&w, "main", None, false, false, Some(r)).unwrap_err().contains("untracked.txt"));
        std::fs::remove_file(c.join("untracked.txt")).unwrap();
        // Land: the work comes back into Ground's main; the copy goes
        let out = land(&w, "main", None, false, false, Some(r)).unwrap();
        assert!(out.contains("merged") && out.contains("floor removed"), "{}", out);
        assert!(repo.join("work.txt").exists() && !c.exists());

        // A copy on a new branch: landing creates it in the project; delete leaves branches alone
        let info = prepare(Some(r), &scene, "New Branch", None, false, false, true, "auto").unwrap();
        let c = PathBuf::from(info.worktree.unwrap());
        commit(&c, "nb.txt", "x");
        assert!(land(&c.display().to_string(), "new-branch", None, false, true, Some(r)).unwrap().contains("brought 'new-branch'"));
        assert!(rev(&repo, "refs/heads/new-branch").is_some());
        std::fs::write(c.join("dirty.txt"), "x").unwrap();
        assert!(delete(&c.display().to_string(), Some("new-branch"), false, false).is_err());
        let out = delete(&c.display().to_string(), Some("new-branch"), false, true).unwrap();
        assert!(out.contains("copy removed") && !c.exists(), "{}", out);
        assert!(rev(&repo, "refs/heads/new-branch").is_some());

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(floors_root().join(slug(&scene)));
    }
}

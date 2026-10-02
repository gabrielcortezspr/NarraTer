//! Floors with git isolation: a floor of a scene can be backed by a git
//! worktree on its own branch, so agents there work without touching the
//! ground checkout (Maestri does a copy-on-write clone on macOS; worktrees
//! are the Linux equivalent — cheap, and the branch lands in the same repo).
//!
//! Worktrees live outside the project, in ~/.local/share/narrater/floors/.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
pub struct FloorInfo {
    /// Where the floor's terminals work (None: shares the ground directory)
    pub dir: Option<String>,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    /// Why the floor isn't isolated, when it isn't
    pub note: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct FloorStatus {
    /// Uncommitted changes in the worktree
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

fn floors_root() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("narrater/floors")
}

/// Prepares a floor for scene `scene` rooted at `root`. Isolated (worktree
/// on a branch) when the root is in a git repository and `no_git` is off;
/// otherwise a plain floor sharing the ground directory, with the reason.
pub fn prepare(
    root: Option<&str>,
    scene: &str,
    name: &str,
    branch: Option<&str>,
    existing_branch: bool,
    no_git: bool,
) -> Result<FloorInfo, String> {
    let plain = |why: &str| Ok(FloorInfo { dir: None, worktree: None, branch: None, note: Some(why.to_string()) });
    if no_git {
        return plain("plain floor (--no-git): it shares the ground directory");
    }
    let Some(root) = root.map(expand).filter(|r| r.is_dir()) else {
        return plain("plain floor: this scene has no working directory (set one in the sidebar, or `narrater workspace create --dir`)");
    };
    let Ok(top) = git(&root, &["rev-parse", "--show-toplevel"]).map(PathBuf::from) else {
        return plain("plain floor: the scene's directory is not a git repository");
    };
    if git(&top, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_err() {
        return plain("plain floor: the repository has no commits yet");
    }

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

    let base = floors_root().join(slug(scene).replace('/', "-"));
    let leaf = slug(name).replace('/', "-");
    let mut worktree = base.join(&leaf);
    let mut n = 2;
    while worktree.exists() {
        worktree = base.join(format!("{}-{}", leaf, n));
        n += 1;
    }
    std::fs::create_dir_all(&base).map_err(|e| format!("cannot create {}: {}", base.display(), e))?;
    let wt = worktree.display().to_string();
    if existing_branch {
        git(&top, &["worktree", "add", &wt, &branch])
    } else {
        git(&top, &["worktree", "add", "-b", &branch, &wt])
    }
    .map_err(|e| {
        if e.contains("is already used by worktree") || e.contains("is already checked out") {
            format!("branch '{}' is already checked out in another worktree (a floor or the ground checkout) — pick another --branch", branch)
        } else {
            format!("git worktree add failed: {}", e)
        }
    })?;

    // A scene rooted in a subdirectory of the repo works in that same
    // subdirectory of the worktree
    let rel = root.canonicalize().ok().and_then(|r| {
        top.canonicalize().ok().and_then(|t| r.strip_prefix(&t).ok().map(Path::to_path_buf))
    });
    let dir = rel.filter(|r| !r.as_os_str().is_empty()).map_or(worktree.clone(), |r| worktree.join(r));
    Ok(FloorInfo {
        dir: Some(dir.display().to_string()),
        worktree: Some(wt),
        branch: Some(branch),
        note: None,
    })
}

#[tauri::command]
pub fn floor_prepare(
    root: Option<String>,
    scene: String,
    name: String,
    branch: Option<String>,
    existing_branch: bool,
    no_git: bool,
) -> Result<FloorInfo, String> {
    prepare(root.as_deref(), &scene, &name, branch.as_deref(), existing_branch, no_git)
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

/// Removes a floor's worktree (the branch and its commits stay in the repo).
/// Refuses a dirty worktree unless `force`.
#[tauri::command]
pub fn floor_remove(worktree: String, force: bool) -> Result<(), String> {
    let wt = PathBuf::from(&worktree);
    if !wt.exists() {
        return Ok(());
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&worktree);
    git(&wt, &args).map(|_| ()).map_err(|e| format!("git worktree remove failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("narrater-floor-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("app")).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"],
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
        let info = prepare(Some("/tmp"), "s", "Scratch", None, false, true).unwrap();
        assert_eq!(info.dir, None);
        assert!(info.note.unwrap().contains("--no-git"));
        let info = prepare(None, "s", "Scratch", None, false, false).unwrap();
        assert!(info.note.unwrap().contains("no working directory"));
    }

    #[test]
    fn isolated_floor_lifecycle() {
        let repo = repo();
        let scene = format!("test-{}", uuid::Uuid::new_v4().simple());
        // Scene rooted at a subdirectory of the repo
        let root = repo.join("app");
        let info = prepare(Some(root.to_str().unwrap()), &scene, "Exp One", None, false, false).unwrap();
        assert_eq!(info.branch.as_deref(), Some("exp-one"));
        // The branch is checked out there now — another floor can't take it
        assert!(prepare(Some(root.to_str().unwrap()), &scene, "Y", Some("exp-one"), true, false)
            .unwrap_err().contains("already checked out"));
        let wt = PathBuf::from(info.worktree.clone().unwrap());
        assert!(info.dir.unwrap().ends_with("/app"));
        assert!(wt.join(".git").exists());

        // Same branch again → refused; --existing-branch on a new name → refused
        assert!(prepare(Some(root.to_str().unwrap()), &scene, "Exp One", None, false, false).unwrap_err().contains("already exists"));
        assert!(prepare(Some(root.to_str().unwrap()), &scene, "X", Some("nope"), true, false).unwrap_err().contains("doesn't exist"));

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
}

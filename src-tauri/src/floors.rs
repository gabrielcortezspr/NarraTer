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

/// Uncommitted work in a checkout, as "N file(s): a, b, c…" (None: clean).
/// `untracked` counts new files too (they'd be lost with a floor's worktree).
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

/// Merges `branch` into `into`. Refuses, changing nothing, on a conflict
/// (checked first with `merge-tree`) or when `into` is checked out on a
/// floor. In the ground checkout it's a regular `git merge`; a branch
/// checked out nowhere is moved by ref (fast-forward or a merge commit).
fn merge_into(ground: &Path, branch: &str, into: &str) -> Result<String, String> {
    let Some(old) = rev(ground, &format!("refs/heads/{}", into)) else {
        return Err(format!("branch '{}' doesn't exist", into));
    };
    let tip = rev(ground, &format!("refs/heads/{}", branch)).ok_or(format!("branch '{}' doesn't exist", branch))?;
    let is_ancestor = |a: &str, b: &str| git(ground, &["merge-base", "--is-ancestor", a, b]).is_ok();
    if is_ancestor(&tip, &old) {
        return Ok(format!("'{}' already contains '{}' — nothing to merge", into, branch));
    }
    let (clean, out) = git_status(ground, &["merge-tree", "--write-tree", "--name-only", "--no-messages", &old, &tip])?;
    if !clean {
        let files: Vec<&str> = out.lines().skip(1).filter(|l| !l.is_empty()).collect();
        if files.is_empty() {
            return Err(format!("cannot merge '{}' into '{}': {}", branch, into, out));
        }
        return Err(format!(
            "merging '{}' into '{}' would conflict in {} — nothing was changed; resolve it on the floor (merge '{}' there and commit), then land again",
            branch, into, files.join(", "), into
        ));
    }
    let tree = out.lines().next().unwrap_or_default().to_string();

    let ground_branch = checkouts(ground)?.first().and_then(|(_, b)| b.clone());
    if ground_branch.as_deref() == Some(into) {
        if let Some(dirty) = uncommitted(ground, false)? {
            return Err(format!("the ground checkout has uncommitted changes ({}) — commit or stash them, then land again", dirty));
        }
        let msg = format!("Merge floor branch '{}'", branch);
        if let Err(e) = git(ground, &["merge", "--no-edit", "-m", &msg, branch]) {
            let _ = git(ground, &["merge", "--abort"]);
            return Err(format!("git merge failed: {}", e));
        }
    } else {
        if let Some((path, _)) = checkouts(ground)?.iter().skip(1).find(|(_, b)| b.as_deref() == Some(into)) {
            return Err(format!("'{}' is checked out in another worktree ({}) — land into a branch that isn't", into, path.display()));
        }
        let new = if is_ancestor(&old, &tip) {
            tip.clone()
        } else {
            let msg = format!("Merge floor branch '{}' into {}", branch, into);
            git(ground, &["commit-tree", &tree, "-p", &old, "-p", &tip, "-m", &msg])
                .map_err(|e| format!("cannot create the merge commit: {}", e))?
        };
        git(ground, &["update-ref", &format!("refs/heads/{}", into), &new, &old])
            .map_err(|e| format!("cannot move '{}': {}", into, e))?;
    }
    let ff = is_ancestor(&old, &tip);
    Ok(format!("merged '{}' into '{}'{}", branch, into, if ff { " (fast-forward)" } else { "" }))
}

/// Lands a git-isolated floor: optionally merges its branch into `into`,
/// then removes its worktree (unless `keep_floor`); the branch stays in the
/// repository unless `delete_branch`. Refused, with nothing changed, when
/// the floor or the ground has uncommitted work or the merge would conflict.
pub fn land(worktree: &str, branch: &str, into: Option<&str>, delete_branch: bool, keep_floor: bool) -> Result<String, String> {
    if delete_branch && into.is_none() {
        return Err("--delete-branch needs --into: the branch is the floor's only copy of its work until it's merged".into());
    }
    if delete_branch && keep_floor {
        return Err("--delete-branch and --keep-floor don't go together: the kept floor still has the branch checked out".into());
    }
    let wt = PathBuf::from(worktree);
    if !wt.is_dir() {
        return Err(format!("the floor's worktree is gone ({}) — delete the floor instead", worktree));
    }
    if into == Some(branch) {
        return Err(format!("'{}' is the floor's own branch — --into takes the branch to merge it into", branch));
    }
    if let Some(dirty) = uncommitted(&wt, true)? {
        return Err(format!("the floor has uncommitted work ({}) — ask its agent to commit it, then land again", dirty));
    }
    let ground = checkouts(&wt)?.into_iter().next().map(|(p, _)| p).ok_or("cannot find the project's main checkout")?;
    if same_path(&ground, &wt) {
        return Err("that is the ground checkout, not a floor".into());
    }
    if into.is_some() {
        if let Some(dirty) = uncommitted(&ground, false)? {
            return Err(format!("the ground checkout has uncommitted changes ({}) — commit or stash them, then land again", dirty));
        }
    }

    let mut parts = Vec::new();
    if let Some(into) = into {
        parts.push(merge_into(&ground, branch, into)?);
    }
    if !keep_floor {
        git(&ground, &["worktree", "remove", worktree]).map_err(|e| {
            format!("{}landed, but its worktree could not be removed: {}", parts.first().map_or(String::new(), |p| format!("{}; ", p)), e)
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

/// Deletes a floor's worktree and (unless `keep_branch`) its branch. Without
/// `force`, a worktree git won't remove (uncommitted work) is an error and
/// nothing changes; with it, the floor goes anyway. Returns what happened.
pub fn delete(worktree: &str, branch: Option<&str>, keep_branch: bool, force: bool) -> Result<String, String> {
    let wt = PathBuf::from(worktree);
    let mut parts = Vec::new();
    let ground = if wt.is_dir() {
        let ground = checkouts(&wt)?.into_iter().next().map(|(p, _)| p);
        let mut args = vec!["worktree", "remove"];
        if force {
            args.extend(["--force", "--force"]);
        }
        args.push(worktree);
        match git(&wt, &args) {
            Ok(_) => parts.push("worktree removed".to_string()),
            Err(e) if !force => {
                return Err(format!("the floor's worktree can't be removed: {} — nothing was changed (--force removes it anyway, discarding that work)", e))
            }
            Err(e) => parts.push(format!("worktree left on disk ({})", e)),
        }
        ground
    } else {
        parts.push("worktree was already gone".to_string());
        None
    };
    if let (Some(branch), false) = (branch, keep_branch) {
        match ground {
            Some(g) => {
                let sha = rev(&g, &format!("refs/heads/{}", branch));
                match git(&g, &["branch", "-D", branch]) {
                    Ok(_) => parts.push(format!(
                        "branch '{}' deleted{}",
                        branch,
                        sha.map_or(String::new(), |s| format!(" (was {} — `git branch {} {}` brings it back)", &s[..s.len().min(10)], branch, &s[..s.len().min(10)]))
                    )),
                    Err(e) => parts.push(format!("branch '{}' kept ({})", branch, e)),
                }
            }
            None => parts.push(format!("branch '{}' kept (the repository wasn't reachable)", branch)),
        }
    } else if let Some(branch) = branch {
        parts.push(format!("branch '{}' stays in the project", branch));
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
) -> Result<String, String> {
    land(&worktree, &branch, into.as_deref().filter(|s| !s.is_empty()), delete_branch, keep_floor)
}

#[tauri::command]
pub fn floor_delete(worktree: String, branch: Option<String>, keep_branch: bool, force: bool) -> Result<String, String> {
    delete(&worktree, branch.as_deref(), keep_branch, force)
}

/// Local branches of the floor's repository (for the land target picker),
/// the ground checkout's branch first.
#[tauri::command]
pub fn floor_branches(worktree: String) -> Result<Vec<String>, String> {
    let wt = PathBuf::from(&worktree);
    let ground_branch = checkouts(&wt)?.into_iter().next().and_then(|(_, b)| b);
    let out = git(&wt, &["for-each-ref", "--format=%(refname:short)", "refs/heads/"])?;
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

    fn commit(dir: &Path, file: &str, text: &str) {
        std::fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", file]).unwrap();
        git(dir, &["commit", "-q", "-m", file]).unwrap();
    }

    fn floor(repo: &Path, scene: &str, name: &str) -> PathBuf {
        let info = prepare(Some(repo.to_str().unwrap()), scene, name, None, false, false).unwrap();
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
        assert!(land(&w, "feature", Some("main"), false, false).unwrap_err().contains("uncommitted work"));
        git(&wt, &["add", "a.txt"]).unwrap();
        git(&wt, &["commit", "-q", "-m", "a"]).unwrap();

        // Flag combinations that make no sense
        assert!(land(&w, "feature", None, true, false).unwrap_err().contains("needs --into"));
        assert!(land(&w, "feature", Some("main"), true, true).unwrap_err().contains("don't go together"));
        assert!(land(&w, "feature", Some("feature"), false, false).unwrap_err().contains("own branch"));

        // Dirty ground (tracked file) → refused
        commit(&repo, "g.txt", "1");
        std::fs::write(repo.join("g.txt"), "2").unwrap();
        assert!(land(&w, "feature", Some("main"), false, false).unwrap_err().contains("ground checkout has uncommitted"));
        git(&repo, &["checkout", "--", "g.txt"]).unwrap();

        // --keep-floor merges (ground diverged → merge commit) and keeps the worktree
        let out = land(&w, "feature", Some("main"), false, true).unwrap();
        assert!(out.contains("merged 'feature' into 'main'") && !out.contains("fast-forward"), "{}", out);
        assert!(repo.join("a.txt").exists() && wt.exists());
        // More work, landed again: this time the floor goes and the branch with it
        commit(&wt, "b.txt", "b");
        let out = land(&w, "feature", Some("main"), true, false).unwrap();
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
        let out = land(&w, "fix", Some("release"), false, true).unwrap();
        assert!(out.contains("fast-forward"), "{}", out);
        assert_eq!(rev(&repo, "refs/heads/release"), rev(&repo, "refs/heads/fix"));
        assert!(!repo.join("f.txt").exists());

        // Both sides change the same file → refused, naming it; main unmoved
        commit(&repo, "f.txt", "ground");
        let before = rev(&repo, "refs/heads/main");
        let err = land(&w, "fix", Some("main"), false, false).unwrap_err();
        assert!(err.contains("conflict") && err.contains("f.txt"), "{}", err);
        assert_eq!(rev(&repo, "refs/heads/main"), before);
        assert!(wt.exists());

        // Without --into: just removes the floor, the branch stays
        let out = land(&w, "fix", None, false, false).unwrap();
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
}

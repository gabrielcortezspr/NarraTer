use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::pty::RoleSpec;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub color: String,
    pub instructions: String,
    /// Delegate-only: the agent never executes tasks itself. For claude
    /// terminals this also disables the file-editing tools at spawn.
    #[serde(default)]
    pub orchestrator: bool,
}

fn roles_path() -> PathBuf {
    config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("narrater")
        .join("roles.json")
}

#[tauri::command]
pub fn load_roles() -> Result<Vec<Role>, String> {
    let path = roles_path();
    if !path.exists() {
        return Ok(default_roles());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut roles: Vec<Role> = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    migrate_default_texts(&mut roles);
    Ok(roles)
}

#[tauri::command]
pub fn save_roles(roles: Vec<Role>) -> Result<(), String> {
    let path = roles_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&roles).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

const LEADER_INSTRUCTIONS: &str = "You are the lead agent — a pure orchestrator. You NEVER execute tasks yourself: do not edit files, run project commands or write code, no matter how trivial the request is. For every task: run `narrater list` to see your team, break the task down and delegate each piece with `narrater ask \"Name\" \"task\"` (several at once with `narrater ask --batch`). Follow up, consolidate and report the result. You are your teammates' single point of contact: answer their questions and unblock them; never redirect them to the user. If no connected agent can do the job, say so and ask for one to be connected — doing it yourself is not an option.";

const DEVELOPER_INSTRUCTIONS: &str = "You are an expert developer. Write clean code, follow best practices and write tests. Chain of command: when a task arrives from another agent (`[narrater from X]: ...`), X is your ONLY point of contact for it — your final answer goes back to X automatically when you finish your turn; for long reports or doubts, message X with `narrater ask \"X\" \"...\"`. Never ask the user about a delegated task.";

fn default_roles() -> Vec<Role> {
    vec![
        Role {
            id: "leader".to_string(),
            name: "Leader".to_string(),
            color: "#8b5cf6".to_string(),
            instructions: LEADER_INSTRUCTIONS.to_string(),
            orchestrator: true,
        },
        Role {
            id: "coder".to_string(),
            name: "Developer".to_string(),
            color: "#3b82f6".to_string(),
            instructions: DEVELOPER_INSTRUCTIONS.to_string(),
            orchestrator: false,
        },
    ]
}

/// The default roles used to teach the old MCP tools (list_peers, ask_agent,
/// reply_message…). Untouched copies are upgraded to the CLI wording; roles
/// the user edited are left alone.
fn migrate_default_texts(roles: &mut [Role]) {
    for role in roles.iter_mut() {
        let mcp_era = role.instructions.contains("list_peers") || role.instructions.contains("reply_message")
            || role.instructions.contains("canvas_connect_nodes");
        if !mcp_era {
            continue;
        }
        match role.id.as_str() {
            "leader" if role.instructions.starts_with("You are the lead agent — a pure orchestrator.") => {
                role.instructions = LEADER_INSTRUCTIONS.to_string();
            }
            "coder" if role.instructions.starts_with("You are an expert developer.") => {
                role.instructions = DEVELOPER_INSTRUCTIONS.to_string();
            }
            _ => {}
        }
    }
}

/// Instruction-file content for a role: the role itself, plus where the real
/// project lives (the agent was started inside the role directory).
pub fn build_role_content(spec: &RoleSpec, project_dir: &Path) -> String {
    let mut role = String::new();
    if let Some(name) = spec.name.as_deref().filter(|n| !n.trim().is_empty()) {
        role.push_str(&format!("Role: {}\n\n", name.trim()));
    }
    role.push_str(spec.instructions.trim());
    if spec.orchestrator {
        role.push_str(
            "\n\nOrchestrator mode (mandatory): you delegate — always. Never edit files, run project \
             commands or write code yourself; your teammates are your hands. The only commands you \
             run are `narrater` commands.",
        );
    }
    role.push_str(
        "\n\nYou work on a NarraTer canvas alongside other agents. Run `narrater list` to see your \
         connected teammates and shared notes before delegating or asking questions \
         (`narrater help` lists every command).",
    );
    format!(
        "<your_assigned_role>\n{}\n</your_assigned_role>\n\n<working_directory>\nIMPORTANT: You were started in this directory to receive the above role assignment. The actual project you should be working on is located at:\n{}\n</working_directory>\n",
        role,
        project_dir.display()
    )
}

/// Gemini CLI / Qwen Code read GEMINI.md/QWEN.md by default — point them at
/// AGENTS.md and give them access to the real project directory.
fn context_settings_json(project_dir: &Path) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "context": {
            "fileName": ["AGENTS.md"],
            "includeDirectories": [project_dir.display().to_string()],
        }
    }))
    .unwrap_or_default()
}

/// Same convention as Maestri's role directories: writes
/// `<project>/.narrater/roles/<key>/{CLAUDE.md,AGENTS.md}` (each CLI loads its
/// own file from its cwd) and returns that directory, the agent's spawn cwd.
pub fn install_role_dir(project_dir: &Path, spec: &RoleSpec, command: &str) -> Result<PathBuf, String> {
    let key: String = spec
        .key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' })
        .collect();
    if key.is_empty() {
        return Err("Cannot install role: empty key".into());
    }
    let base = project_dir.join(".narrater");
    let dir = base.join("roles").join(&key);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot install role \"{}\": could not create {} ({})", key, dir.display(), e))?;
    // Keep the generated files out of the user's git status
    let ignore = base.join(".gitignore");
    if !ignore.exists() {
        let _ = fs::write(&ignore, "*\n");
    }

    let content = build_role_content(spec, project_dir);
    for file in ["CLAUDE.md", "AGENTS.md"] {
        fs::write(dir.join(file), &content)
            .map_err(|e| format!("Cannot install role \"{}\": could not write {} ({})", key, file, e))?;
    }

    let command = command.to_lowercase();
    for (keyword, settings_dir) in [("gemini", ".gemini"), ("qwen", ".qwen")] {
        if command.contains(keyword) {
            let sdir = dir.join(settings_dir);
            let _ = fs::create_dir_all(&sdir);
            let _ = fs::write(sdir.join("settings.json"), context_settings_json(project_dir));
        }
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(key: &str, orchestrator: bool) -> RoleSpec {
        RoleSpec {
            key: key.to_string(),
            name: Some("Reviewer".to_string()),
            instructions: "Review code. Never edit files.".to_string(),
            orchestrator,
        }
    }

    #[test]
    fn role_content_wraps_role_and_points_at_project() {
        let c = build_role_content(&spec("r", false), Path::new("/work/app"));
        assert!(c.starts_with("<your_assigned_role>\nRole: Reviewer\n\nReview code. Never edit files."));
        assert!(c.contains("narrater list"));
        assert!(c.contains("<working_directory>\nIMPORTANT"));
        assert!(c.contains("\n/work/app\n</working_directory>"));
        assert!(!c.contains("Orchestrator mode"));
        assert!(build_role_content(&spec("r", true), Path::new("/w")).contains("Orchestrator mode"));
    }

    #[test]
    fn installs_claude_agents_and_gemini_files() {
        let project = std::env::temp_dir().join(format!("narrater-role-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&project).unwrap();
        let dir = install_role_dir(&project, &spec("Code Reviewer!", false), "gemini --yolo").unwrap();
        assert_eq!(dir, project.join(".narrater/roles/code-reviewer-"));
        let claude = fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert_eq!(claude, fs::read_to_string(dir.join("AGENTS.md")).unwrap());
        let settings = fs::read_to_string(dir.join(".gemini/settings.json")).unwrap();
        assert!(settings.contains("AGENTS.md"));
        assert!(!dir.join(".qwen").exists());
        assert_eq!(fs::read_to_string(project.join(".narrater/.gitignore")).unwrap(), "*\n");
        fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn migrates_untouched_mcp_era_defaults_only() {
        let mut roles = vec![
            Role { id: "leader".into(), name: "Leader".into(), color: "#000".into(),
                   instructions: "You are the lead agent — a pure orchestrator. use list_peers".into(), orchestrator: true },
            Role { id: "mine".into(), name: "Mine".into(), color: "#000".into(),
                   instructions: "custom list_peers".into(), orchestrator: false },
        ];
        migrate_default_texts(&mut roles);
        assert_eq!(roles[0].instructions, LEADER_INSTRUCTIONS);
        assert_eq!(roles[1].instructions, "custom list_peers");
    }
}

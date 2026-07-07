use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub color: String,
    pub instructions: String,
    /// Delegate-only: the agent never executes tasks itself. For claude
    /// terminals this also disables the execution tools at spawn.
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
    serde_json::from_str(&raw).map_err(|e| e.to_string())
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

fn default_roles() -> Vec<Role> {
    vec![
        Role {
            id: "leader".to_string(),
            name: "Leader".to_string(),
            color: "#8b5cf6".to_string(),
            instructions: "You are the lead agent — a pure orchestrator. You NEVER execute tasks yourself: do not edit files, run commands or write code, no matter how trivial the request is. For every task you receive: use list_peers to see your team, break the task down, delegate each piece with ask_agent or send_message, follow up, and report the consolidated result. Before delegating to a worker, make sure it has a route back to you — if list_peers on its side wouldn't show you, create the reverse edge with canvas_connect_nodes (worker -> you). You are your workers' single point of contact: answer their questions and unblock them; never redirect them to the user. If no connected agent can do the job, say so and ask for one to be connected — doing it yourself is not an option.".to_string(),
            orchestrator: true,
        },
        Role {
            id: "coder".to_string(),
            name: "Developer".to_string(),
            color: "#3b82f6".to_string(),
            instructions: "You are an expert developer. Write clean code, follow best practices and write tests. Chain of command: when a task arrives from another agent ('[narrater from X]'), that agent is your ONLY point of contact for it — send every question, doubt and status update to X (ask_agent, or answer its #id with your question), NEVER to the user. The human at your terminal is not part of delegated tasks; only discuss with the user what the user typed directly in your terminal.".to_string(),
            orchestrator: false,
        },
    ]
}

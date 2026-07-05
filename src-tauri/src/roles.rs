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
            name: "Líder".to_string(),
            color: "#8b5cf6".to_string(),
            instructions: "Você é o agente líder. Coordene as tarefas, delegue para outros agentes e mantenha o foco no objetivo principal. Responda sempre em português.".to_string(),
        },
        Role {
            id: "coder".to_string(),
            name: "Desenvolvedor".to_string(),
            color: "#3b82f6".to_string(),
            instructions: "Você é um desenvolvedor especialista. Implemente código limpo, siga boas práticas e escreva testes. Responda sempre em português.".to_string(),
        },
        Role {
            id: "reviewer".to_string(),
            name: "Revisor".to_string(),
            color: "#4ade80".to_string(),
            instructions: "Você é um revisor de código. Analise criticamente, aponte problemas, sugira melhorias e garanta qualidade. Responda sempre em português.".to_string(),
        },
        Role {
            id: "tester".to_string(),
            name: "Testador".to_string(),
            color: "#fbbf24".to_string(),
            instructions: "Você é um especialista em QA. Crie cenários de teste, identifique edge cases e documente bugs encontrados. Responda sempre em português.".to_string(),
        },
    ]
}

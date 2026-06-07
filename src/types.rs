// Data type definitions for JSON input, cache structures, and user configuration.

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct Workspace {
    pub current_dir: Option<String>,
    pub project_dir: Option<String>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct ModelInfo {
    pub id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct CurrentUsage {
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct ContextWindow {
    pub used_percentage: Option<f64>,
    pub remaining_percentage: Option<f64>,
    pub total_input_tokens: Option<u64>,
    pub total_output_tokens: Option<u64>,
    pub context_window_size: Option<u64>,
    pub current_usage: Option<CurrentUsage>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct SandboxInfo {
    pub enabled: Option<bool>,
    pub allow_network: Option<bool>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct AgentInfo {
    pub name: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct InputVcsInfo {
    #[serde(rename = "type")]
    pub vcs_type: Option<String>,
    pub client: Option<String>,
    pub branch: Option<String>,
    pub dirty: Option<bool>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct InputJson {
    pub agent_state: Option<String>,
    pub model: Option<ModelInfo>,
    pub workspace: Option<Workspace>,
    pub cwd: Option<String>,
    pub context_window: Option<ContextWindow>,
    pub sandbox: Option<SandboxInfo>,
    pub agent: Option<AgentInfo>,
    pub vcs: Option<InputVcsInfo>,
    pub product: Option<String>,
    pub artifacts: Option<Vec<serde_json::Value>>,
    pub artifact_count: Option<u32>,
    pub subagents: Option<Vec<serde_json::Value>>,
    pub background_tasks: Option<Vec<serde_json::Value>>,
    pub task_count: Option<u32>,
    pub tool_confirmation_pending: Option<bool>,
    pub pending_input_count: Option<u32>,
    pub plan_tier: Option<String>,
    pub email: Option<String>,
    pub version: Option<String>,
    pub conversation_id: Option<String>,
    pub terminal_width: Option<usize>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct QuotaItem {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "remainingFraction")]
    pub remaining_fraction: f64,
    #[serde(rename = "resetTime")]
    pub reset_time: Option<String>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct VcsInfo {
    pub cwd: String,
    pub branch: String,
    pub dirty: bool,
    #[serde(default)]
    pub ahead: u32,
    #[serde(default)]
    pub behind: u32,
    #[serde(default)]
    pub modified: u32,
    #[serde(rename = "lastChecked")]
    pub last_checked: u64,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct CacheData {
    #[serde(default)]
    pub quota: Vec<QuotaItem>,
    pub vcs: Option<VcsInfo>,
    #[serde(default, rename = "lastRefreshed")]
    pub last_refreshed: u64,
    #[serde(default)]
    pub token_hash: Option<String>,
    #[serde(default)]
    pub needs_login: Option<bool>,
}

#[derive(Serialize, Deserialize)]
pub struct LockContent {
    pub pid: u32,
    pub time: u64,
    pub cwd: String,
}

pub fn parse_input_json(input: &str) -> InputJson {
    let clean_input = input.trim();
    let clean_input = clean_input.strip_prefix('\u{feff}').unwrap_or(clean_input);

    if !clean_input.is_empty() {
        if let Ok(parsed) = serde_json::from_str::<InputJson>(clean_input) {
            return parsed;
        }
    }
    InputJson::default()
}

#[derive(Deserialize, Serialize, Clone)]
pub struct UserConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "frost".to_string()
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

pub fn load_user_config() -> UserConfig {
    let path = crate::path::resolve_antigravity_path("statusline.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<UserConfig>(&content) {
                return config;
            }
        }
    } else {
        let default_config = UserConfig::default();
        if let Ok(json_str) = serde_json::to_string_pretty(&default_config) {
            let _ = std::fs::write(&path, json_str);
        }
    }
    UserConfig::default()
}

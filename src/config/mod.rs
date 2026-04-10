use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum PushToTalkHotkey {
    #[default]
    CtrlSpace,
    CtrlGrave,
}

impl PushToTalkHotkey {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::CtrlSpace => "Ctrl+Space",
            Self::CtrlGrave => "Ctrl+`",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub assemblyai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub elevenlabs_api_key: Option<String>,
    pub elevenlabs_voice_id: Option<String>,
    pub push_to_talk_hotkey: PushToTalkHotkey,
    pub autostart_enabled: bool,
}

impl AppConfig {
    pub fn load() -> Self {
        let mut config = Self::load_from_disk().unwrap_or_default();
        config.apply_env_overrides();
        config
    }

    pub fn has_llm_provider(&self) -> bool {
        self.anthropic_api_key.is_some()
    }

    pub fn has_transcription_provider(&self) -> bool {
        self.assemblyai_api_key.is_some()
    }

    pub fn has_tts_provider(&self) -> bool {
        self.elevenlabs_api_key.is_some()
    }

    pub fn needs_onboarding(&self) -> bool {
        !self.has_llm_provider() || !self.has_transcription_provider() || !self.has_tts_provider()
    }

    pub fn save(&self) -> Result<(), String> {
        let config_path = config_file_path().ok_or_else(|| "Config directory unavailable".to_string())?;
        if let Some(parent_dir) = config_path.parent() {
            fs::create_dir_all(parent_dir).map_err(|e| format!("Failed to create config directory: {e}"))?;
        }

        let serialized =
            toml::to_string_pretty(self).map_err(|e| format!("Failed to serialize config: {e}"))?;
        fs::write(&config_path, serialized).map_err(|e| format!("Failed to write config: {e}"))
    }

    fn load_from_disk() -> Option<Self> {
        let config_path = config_file_path()?;
        let raw = fs::read_to_string(config_path).ok()?;
        toml::from_str(&raw).ok()
    }

    fn apply_env_overrides(&mut self) {
        self.assemblyai_api_key = env_override("ASSEMBLYAI_API_KEY", self.assemblyai_api_key.take());
        self.anthropic_api_key = env_override("ANTHROPIC_API_KEY", self.anthropic_api_key.take());
        self.elevenlabs_api_key = env_override("ELEVENLABS_API_KEY", self.elevenlabs_api_key.take());
        self.elevenlabs_voice_id = env_override("ELEVENLABS_VOICE_ID", self.elevenlabs_voice_id.take());
    }
}

fn env_override(name: &str, fallback: Option<String>) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or(fallback)
}

pub fn config_file_path() -> Option<PathBuf> {
    let project_dirs = ProjectDirs::from("com", "tgomareli", "clicky-desktop")?;
    Some(project_dirs.config_dir().join("config.toml"))
}

pub fn config_file_modified_at() -> Option<SystemTime> {
    let path = config_file_path()?;
    fs::metadata(path).ok()?.modified().ok()
}

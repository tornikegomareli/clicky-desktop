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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LlmProviderType {
    #[default]
    Anthropic,
    OpenRouter,
    Ollama,
    Gemini,
    Mistral,
    Custom,
}

impl LlmProviderType {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic (Claude)",
            Self::OpenRouter => "OpenRouter (Free / Multi-model)",
            Self::Ollama => "Ollama (Local / Free)",
            Self::Gemini => "Google Gemini / Antigravity",
            Self::Mistral => "Mistral AI",
            Self::Custom => "Custom OpenAI-Compatible",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Ollama => "http://localhost:11434/v1",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Mistral => "https://api.mistral.ai/v1",
            Self::Custom => "http://localhost:11434/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-sonnet-4-6",
            Self::OpenRouter => "google/gemini-2.0-flash-exp:free",
            Self::Ollama => "llama3.2-vision",
            Self::Gemini => "gemini-2.0-flash",
            Self::Mistral => "pixtral-12b-2409",
            Self::Custom => "default",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SttProviderType {
    #[default]
    AssemblyAI,
    Groq,
    OpenAiCompat,
}

impl SttProviderType {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::AssemblyAI => "AssemblyAI (Real-time stream)",
            Self::Groq => "Groq Whisper (Free & Fast)",
            Self::OpenAiCompat => "OpenAI / Local Whisper",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::AssemblyAI => "",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::OpenAiCompat => "http://localhost:8000/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::AssemblyAI => "",
            Self::Groq => "whisper-large-v3-turbo",
            Self::OpenAiCompat => "whisper-1",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TtsProviderType {
    #[default]
    ElevenLabs,
    EdgeTts,
    OpenAiCompat,
    System,
}

impl TtsProviderType {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::ElevenLabs => "ElevenLabs",
            Self::EdgeTts => "Microsoft Edge TTS (Free, no key)",
            Self::OpenAiCompat => "OpenAI / Kokoro TTS",
            Self::System => "System Fallback (espeak-ng)",
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

    // Provider selections
    #[serde(default)]
    pub llm_provider: LlmProviderType,
    #[serde(default)]
    pub stt_provider: SttProviderType,
    #[serde(default)]
    pub tts_provider: TtsProviderType,

    // LLM custom/generic settings
    #[serde(default)]
    pub custom_llm_base_url: Option<String>,
    #[serde(default)]
    pub custom_llm_api_key: Option<String>,
    #[serde(default)]
    pub custom_llm_model: Option<String>,

    // STT custom/generic settings
    #[serde(default)]
    pub groq_api_key: Option<String>,
    #[serde(default)]
    pub custom_stt_base_url: Option<String>,
    #[serde(default)]
    pub custom_stt_api_key: Option<String>,
    #[serde(default)]
    pub custom_stt_model: Option<String>,

    // TTS custom/generic settings
    #[serde(default)]
    pub custom_tts_base_url: Option<String>,
    #[serde(default)]
    pub custom_tts_api_key: Option<String>,
    #[serde(default)]
    pub custom_tts_model: Option<String>,
    #[serde(default)]
    pub custom_tts_voice: Option<String>,
}

impl AppConfig {
    pub fn load() -> Self {
        let mut config = Self::load_from_disk().unwrap_or_default();
        config.apply_env_overrides();
        config
    }

    pub fn has_llm_provider(&self) -> bool {
        match self.llm_provider {
            LlmProviderType::Anthropic => self.anthropic_api_key.is_some(),
            LlmProviderType::Ollama => true, // Local models don't require API key
            LlmProviderType::OpenRouter | LlmProviderType::Gemini | LlmProviderType::Mistral => {
                self.custom_llm_api_key.is_some()
            }
            LlmProviderType::Custom => {
                self.custom_llm_base_url.is_some()
            }
        }
    }

    pub fn has_transcription_provider(&self) -> bool {
        match self.stt_provider {
            SttProviderType::AssemblyAI => self.assemblyai_api_key.is_some(),
            SttProviderType::Groq => self.groq_api_key.is_some(),
            SttProviderType::OpenAiCompat => {
                self.custom_stt_base_url.is_some() || self.custom_stt_api_key.is_some()
            }
        }
    }

    pub fn has_tts_provider(&self) -> bool {
        match self.tts_provider {
            TtsProviderType::ElevenLabs => self.elevenlabs_api_key.is_some(),
            TtsProviderType::EdgeTts | TtsProviderType::System => true, // No API key required
            TtsProviderType::OpenAiCompat => {
                self.custom_tts_base_url.is_some() || self.custom_tts_api_key.is_some()
            }
        }
    }

    pub fn needs_onboarding(&self) -> bool {
        !self.has_llm_provider() || !self.has_transcription_provider() || !self.has_tts_provider()
    }

    pub fn save(&self) -> Result<(), String> {
        let config_path =
            config_file_path().ok_or_else(|| "Config directory unavailable".to_string())?;
        if let Some(parent_dir) = config_path.parent() {
            fs::create_dir_all(parent_dir)
                .map_err(|e| format!("Failed to create config directory: {e}"))?;
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
        self.assemblyai_api_key =
            env_override("ASSEMBLYAI_API_KEY", self.assemblyai_api_key.take());
        self.anthropic_api_key = env_override("ANTHROPIC_API_KEY", self.anthropic_api_key.take());
        self.elevenlabs_api_key =
            env_override("ELEVENLABS_API_KEY", self.elevenlabs_api_key.take());
        self.elevenlabs_voice_id =
            env_override("ELEVENLABS_VOICE_ID", self.elevenlabs_voice_id.take());

        // Generic / alternative provider overrides
        self.groq_api_key = env_override("GROQ_API_KEY", self.groq_api_key.take());

        if let Some(key) = env_override("OPENROUTER_API_KEY", None) {
            self.llm_provider = LlmProviderType::OpenRouter;
            self.custom_llm_api_key = Some(key);
        } else if let Some(key) = env_override("GEMINI_API_KEY", None) {
            self.llm_provider = LlmProviderType::Gemini;
            self.custom_llm_api_key = Some(key);
        } else if let Some(key) = env_override("MISTRAL_API_KEY", None) {
            self.llm_provider = LlmProviderType::Mistral;
            self.custom_llm_api_key = Some(key);
        } else if let Some(key) = env_override("CUSTOM_LLM_API_KEY", None) {
            self.custom_llm_api_key = Some(key);
        }

        if let Some(url) = env_override("CUSTOM_LLM_BASE_URL", None) {
            self.custom_llm_base_url = Some(url);
        }
        if let Some(model) = env_override("CUSTOM_LLM_MODEL", None) {
            self.custom_llm_model = Some(model);
        }

        if self.groq_api_key.is_some() && self.assemblyai_api_key.is_none() {
            self.stt_provider = SttProviderType::Groq;
        }

        if let Some(prov) = std::env::var("LLM_PROVIDER").ok() {
            match prov.to_lowercase().as_str() {
                "anthropic" => self.llm_provider = LlmProviderType::Anthropic,
                "openrouter" => self.llm_provider = LlmProviderType::OpenRouter,
                "ollama" => self.llm_provider = LlmProviderType::Ollama,
                "gemini" => self.llm_provider = LlmProviderType::Gemini,
                "mistral" => self.llm_provider = LlmProviderType::Mistral,
                "custom" => self.llm_provider = LlmProviderType::Custom,
                _ => {}
            }
        }

        if let Some(prov) = std::env::var("STT_PROVIDER").ok() {
            match prov.to_lowercase().as_str() {
                "assemblyai" => self.stt_provider = SttProviderType::AssemblyAI,
                "groq" => self.stt_provider = SttProviderType::Groq,
                "openaicompat" | "whisper" => self.stt_provider = SttProviderType::OpenAiCompat,
                _ => {}
            }
        }

        if let Some(prov) = std::env::var("TTS_PROVIDER").ok() {
            match prov.to_lowercase().as_str() {
                "elevenlabs" => self.tts_provider = TtsProviderType::ElevenLabs,
                "edgetts" | "edge-tts" => self.tts_provider = TtsProviderType::EdgeTts,
                "openaicompat" => self.tts_provider = TtsProviderType::OpenAiCompat,
                "system" | "espeak" => self.tts_provider = TtsProviderType::System,
                _ => {}
            }
        }
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

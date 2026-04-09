/// ElevenLabs TTS client — sends text to ElevenLabs and returns MP3 audio bytes.
/// Supports direct API key mode and Cloudflare Worker proxy mode (same dual-mode
/// pattern as claude.rs and assemblyai.rs).
///
/// Ported from ElevenLabsTTSClient.swift:1-81.

use reqwest::Client;
use serde_json::json;
use log::{debug, error};

/// Voice settings — eleven_multilingual_v2 produces natural-sounding speech.
/// flash_v2_5 is faster but noticeably robotic.
const TTS_MODEL_ID: &str = "eleven_multilingual_v2";
const TTS_STABILITY: f64 = 0.5;
const TTS_SIMILARITY_BOOST: f64 = 0.75;

/// Explicit output format for high quality MP3.
const TTS_OUTPUT_FORMAT: &str = "mp3_44100_128";

/// Default voice ID (from worker wrangler.toml).
const DEFAULT_VOICE_ID: &str = "kPzsL2i3teMYv0FxEYQ6";

/// Sends text to ElevenLabs TTS and returns the raw MP3 audio bytes.
///
/// Supports two modes:
/// - **Direct**: if `api_key` is provided, calls ElevenLabs API directly
/// - **Worker proxy**: if `worker_base_url` is provided, calls `{base_url}/tts`
///
/// Direct mode takes priority if both are provided.
pub async fn synthesize_speech(
    http_client: &Client,
    api_key: Option<&str>,
    voice_id: Option<&str>,
    worker_base_url: Option<&str>,
    text: &str,
) -> Result<Vec<u8>, TtsError> {
    let request_body = json!({
        "text": text,
        "model_id": TTS_MODEL_ID,
        "voice_settings": {
            "stability": TTS_STABILITY,
            "similarity_boost": TTS_SIMILARITY_BOOST,
        },
    });

    debug!("Requesting TTS for {} chars of text", text.len());

    let response = if let Some(key) = api_key {
        // Direct ElevenLabs API
        let vid = voice_id.unwrap_or(DEFAULT_VOICE_ID);
        let url = format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}?output_format={}",
            vid, TTS_OUTPUT_FORMAT,
        );
        debug!("TTS: direct ElevenLabs API (voice {})", vid);
        http_client
            .post(&url)
            .header("xi-api-key", key)
            .header("content-type", "application/json")
            .header("accept", "audio/mpeg")
            .json(&request_body)
            .send()
            .await
            .map_err(|err: reqwest::Error| TtsError::NetworkError(err.to_string()))?
    } else if let Some(base_url) = worker_base_url {
        // Worker proxy
        let url = format!("{}/tts", base_url);
        debug!("TTS: via worker proxy {}", url);
        http_client
            .post(&url)
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|err: reqwest::Error| TtsError::NetworkError(err.to_string()))?
    } else {
        return Err(TtsError::NetworkError("No ElevenLabs API key or worker URL configured".into()));
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_: reqwest::Error| "unable to read error body".to_string());
        error!("ElevenLabs TTS error {}: {}", status, error_body);
        return Err(TtsError::ApiError {
            status_code: status.as_u16(),
            body: error_body,
        });
    }

    let audio_bytes = response
        .bytes()
        .await
        .map_err(|err: reqwest::Error| TtsError::NetworkError(err.to_string()))?;

    debug!("TTS response: {} bytes of audio", audio_bytes.len());

    Ok(audio_bytes.to_vec())
}

#[derive(Debug)]
pub enum TtsError {
    NetworkError(String),
    ApiError { status_code: u16, body: String },
}

impl std::fmt::Display for TtsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsError::NetworkError(msg) => write!(formatter, "TTS network error: {}", msg),
            TtsError::ApiError { status_code, body } => {
                write!(formatter, "TTS API error {}: {}", status_code, body)
            }
        }
    }
}

impl std::error::Error for TtsError {}

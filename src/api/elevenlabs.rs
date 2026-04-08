/// ElevenLabs TTS client — sends text to the Cloudflare Worker /tts endpoint
/// and returns MP3 audio bytes.
/// Ported from ElevenLabsTTSClient.swift:1-81.

use reqwest::Client;
use serde_json::json;
use log::{debug, error};

/// Voice settings matching the macOS implementation.
const TTS_MODEL_ID: &str = "eleven_flash_v2_5";
const TTS_STABILITY: f64 = 0.5;
const TTS_SIMILARITY_BOOST: f64 = 0.75;

/// Sends text to ElevenLabs TTS via the Cloudflare Worker proxy and returns
/// the raw MP3 audio bytes.
///
/// The Worker at /tts holds the API key and voice ID — the client just sends
/// the text and voice settings.
pub async fn synthesize_speech(
    http_client: &Client,
    worker_base_url: &str,
    text: &str,
) -> Result<Vec<u8>, TtsError> {
    let tts_endpoint_url = format!("{}/tts", worker_base_url);

    let request_body = json!({
        "text": text,
        "model_id": TTS_MODEL_ID,
        "voice_settings": {
            "stability": TTS_STABILITY,
            "similarity_boost": TTS_SIMILARITY_BOOST,
        },
    });

    debug!("Requesting TTS for {} chars of text", text.len());

    let response = http_client
        .post(&tts_endpoint_url)
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|err: reqwest::Error| TtsError::NetworkError(err.to_string()))?;

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

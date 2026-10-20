use log::{error, info};
use reqwest::Client;
use serde_json::json;

/// OpenAI-compatible Text-To-Speech client.
/// Works with:
/// - OpenAI TTS (`https://api.openai.com/v1/audio/speech` with model `tts-1`)
/// - Local Kokoro-TTS or AllTalk servers exposing `/audio/speech`
pub async fn synthesize_speech(
    http_client: &Client,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    voice: &str,
    input_text: &str,
) -> Result<Vec<u8>, String> {
    let base_url = base_url.trim_end_matches('/');
    let url = if base_url.ends_with("/audio/speech") {
        base_url.to_string()
    } else {
        format!("{}/audio/speech", base_url)
    };

    let body = json!({
        "model": model,
        "input": input_text,
        "voice": voice,
        "response_format": "mp3"
    });

    info!("Synthesizing speech via OpenAI TTS endpoint ({url}, voice: {voice})...");

    let mut request = http_client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body);

    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to send TTS request: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let err_text = response.text().await.unwrap_or_default();
        error!("OpenAI TTS error (status {status}): {err_text}");
        return Err(format!("TTS error {status}: {err_text}"));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read MP3 audio bytes: {e}"))?;

    Ok(bytes.to_vec())
}

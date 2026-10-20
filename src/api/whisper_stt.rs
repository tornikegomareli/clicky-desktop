use log::{debug, error, info};
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde_json::Value;

/// OpenAI-compatible Audio Transcription client.
/// Works with:
/// - Groq Whisper (`https://api.groq.com/openai/v1/audio/transcriptions` with model `whisper-large-v3-turbo`)
/// - OpenAI Whisper (`https://api.openai.com/v1/audio/transcriptions` with model `whisper-1`)
/// - Local Whisper (Faster-Whisper / Whisper.cpp server running locally)
pub async fn transcribe_audio_pcm(
    http_client: &Client,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    pcm16_data: &[u8],
    sample_rate: u32,
) -> Result<String, String> {
    if pcm16_data.is_empty() {
        return Ok(String::new());
    }

    let base_url = base_url.trim_end_matches('/');
    let url = if base_url.ends_with("/audio/transcriptions") {
        base_url.to_string()
    } else {
        format!("{}/audio/transcriptions", base_url)
    };

    let wav_bytes = create_wav_buffer(pcm16_data, sample_rate, 1);

    let file_part = Part::bytes(wav_bytes)
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("Invalid mime type: {}", e))?;

    let form = Form::new()
        .text("model", model.to_string())
        .part("file", file_part)
        .text("response_format", "json");

    info!("Sending audio to Whisper STT ({url}, model: {model})...");

    let mut request = http_client.post(&url).multipart(form);

    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to send transcription request: {e}"))?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read transcription response: {e}"))?;

    if !status.is_success() {
        error!("Whisper STT error (status {status}): {body_text}");
        return Err(format!("STT API error {status}: {body_text}"));
    }

    debug!("Whisper response: {body_text}");

    let parsed: Value = serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse response JSON: {e}"))?;

    let text = parsed
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim();

    Ok(text.to_string())
}

/// Generates a standard RIFF/WAVE header for mono 16-bit PCM audio.
pub fn create_wav_buffer(pcm_data: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
    let block_align = channels * (bits_per_sample / 8);
    let subchunk2_size = pcm_data.len() as u32;
    let chunk_size = 36 + subchunk2_size;

    let mut wav = Vec::with_capacity(44 + pcm_data.len());

    // RIFF chunk descriptor
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&chunk_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // "fmt " sub-chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size for PCM
    wav.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat 1 = PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // "data" sub-chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&subchunk2_size.to_le_bytes());
    wav.extend_from_slice(pcm_data);

    wav
}

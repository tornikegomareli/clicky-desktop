/// OpenAI-compatible API client with SSE streaming.
/// Supports OpenAI's vision API format for screenshot + transcript messages.
/// Can be used with OpenAI directly or any OpenAI-compatible endpoint.

use reqwest::Client;
use serde_json::json;
use futures_util::StreamExt;
use log::{error, debug};

use super::claude::ScreenshotForClaude;

pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";

/// Streams a response from an OpenAI-compatible API via SSE.
pub async fn stream_openai_response(
    http_client: &Client,
    api_key: &str,
    model: &str,
    system_prompt: &str,
    messages: Vec<serde_json::Value>,
    mut on_text_delta: impl FnMut(&str),
) -> Result<String, OpenAiApiError> {
    let mut all_messages = vec![json!({"role": "system", "content": system_prompt})];
    all_messages.extend(messages);

    let request_body = json!({
        "model": model,
        "max_tokens": 1024,
        "messages": all_messages,
        "stream": true,
    });

    debug!("Sending OpenAI request (model: {})", model);

    let response = http_client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| OpenAiApiError::NetworkError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        error!("OpenAI API error {}: {}", status, body);
        return Err(OpenAiApiError::ApiError {
            status_code: status.as_u16(),
            body,
        });
    }

    // Parse SSE stream — OpenAI format: choices[0].delta.content
    let mut full_response = String::new();
    let mut byte_stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = chunk_result
            .map_err(|e| OpenAiApiError::StreamError(e.to_string()))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in event_text.lines() {
                if let Some(json_data) = line.strip_prefix("data: ") {
                    if json_data == "[DONE]" {
                        continue;
                    }
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_data) {
                        if let Some(content) = parsed["choices"][0]["delta"]["content"].as_str() {
                            full_response.push_str(content);
                            on_text_delta(content);
                        }
                    }
                }
            }
        }
    }

    debug!("OpenAI response complete ({} chars)", full_response.len());
    Ok(full_response)
}

/// Builds OpenAI vision message content (different format from Claude).
pub fn build_vision_message_content(
    user_transcript: &str,
    screenshots: &[ScreenshotForClaude],
) -> serde_json::Value {
    let mut content_blocks: Vec<serde_json::Value> = Vec::new();

    for screenshot in screenshots {
        let base64_image = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &screenshot.jpeg_data,
        );

        content_blocks.push(json!({
            "type": "text",
            "text": screenshot.label,
        }));

        content_blocks.push(json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:image/jpeg;base64,{}", base64_image),
            },
        }));
    }

    content_blocks.push(json!({
        "type": "text",
        "text": user_transcript,
    }));

    serde_json::Value::Array(content_blocks)
}

#[derive(Debug)]
pub enum OpenAiApiError {
    NetworkError(String),
    ApiError { status_code: u16, body: String },
    StreamError(String),
}

impl std::fmt::Display for OpenAiApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAiApiError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            OpenAiApiError::ApiError { status_code, body } => write!(f, "API error {}: {}", status_code, body),
            OpenAiApiError::StreamError(msg) => write!(f, "Stream error: {}", msg),
        }
    }
}

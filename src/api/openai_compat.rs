use crate::api::claude::ScreenshotForClaude;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use log::{debug, error, info};
use reqwest::Client;
use serde_json::json;

/// OpenAI-compatible vision completions client.
/// Works with:
/// - OpenRouter (supports free models like Gemini 2.0 Flash, Llama 3.2 Vision, Qwen 2.5 VL)
/// - Ollama (local models like llama3.2-vision, qwen2.5-vl)
/// - Google AI Studio / Gemini / Antigravity (`https://generativelanguage.googleapis.com/v1beta/openai`)
/// - Mistral AI (`https://api.mistral.ai/v1` with Pixtral)
/// - OpenAI / Codex (`https://api.openai.com/v1` with gpt-4o, gpt-4o-mini)
/// - Any custom OpenAI-compatible endpoint
pub async fn send_openai_vision_request(
    http_client: &Client,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    system_prompt: &str,
    history_exchanges: &[(String, String)],
    transcript: &str,
    screenshots: &[ScreenshotForClaude],
) -> Result<String, String> {
    let base_url = base_url.trim_end_matches('/');
    let url = if base_url.ends_with("/chat/completions") {
        base_url.to_string()
    } else {
        format!("{}/chat/completions", base_url)
    };

    let mut messages: Vec<serde_json::Value> = Vec::new();

    // System prompt
    messages.push(json!({
        "role": "system",
        "content": system_prompt
    }));

    // Historical exchanges
    for (user_text, assistant_text) in history_exchanges {
        messages.push(json!({
            "role": "user",
            "content": user_text
        }));
        messages.push(json!({
            "role": "assistant",
            "content": assistant_text
        }));
    }

    // Current turn with user transcript and attached screenshot(s)
    let mut current_user_content: Vec<serde_json::Value> = Vec::new();

    // Add user text
    current_user_content.push(json!({
        "type": "text",
        "text": transcript
    }));

    // Add screenshots formatted as data URLs
    for screenshot in screenshots {
        let b64 = BASE64_STANDARD.encode(&screenshot.jpeg_data);
        let image_url = format!("data:image/jpeg;base64,{}", b64);
        current_user_content.push(json!({
            "type": "image_url",
            "image_url": {
                "url": image_url
            }
        }));
    }

    messages.push(json!({
        "role": "user",
        "content": current_user_content
    }));

    let request_body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": 1024,
        "temperature": 0.7
    });

    info!("Sending vision request to OpenAI-compatible endpoint: {} (model: {})", url, model);

    let mut request_builder = http_client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body);

    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        request_builder = request_builder.bearer_auth(key);
    }

    let response = request_builder
        .send()
        .await
        .map_err(|e| format!("Failed to send request to {}: {}", url, e))?;

    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if !status.is_success() {
        error!("OpenAI-compatible API error (status {}): {}", status, body_text);
        return Err(format!("API error {}: {}", status, body_text));
    }

    debug!("Raw response from {}: {}", url, body_text);

    let parsed_json: serde_json::Value = serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse response JSON: {} (response: {})", e, body_text))?;

    if let Some(error_obj) = parsed_json.get("error") {
        let err_msg = error_obj
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown API error");
        return Err(format!("Provider error: {}", err_msg));
    }

    let content = parsed_json
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| format!("No content found in choices[0].message.content: {}", body_text))?;

    Ok(content.trim().to_string())
}

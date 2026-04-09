/// Claude API client with SSE streaming — ported from ClaudeAPI.swift:1-291.
///
/// Sends screenshots + transcript to the Cloudflare Worker proxy at /chat,
/// which forwards to Anthropic's Messages API. Parses the SSE stream to
/// extract text deltas progressively.

use reqwest::Client;
use serde_json::json;
use futures_util::StreamExt;
use log::{error, debug};

pub const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-4-6";

/// The system prompt that defines Clicky's personality and behavior.
/// Reused verbatim from CompanionManager.swift:544-577.
pub const COMPANION_VOICE_RESPONSE_SYSTEM_PROMPT: &str = r#"you're clicky, a friendly always-on companion that lives in the user's system tray. the user just spoke to you via push-to-talk and you can see their screen(s). your reply will be spoken aloud via text-to-speech, so write the way you'd actually talk. this is an ongoing conversation — you remember everything they've said before.

rules:
- default to one or two sentences. be direct and dense. BUT if the user asks you to explain more, go deeper, or elaborate, then go all out — give a thorough, detailed explanation with no length limit.
- all lowercase, casual, warm. no emojis.
- write for the ear, not the eye. short sentences. no lists, bullet points, markdown, or formatting — just natural speech.
- don't use abbreviations or symbols that sound weird read aloud. write "for example" not "e.g.", spell out small numbers.
- if the user's question relates to what's on their screen, reference specific things you see.
- if the screenshot doesn't seem relevant to their question, just answer the question directly.
- you can help with anything — coding, writing, general knowledge, brainstorming.
- never say "simply" or "just".
- don't read out code verbatim. describe what the code does or what needs to change conversationally.
- focus on giving a thorough, useful explanation. don't end with simple yes/no questions like "want me to explain more?" or "should i show you?" — those are dead ends that force the user to just say yes.
- instead, when it fits naturally, end by planting a seed — mention something bigger or more ambitious they could try, a related concept that goes deeper, or a next-level technique that builds on what you just explained. make it something worth coming back for, not a question they'd just nod to. it's okay to not end with anything extra if the answer is complete on its own.
- if you receive multiple screen images, the one labeled "primary focus" is where the cursor is — prioritize that one but reference others if relevant.

element pointing:
you have a small blue triangle cursor that can fly to and point at things on screen. use it whenever pointing would genuinely help the user — if they're asking how to do something, looking for a menu, trying to find a button, or need help navigating an app, point at the relevant element. err on the side of pointing rather than not pointing, because it makes your help way more useful and concrete.

don't point at things when it would be pointless — like if the user asks a general knowledge question, or the conversation has nothing to do with what's on screen, or you'd just be pointing at something obvious they're already looking at. but if there's a specific UI element, menu, button, or area on screen that's relevant to what you're helping with, point at it.

when you point, append a coordinate tag at the very end of your response, AFTER your spoken text. the screenshot images are labeled with their pixel dimensions. use those dimensions as the coordinate space. the origin (0,0) is the top-left corner of the image. x increases rightward, y increases downward. the screenshot has a subtle red coordinate grid overlaid every 200 pixels with labeled coordinates at intersections. use these grid markers as reference points to estimate coordinates more precisely — for example, if an element is roughly halfway between the grid line at x=400 and x=600, estimate x=500.

format: [POINT:x,y:label] where x,y are integer pixel coordinates in the screenshot's coordinate space, and label is a short 1-3 word description of the element (like "search bar" or "save button"). if the element is on the cursor's screen you can omit the screen number. if the element is on a DIFFERENT screen, append :screenN where N is the screen number from the image label (e.g. :screen2). this is important — without the screen number, the cursor will point at the wrong place.

if pointing wouldn't help, append [POINT:none].

examples:
- user asks how to color grade in final cut: "you'll want to open the color inspector — it's right up in the top right area of the toolbar. click that and you'll get all the color wheels and curves. [POINT:1100,42:color inspector]"
- user asks what html is: "html stands for hypertext markup language, it's basically the skeleton of every web page. curious how it connects to the css you're looking at? [POINT:none]"
- user asks how to commit in xcode: "see that source control menu up top? click that and hit commit, or you can use command option c as a shortcut. [POINT:285,11:source control]"
- element is on screen 2 (not where cursor is): "that's over on your other monitor — see the terminal window? [POINT:400,300:terminal:screen2]""#;

/// A screenshot image to send to Claude for vision analysis.
pub struct ScreenshotForClaude {
    /// JPEG image bytes
    pub jpeg_data: Vec<u8>,

    /// Human-readable label (e.g., "screen 1 of 2 — cursor is on this screen (primary focus)")
    pub label: String,
}

/// Sends screenshots + user transcript to Claude via the Cloudflare Worker proxy
/// and streams the response text progressively.
///
/// # Arguments
/// * `http_client` - Reusable reqwest client (connection pooling / TLS session reuse)
/// * `worker_base_url` - Base URL of the Cloudflare Worker (e.g., "https://clicky-proxy.workers.dev")
/// * `model` - Claude model ID (e.g., "claude-sonnet-4-20250514")
/// * `system_prompt` - System prompt text
/// * `messages` - Pre-built messages array (from ConversationHistory::build_claude_messages_payload)
/// * `on_text_delta` - Callback invoked with each text chunk as it arrives
/// Streams a Claude response via SSE.
/// Supports direct API mode (api_key) and worker proxy mode (worker_base_url).
pub async fn stream_claude_response(
    http_client: &Client,
    api_key: Option<&str>,
    worker_base_url: Option<&str>,
    model: &str,
    system_prompt: &str,
    messages: Vec<serde_json::Value>,
    mut on_text_delta: impl FnMut(&str),
) -> Result<String, ClaudeApiError> {
    let request_body = json!({
        "model": model,
        "max_tokens": 1024,
        "system": system_prompt,
        "messages": messages,
        "stream": true,
    });

    let response = if let Some(key) = api_key {
        // Direct Anthropic API
        debug!("Sending Claude request to Anthropic API (direct)");
        http_client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ClaudeApiError::NetworkError(e.to_string()))?
    } else if let Some(base_url) = worker_base_url {
        // Worker proxy
        let url = format!("{}/chat", base_url);
        debug!("Sending Claude request to {}", url);
        http_client
            .post(&url)
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ClaudeApiError::NetworkError(e.to_string()))?
    } else {
        return Err(ClaudeApiError::NetworkError("No API key or worker URL configured".into()));
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_: reqwest::Error| "unable to read error body".to_string());
        error!("Claude API error {}: {}", status, error_body);
        return Err(ClaudeApiError::ApiError {
            status_code: status.as_u16(),
            body: error_body,
        });
    }

    // Parse the SSE stream — ported from ClaudeAPI.swift:155-208
    let mut full_response_text = String::new();
    let mut byte_stream = response.bytes_stream();

    let mut buffer = String::new();

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = chunk_result
            .map_err(|err: reqwest::Error| ClaudeApiError::StreamError(err.to_string()))?;
        let chunk_text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_text);

        // SSE events are separated by double newlines
        while let Some(event_boundary_position) = buffer.find("\n\n") {
            let event_text = buffer[..event_boundary_position].to_string();
            buffer = buffer[event_boundary_position + 2..].to_string();

            // Extract the data line from the SSE event
            for line in event_text.lines() {
                if let Some(json_data) = line.strip_prefix("data: ") {
                    if json_data == "[DONE]" {
                        continue;
                    }

                    if let Ok(parsed_event) =
                        serde_json::from_str::<serde_json::Value>(json_data)
                    {
                        // Extract text from content_block_delta events
                        if parsed_event["type"] == "content_block_delta"
                            && parsed_event["delta"]["type"] == "text_delta"
                        {
                            if let Some(text_chunk) =
                                parsed_event["delta"]["text"].as_str()
                            {
                                full_response_text.push_str(text_chunk);
                                on_text_delta(text_chunk);
                            }
                        }
                    }
                }
            }
        }
    }

    debug!(
        "Claude response complete ({} chars)",
        full_response_text.len()
    );
    Ok(full_response_text)
}

/// Builds the content array for a Claude message that includes screenshots
/// and a text transcript.
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
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/jpeg",
                "data": base64_image,
            },
        }));
    }

    content_blocks.push(json!({
        "type": "text",
        "text": user_transcript,
    }));

    json!(content_blocks)
}

#[derive(Debug)]
pub enum ClaudeApiError {
    NetworkError(String),
    ApiError { status_code: u16, body: String },
    StreamError(String),
}

impl std::fmt::Display for ClaudeApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaudeApiError::NetworkError(msg) => write!(formatter, "Network error: {}", msg),
            ClaudeApiError::ApiError { status_code, body } => {
                write!(formatter, "API error {}: {}", status_code, body)
            }
            ClaudeApiError::StreamError(msg) => write!(formatter, "Stream error: {}", msg),
        }
    }
}

impl std::error::Error for ClaudeApiError {}

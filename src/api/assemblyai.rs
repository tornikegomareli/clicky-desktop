/// AssemblyAI real-time streaming transcription via WebSocket.
/// Ported from AssemblyAIStreamingTranscriptionProvider.swift:1-478.
///
/// Flow:
/// 1. Fetch a temporary token from the Cloudflare Worker /transcribe-token
/// 2. Open a WebSocket to AssemblyAI's streaming endpoint
/// 3. Stream PCM16 audio frames as binary messages
/// 4. Receive JSON turn-based transcript updates
/// 5. On hotkey release, send ForceEndpoint message and finalize

use futures_util::{SinkExt, StreamExt};
use log::{debug, error, warn};
use reqwest::Client;
use serde::Deserialize;
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// WebSocket endpoint for AssemblyAI real-time streaming.
const ASSEMBLYAI_WEBSOCKET_BASE_URL: &str = "wss://streaming.assemblyai.com/v3/ws";

/// Audio configuration matching the macOS implementation.
const AUDIO_SAMPLE_RATE: u32 = 16000;
const AUDIO_ENCODING: &str = "pcm_s16le";
const SPEECH_MODEL: &str = "u3-rt-pro";

/// Grace period after requesting final transcript before force-delivering.
const FINAL_TRANSCRIPT_GRACE_PERIOD_SECONDS: f64 = 1.4;

/// Fallback deadline if the grace period doesn't produce a result.
const FINAL_TRANSCRIPT_FALLBACK_DELAY_SECONDS: f64 = 2.8;

/// Fetches a temporary streaming token from AssemblyAI.
///
/// Supports two modes:
/// - **Direct API**: pass `api_key` to call AssemblyAI directly (for development)
/// - **Worker proxy**: pass `worker_base_url` to go through a Cloudflare Worker (for distribution)
///
/// The token is valid for 480 seconds (8 minutes).
pub async fn fetch_temporary_streaming_token(
    http_client: &Client,
    api_key: Option<&str>,
    worker_base_url: Option<&str>,
) -> Result<String, TranscriptionError> {
    let response = if let Some(key) = api_key {
        // Direct API call — GET with auth header
        http_client
            .get("https://streaming.assemblyai.com/v3/token?expires_in_seconds=480")
            .header("Authorization", key)
            .send()
            .await
            .map_err(|e| TranscriptionError::TokenFetchError(e.to_string()))?
    } else if let Some(base_url) = worker_base_url {
        // Worker proxy — proxy adds the API key
        http_client
            .post(format!("{}/transcribe-token", base_url))
            .send()
            .await
            .map_err(|e| TranscriptionError::TokenFetchError(e.to_string()))?
    } else {
        return Err(TranscriptionError::TokenFetchError(
            "No API key or worker URL configured".into(),
        ));
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(TranscriptionError::TokenFetchError(format!(
            "HTTP {}: {}", status, body
        )));
    }

    let token_response: TokenResponse = response
        .json()
        .await
        .map_err(|e| TranscriptionError::TokenFetchError(e.to_string()))?;

    debug!("Fetched AssemblyAI streaming token");
    Ok(token_response.token)
}

/// Manages a single streaming transcription session.
///
/// Audio data is sent via the `audio_sender` channel. Transcript updates
/// arrive via the `transcript_receiver` channel. Call `request_final_transcript`
/// when the user releases the hotkey.
pub struct StreamingTranscriptionSession {
    /// Send PCM16 audio data to the WebSocket
    audio_sender: mpsc::Sender<Vec<u8>>,

    /// Receive transcript updates (partial and final)
    pub transcript_receiver: mpsc::Receiver<TranscriptUpdate>,

    /// Send control messages (e.g., ForceEndpoint)
    control_sender: mpsc::Sender<ControlMessage>,
}

#[derive(Debug, Clone)]
pub enum TranscriptUpdate {
    /// Partial transcript — may change as more audio arrives
    Partial(String),

    /// Final transcript — will not change
    Final(String),

    /// Session error
    Error(String),
}

enum ControlMessage {
    RequestFinalTranscript,
    Cancel,
}

impl StreamingTranscriptionSession {
    /// Starts a new streaming session. Connects to AssemblyAI's WebSocket
    /// and spawns background tasks for sending audio and receiving transcripts.
    pub async fn start(
        temporary_auth_token: &str,
    ) -> Result<Self, TranscriptionError> {
        let websocket_url = format!(
            "{}?sample_rate={}&encoding={}&speech_model={}&token={}",
            ASSEMBLYAI_WEBSOCKET_BASE_URL, AUDIO_SAMPLE_RATE, AUDIO_ENCODING, SPEECH_MODEL,
            temporary_auth_token,
        );

        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&websocket_url)
            .header("Host", "streaming.assemblyai.com")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|err| TranscriptionError::ConnectionError(err.to_string()))?;

        let (websocket_stream, _response) =
            tokio_tungstenite::connect_async(request)
                .await
                .map_err(|err| TranscriptionError::ConnectionError(err.to_string()))?;

        debug!("Connected to AssemblyAI WebSocket");

        let (mut websocket_writer, mut websocket_reader) = websocket_stream.split();

        let (audio_sender, mut audio_receiver) = mpsc::channel::<Vec<u8>>(64);
        let (transcript_sender, transcript_receiver) = mpsc::channel::<TranscriptUpdate>(32);
        let (control_sender, mut control_receiver) = mpsc::channel::<ControlMessage>(4);

        // Audio sender task: forwards PCM16 data to the WebSocket as binary messages
        tokio::spawn(async move {
            debug!("Audio sender task started");
            loop {
                tokio::select! {
                    biased; // prioritize audio over control to avoid starving audio
                    audio_data = audio_receiver.recv() => {
                        match audio_data {
                            Some(data) => {
                                if let Err(err) = websocket_writer.send(Message::Binary(data.into())).await {
                                    error!("WebSocket send error: {}", err);
                                    break;
                                }
                            }
                            None => {
                                debug!("Audio channel closed, stopping sender task");
                                break;
                            }
                        }
                    }
                    control_message = control_receiver.recv() => {
                        match control_message {
                            Some(ControlMessage::RequestFinalTranscript) => {
                                debug!("Sending ForceEndpoint to AssemblyAI");
                                let force_endpoint_message = serde_json::json!({"type": "ForceEndpoint"});
                                if let Err(err) = websocket_writer.send(Message::Text(force_endpoint_message.to_string().into())).await {
                                    error!("WebSocket ForceEndpoint send error: {}", err);
                                }
                            }
                            Some(ControlMessage::Cancel) => {
                                debug!("Cancel received, sending Terminate");
                                let terminate = serde_json::json!({"type": "Terminate"});
                                let _ = websocket_writer.send(Message::Text(terminate.to_string().into())).await;
                                break;
                            }
                            None => {
                                debug!("Control channel closed, sending Terminate");
                                let terminate = serde_json::json!({"type": "Terminate"});
                                let _ = websocket_writer.send(Message::Text(terminate.to_string().into())).await;
                                break;
                            }
                        }
                    }
                }
            }
            debug!("Audio sender task ended");
        });

        // Transcript receiver task: parses incoming WebSocket messages and
        // tracks turn-based transcripts (ported from lines 273-347)
        let transcript_sender_for_receiver = transcript_sender.clone();
        tokio::spawn(async move {
            let mut stored_turns: BTreeMap<i64, StoredTurnTranscript> = BTreeMap::new();
            let mut has_delivered_final = false;

            while let Some(message_result) = websocket_reader.next().await {
                let message = match message_result {
                    Ok(msg) => msg,
                    Err(err) => {
                        warn!("WebSocket receive error: {}", err);
                        break;
                    }
                };

                let text = match message {
                    Message::Text(text) => text.to_string(),
                    Message::Close(frame) => {
                        debug!("WebSocket close frame: {:?}", frame);
                        break;
                    }
                    _ => continue,
                };

                debug!("WS recv: {}", &text[..text.len().min(200)]);

                // Parse the message type
                let Ok(envelope) = serde_json::from_str::<MessageEnvelope>(&text) else {
                    debug!("Could not parse message envelope, skipping");
                    continue;
                };

                match envelope.message_type.as_str() {
                    "Begin" => {
                        debug!("AssemblyAI session begun");
                        continue;
                    }
                    "SpeechStarted" => {
                        debug!("Speech detected");
                        continue;
                    }
                    "Turn" | "turn" => {
                        if let Ok(turn) = serde_json::from_str::<TurnMessage>(&text) {
                            let turn_order = turn.turn_order.unwrap_or(0);
                            let transcript_text =
                                turn.transcript.as_deref().unwrap_or("").to_string();
                            let is_formatted = turn.turn_is_formatted.unwrap_or(false);

                            // Don't overwrite formatted text with unformatted
                            if let Some(existing) = stored_turns.get(&turn_order) {
                                if existing.is_formatted && !is_formatted {
                                    continue;
                                }
                            }

                            stored_turns.insert(
                                turn_order,
                                StoredTurnTranscript {
                                    transcript_text,
                                    is_formatted,
                                },
                            );

                            // Compose full transcript from all turns in order
                            let full_transcript = compose_transcript_from_turns(&stored_turns);
                            let _ = transcript_sender_for_receiver
                                .send(TranscriptUpdate::Partial(full_transcript))
                                .await;
                        }
                    }
                    "Termination" => {
                        if !has_delivered_final {
                            has_delivered_final = true;
                            let full_transcript = compose_transcript_from_turns(&stored_turns);
                            let _ = transcript_sender_for_receiver
                                .send(TranscriptUpdate::Final(full_transcript))
                                .await;
                        }
                        break;
                    }
                    "Error" | "error" => {
                        if let Ok(error_msg) = serde_json::from_str::<ErrorMessage>(&text) {
                            let error_description = error_msg
                                .error
                                .or(error_msg.message)
                                .unwrap_or_else(|| "Unknown error".to_string());
                            let _ = transcript_sender_for_receiver
                                .send(TranscriptUpdate::Error(error_description))
                                .await;
                        }
                        break;
                    }
                    _ => {
                        debug!("Ignoring AssemblyAI message type: {}", envelope.message_type);
                    }
                }
            }

            // If we exited without delivering a final transcript, deliver what we have
            if !has_delivered_final {
                let full_transcript = compose_transcript_from_turns(&stored_turns);
                if !full_transcript.is_empty() {
                    let _ = transcript_sender_for_receiver
                        .send(TranscriptUpdate::Final(full_transcript))
                        .await;
                }
            }
        });

        Ok(Self {
            audio_sender,
            transcript_receiver,
            control_sender,
        })
    }

    /// Sends a chunk of PCM16 audio data to the WebSocket.
    pub async fn send_audio(&self, pcm16_audio_data: Vec<u8>) -> Result<(), TranscriptionError> {
        self.audio_sender
            .send(pcm16_audio_data)
            .await
            .map_err(|_| TranscriptionError::SessionClosed)
    }

    /// Requests the final transcript — called when the user releases the hotkey.
    /// Sends a ForceEndpoint message to AssemblyAI to flush pending audio.
    pub async fn request_final_transcript(&self) -> Result<(), TranscriptionError> {
        self.control_sender
            .send(ControlMessage::RequestFinalTranscript)
            .await
            .map_err(|_| TranscriptionError::SessionClosed)
    }

    /// Cancels the session and closes the WebSocket.
    pub async fn cancel(&self) {
        let _ = self.control_sender.send(ControlMessage::Cancel).await;
    }

    /// Returns a clone of the audio sender for use in a separate task.
    pub fn audio_sender(&self) -> mpsc::Sender<Vec<u8>> {
        self.audio_sender.clone()
    }

    /// Takes the transcript receiver out of the session.
    /// After calling this, transcript_receiver is replaced with a dummy channel.
    pub fn take_transcript_receiver(&mut self) -> mpsc::Receiver<TranscriptUpdate> {
        let (_dummy_tx, dummy_rx) = mpsc::channel(1);
        std::mem::replace(&mut self.transcript_receiver, dummy_rx)
    }
}

/// Composes a full transcript from all stored turns in order.
/// Turns are joined with spaces (ported from lines 331-347).
fn compose_transcript_from_turns(stored_turns: &BTreeMap<i64, StoredTurnTranscript>) -> String {
    stored_turns
        .values()
        .map(|turn| turn.transcript_text.as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// --- JSON message types ---

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Deserialize)]
struct MessageEnvelope {
    #[serde(rename = "type")]
    message_type: String,
}

#[derive(Deserialize)]
struct TurnMessage {
    transcript: Option<String>,
    turn_order: Option<i64>,
    #[allow(dead_code)]
    end_of_turn: Option<bool>,
    turn_is_formatted: Option<bool>,
}

#[derive(Deserialize)]
struct ErrorMessage {
    error: Option<String>,
    message: Option<String>,
}

struct StoredTurnTranscript {
    transcript_text: String,
    is_formatted: bool,
}

#[derive(Debug)]
pub enum TranscriptionError {
    TokenFetchError(String),
    ConnectionError(String),
    SessionClosed,
}

impl std::fmt::Display for TranscriptionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptionError::TokenFetchError(msg) => {
                write!(formatter, "Token fetch error: {}", msg)
            }
            TranscriptionError::ConnectionError(msg) => {
                write!(formatter, "Connection error: {}", msg)
            }
            TranscriptionError::SessionClosed => write!(formatter, "Session closed"),
        }
    }
}

impl std::error::Error for TranscriptionError {}

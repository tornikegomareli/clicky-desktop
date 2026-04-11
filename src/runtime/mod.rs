use crate::api;
use crate::app;
use crate::app::state_machine::PointingInstruction;
use crate::audio;
use crate::audio::UiEvent;
use crate::config;
use crate::core;
use crate::core::pcm16_converter;
use crate::screenshot;
use log::info;
use std::sync::mpsc as std_mpsc;

#[derive(Clone, PartialEq)]
pub enum LlmProvider {
    Anthropic,
    Disabled,
}

pub fn log_runtime_config_state(app_config: &config::AppConfig, simulation_mode: bool) {
    if app_config.anthropic_api_key.is_some() {
        info!("LLM: direct Anthropic API (Claude)");
    } else {
        log::warn!("Set ANTHROPIC_API_KEY to enable AI responses");
    }

    if !app_config.assemblyai_api_key.is_some() && !simulation_mode {
        log::warn!("Set ASSEMBLYAI_API_KEY to enable transcription");
    } else if app_config.assemblyai_api_key.is_some() {
        info!("Transcription: direct AssemblyAI API");
    }

    if !app_config.elevenlabs_api_key.is_some() {
        log::warn!("Set ELEVENLABS_API_KEY to enable TTS");
    } else {
        info!("TTS: direct ElevenLabs API");
    }
}

pub async fn run_transcription_bridge(
    audio_rx: std_mpsc::Receiver<audio::capture::AudioChunk>,
    ui_tx: std_mpsc::Sender<UiEvent>,
    api_key: Option<String>,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let Some(api_key) = api_key else {
        let _ = ui_tx.send(UiEvent::TranscriptionError(
            "AssemblyAI API key is not configured".into(),
        ));
        return;
    };

    let http_client = reqwest::Client::new();

    let token = match api::assemblyai::fetch_temporary_streaming_token(&http_client, &api_key).await
    {
        Ok(token) => token,
        Err(error) => {
            let _ = ui_tx.send(UiEvent::TranscriptionError(format!(
                "Token fetch failed: {error}"
            )));
            return;
        }
    };

    let mut session = match api::assemblyai::StreamingTranscriptionSession::start(&token).await {
        Ok(session) => session,
        Err(error) => {
            let _ = ui_tx.send(UiEvent::TranscriptionError(format!(
                "Session start failed: {error}"
            )));
            return;
        }
    };

    info!("AssemblyAI transcription session started");

    let mut transcript_rx = session.take_transcript_receiver();
    let audio_sender = session.audio_sender();
    let (async_audio_tx, mut async_audio_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

    tokio::task::spawn_blocking(move || {
        const MIN_CHUNK_BYTES: usize = 3200;
        let mut buffer = Vec::with_capacity(MIN_CHUNK_BYTES * 2);

        while let Ok(chunk) = audio_rx.recv() {
            let pcm16 = pcm16_converter::convert_float32_to_pcm16_mono(
                &chunk.samples,
                chunk.sample_rate,
                chunk.channels,
            );
            buffer.extend_from_slice(&pcm16);

            if buffer.len() >= MIN_CHUNK_BYTES {
                if async_audio_tx
                    .blocking_send(std::mem::take(&mut buffer))
                    .is_err()
                {
                    break;
                }
                buffer = Vec::with_capacity(MIN_CHUNK_BYTES * 2);
            }
        }

        if !buffer.is_empty() {
            let _ = async_audio_tx.blocking_send(buffer);
        }
    });

    let ui_tx_for_transcripts = ui_tx.clone();
    let (final_tx, final_rx) = tokio::sync::oneshot::channel::<()>();
    let transcript_handle = tokio::spawn(async move {
        while let Some(update) = transcript_rx.recv().await {
            let is_final = matches!(update, api::assemblyai::TranscriptUpdate::Final(_));
            let event = match update {
                api::assemblyai::TranscriptUpdate::Partial(text) => {
                    UiEvent::PartialTranscript(text)
                }
                api::assemblyai::TranscriptUpdate::Final(text) => UiEvent::FinalTranscript(text),
                api::assemblyai::TranscriptUpdate::Error(error) => {
                    UiEvent::TranscriptionError(error)
                }
            };

            if ui_tx_for_transcripts.send(event).is_err() {
                break;
            }

            if is_final {
                let _ = final_tx.send(());
                break;
            }
        }
    });

    let _ = cancel_rx.await;
    while let Some(pcm16) = async_audio_rx.recv().await {
        if audio_sender.send(pcm16).await.is_err() {
            break;
        }
    }

    info!("Audio drained, requesting final transcript...");
    let _ = session.request_final_transcript().await;

    match tokio::time::timeout(tokio::time::Duration::from_millis(1500), final_rx).await {
        Ok(_) => {}
        Err(_) => {
            let _ = ui_tx.send(UiEvent::FinalTranscript(String::new()));
            transcript_handle.abort();
        }
    }
}

pub async fn run_llm_pipeline(
    http_client: reqwest::Client,
    provider: LlmProvider,
    anthropic_api_key: Option<String>,
    elevenlabs_api_key: Option<String>,
    elevenlabs_voice_id: Option<String>,
    tts_enabled: bool,
    transcript: String,
    cursor_position: (f32, f32),
    history_exchanges: Vec<(String, String)>,
    platform: app::platform::PlatformInfo,
    ui_tx: std_mpsc::Sender<UiEvent>,
) {
    let (cursor_x, cursor_y) = cursor_position;
    let capture = match tokio::task::spawn_blocking(move || {
        screenshot::capture_primary_focus_screen(cursor_x, cursor_y, &platform)
    })
    .await
    {
        Ok(Ok(capture)) => capture,
        Ok(Err(error)) => {
            log::error!(
                "Screenshot failed: {} — cannot send to LLM without screen context",
                error
            );
            let _ = ui_tx.send(UiEvent::PipelineError(format!(
                "Screenshot failed: {error}"
            )));
            return;
        }
        Err(error) => {
            log::error!("Screenshot task panicked: {}", error);
            let _ = ui_tx.send(UiEvent::PipelineError("Screenshot task failed".into()));
            return;
        }
    };

    log_debug_screenshots(&capture);

    let mut messages = Vec::new();
    for (user_text, assistant_text) in &history_exchanges {
        messages.push(serde_json::json!({"role": "user", "content": user_text}));
        messages.push(serde_json::json!({"role": "assistant", "content": assistant_text}));
    }

    let system_prompt = api::claude::COMPANION_VOICE_RESPONSE_SYSTEM_PROMPT;

    let result: Result<String, String> = match provider {
        LlmProvider::Anthropic => {
            let Some(anthropic_api_key) = anthropic_api_key.as_deref() else {
                let _ = ui_tx.send(UiEvent::PipelineError(
                    "Anthropic API key is not configured".into(),
                ));
                return;
            };

            let user_content =
                api::claude::build_vision_message_content(&transcript, &capture.screenshots);
            messages.push(serde_json::json!({"role": "user", "content": user_content}));

            let model = api::claude::DEFAULT_CLAUDE_MODEL;
            info!("Sending to Claude ({})...", model);

            api::claude::stream_claude_response(
                &http_client,
                anthropic_api_key,
                model,
                system_prompt,
                messages,
                |_| {},
            )
            .await
            .map_err(|error| error.to_string())
        }
        LlmProvider::Disabled => unreachable!(),
    };

    match result {
        Ok(response_text) => {
            let parsed = core::point_parser::parse_claude_response(&response_text);
            let pointing_instruction = parsed.pointing.map(|pointing| PointingInstruction {
                screenshot_x: pointing.screenshot_pixel_x,
                screenshot_y: pointing.screenshot_pixel_y,
                label: pointing.element_label,
                screen_number: pointing.screen_number,
            });

            if tts_enabled && !parsed.spoken_text.is_empty() {
                if let Some(elevenlabs_api_key) = elevenlabs_api_key.clone() {
                    let tts_client = http_client.clone();
                    let tts_ui_tx = ui_tx.clone();
                    let tts_voice = elevenlabs_voice_id.clone();
                    let tts_text = parsed.spoken_text.clone();

                    tokio::spawn(async move {
                        match api::elevenlabs::synthesize_speech(
                            &tts_client,
                            &elevenlabs_api_key,
                            tts_voice.as_deref(),
                            &tts_text,
                        )
                        .await
                        {
                            Ok(mp3_bytes) => {
                                let _ = tts_ui_tx.send(UiEvent::TtsAudio(mp3_bytes));
                            }
                            Err(error) => {
                                let _ = tts_ui_tx.send(UiEvent::TtsError(error.to_string()));
                            }
                        }
                    });
                }
            }

            let _ = ui_tx.send(UiEvent::LlmResponse {
                spoken_text: parsed.spoken_text,
                pointing_instruction,
                display_infos: capture.display_infos.clone(),
                computer_use_global_coordinate: None,
            });

            if let Some(anthropic_api_key) = anthropic_api_key.clone() {
                if let Some((cursor_screenshot, cursor_display)) =
                    extract_cursor_screen_data(&capture)
                {
                    let computer_use_client = http_client.clone();
                    let computer_use_ui_tx = ui_tx.clone();
                    let computer_use_transcript = transcript.clone();

                    tokio::spawn(async move {
                        if let Some(global_coordinate) = detect_computer_use_coordinate(
                            &computer_use_client,
                            &anthropic_api_key,
                            &cursor_screenshot,
                            &cursor_display,
                            &computer_use_transcript,
                        )
                        .await
                        {
                            let _ = computer_use_ui_tx
                                .send(UiEvent::ComputerUseCoordinate(global_coordinate));
                        }
                    });
                }
            }
        }
        Err(error) => {
            log::error!("LLM API error: {}", error);
            let _ = ui_tx.send(UiEvent::PipelineError(error));
        }
    }
}

#[cfg(debug_assertions)]
fn log_debug_screenshots(capture: &screenshot::CaptureResult) {
    for (index, screenshot) in capture.screenshots.iter().enumerate() {
        let path = if cfg!(target_os = "windows") {
            format!(
                "{}\\clicky_debug_screenshot_{}.jpg",
                std::env::temp_dir().display(),
                index
            )
        } else {
            format!("/tmp/clicky_debug_screenshot_{}.jpg", index)
        };

        if let Err(error) = std::fs::write(&path, &screenshot.jpeg_data) {
            log::warn!("Failed to save debug screenshot: {}", error);
        } else {
            info!(
                "Debug screenshot saved: {} (label: {})",
                path, screenshot.label
            );
        }
    }

    for (index, display) in capture.display_infos.iter().enumerate() {
        info!(
            "Debug display {}: origin=({},{}) size={}x{} screenshot={}x{} cursor={}",
            index,
            display.global_origin_x,
            display.global_origin_y,
            display.display_width_points,
            display.display_height_points,
            display.screenshot_width_pixels,
            display.screenshot_height_pixels,
            display.is_cursor_display
        );
    }
}

#[cfg(not(debug_assertions))]
fn log_debug_screenshots(_capture: &screenshot::CaptureResult) {}

fn extract_cursor_screen_data(
    capture: &screenshot::CaptureResult,
) -> Option<(Vec<u8>, core::coordinate_mapper::DisplayInfo)> {
    capture
        .screenshots
        .iter()
        .zip(capture.display_infos.iter())
        .find(|(_, display)| display.is_cursor_display)
        .map(|(screenshot, display)| (screenshot.jpeg_data.clone(), display.clone()))
}

async fn detect_computer_use_coordinate(
    http_client: &reqwest::Client,
    anthropic_api_key: &str,
    cursor_screenshot: &[u8],
    cursor_display: &core::coordinate_mapper::DisplayInfo,
    transcript: &str,
) -> Option<(f64, f64)> {
    info!("Running Computer Use API for precise coordinate detection...");

    let coordinate = api::computer_use::detect_element_location(
        http_client,
        anthropic_api_key,
        cursor_screenshot,
        transcript,
        cursor_display.display_width_points,
        cursor_display.display_height_points,
    )
    .await?;

    let global_x = cursor_display.global_origin_x + coordinate.display_local_x;
    let global_y = cursor_display.global_origin_y + coordinate.display_local_y;
    info!(
        "Computer Use global coordinate: ({:.1}, {:.1})",
        global_x, global_y
    );
    Some((global_x, global_y))
}

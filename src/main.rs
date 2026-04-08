mod app;
mod core;
mod api;
mod overlay;
mod tray;
mod panel;
mod hotkey;
mod audio;
mod screenshot;
mod cursor_tracker;

use app::state_machine::{VoiceState, VoiceStateTransition, PointingInstruction};
use audio::UiEvent;
use audio::capture::MicCapture;
use core::audio_rms::AudioPowerLevelTracker;
use core::pcm16_converter;
use core::conversation::ConversationHistory;
use hotkey::{PushToTalkTransition, HotkeyBackend};
use overlay::renderer::{self, CursorNavigationMode, OverlayRenderState};
use tray::TrayMenuEvent;
use log::info;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};

#[derive(Clone, PartialEq)]
enum LlmProvider {
    Anthropic,
    OpenAi,
    WorkerProxy,
    Disabled,
}

fn main() {
    // Load .env file if present (ignored if missing)
    let _ = dotenvy::dotenv();
    env_logger::init();

    let platform = app::platform::detect();
    info!("Clicky Desktop starting on {}", platform);

    // Initialize GTK (required on Linux for tray-icon's menu system)
    #[cfg(target_os = "linux")]
    {
        gtk::init().expect("Failed to initialize GTK");
    }

    // Tokio runtime for async API calls (AssemblyAI, Claude, ElevenLabs)
    let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    // Shared RMS tracker — written by mic callback, read by render loop
    let rms_tracker = Arc::new(Mutex::new(AudioPowerLevelTracker::new()));
    let mut mic_capture = MicCapture::new(Arc::clone(&rms_tracker));

    // Channel for async tasks to send events to the sync render loop
    let (ui_event_tx, ui_event_rx) = std_mpsc::channel::<UiEvent>();

    // Active transcription session cancel handle
    let mut active_session_cancel: Option<tokio::sync::oneshot::Sender<()>> = None;

    // API configuration
    let assemblyai_api_key = std::env::var("ASSEMBLYAI_API_KEY").ok();
    let anthropic_api_key = std::env::var("ANTHROPIC_API_KEY").ok();
    let openai_api_key = std::env::var("OPENAI_API_KEY").ok();
    let worker_base_url = std::env::var("CLICKY_WORKER_URL").ok();
    let transcription_enabled = assemblyai_api_key.is_some() || worker_base_url.is_some();

    // LLM provider: Anthropic (preferred) > OpenAI > Worker proxy > disabled
    let llm_provider = if anthropic_api_key.is_some() {
        info!("LLM: direct Anthropic API (Claude)");
        LlmProvider::Anthropic
    } else if openai_api_key.is_some() {
        info!("LLM: direct OpenAI API");
        LlmProvider::OpenAi
    } else if worker_base_url.is_some() {
        info!("LLM: via worker proxy (Claude)");
        LlmProvider::WorkerProxy
    } else {
        log::warn!("Set ANTHROPIC_API_KEY or OPENAI_API_KEY to enable AI responses");
        LlmProvider::Disabled
    };

    if !transcription_enabled {
        log::warn!("Set ASSEMBLYAI_API_KEY or CLICKY_WORKER_URL to enable transcription");
    } else if assemblyai_api_key.is_some() {
        info!("Transcription: direct AssemblyAI API");
    } else {
        info!("Transcription: via worker proxy");
    }

    // Reusable HTTP client for API calls
    let http_client = reqwest::Client::new();

    // Conversation history (max 10 exchanges)
    let mut conversation_history = ConversationHistory::new();

    // Audio player for TTS playback (Phase 4)
    let _audio_player = match audio::playback::AudioPlayer::new() {
        Ok(player) => {
            info!("Audio output ready");
            Some(player)
        }
        Err(err) => {
            log::warn!("Audio output not available: {}", err);
            None
        }
    };

    // Initialize system tray icon
    let tray_icon = match tray::ClickyTrayIcon::new() {
        Ok(tray) => {
            info!("System tray icon ready");
            Some(tray)
        }
        Err(err) => {
            log::warn!("System tray not available: {}", err);
            None
        }
    };

    // Initialize global hotkey (platform-specific backend)
    let hotkey_manager = hotkey::create(&platform);

    // Create the overlay window
    let screen_w = 1920;
    let screen_h = 1080;
    let (mut raylib_handle, raylib_thread) = renderer::create_overlay_window(screen_w, screen_h);

    // Create platform-appropriate cursor tracker
    let cursor_tracker = cursor_tracker::create(&platform, screen_w, screen_h);

    // Initialize overlay render state
    let mut render_state = OverlayRenderState::new();
    let mut voice_state = VoiceState::Idle;

    info!("Entering main render loop at 60fps");

    let mut frame_count: u64 = 0;
    let mut last_transcript: Option<String> = None;
    let mut processing_since: Option<std::time::Instant> = None;
    let mut claude_pipeline_active = false;
    let mut responding_since: Option<std::time::Instant> = None;

    // Main render loop — runs at 60fps
    while !raylib_handle.window_should_close() {
        frame_count += 1;
        let delta_seconds = raylib_handle.get_frame_time() as f64;

        // Poll system tray menu events
        if let Some(tray) = &tray_icon {
            if let Some(event) = tray.poll_menu_event() {
                match event {
                    TrayMenuEvent::Quit => {
                        info!("Quit requested from tray menu");
                        break;
                    }
                    TrayMenuEvent::ToggleOverlay => {
                        info!("Toggle overlay requested");
                    }
                    TrayMenuEvent::OpenSettings => {
                        info!("Open settings requested");
                    }
                }
            }
        }

        // Poll global hotkey events
        if let Some(ref hotkey) = hotkey_manager {
            if let Some(transition) = hotkey.poll_hotkey_event() {
                match transition {
                    PushToTalkTransition::Pressed => {
                        info!("Push-to-talk: PRESSED");
                        if let Some(new_state) =
                            voice_state.apply(VoiceStateTransition::HotkeyPressed)
                        {
                            voice_state = new_state;
                            render_state.voice_state = voice_state;

                            // Cancel any active session
                            if let Some(cancel_tx) = active_session_cancel.take() {
                                let _ = cancel_tx.send(());
                            }
                            claude_pipeline_active = false;
                            responding_since = None;

                            // Reset RMS tracker
                            if let Ok(mut tracker) = rms_tracker.lock() {
                                tracker.reset();
                            }

                            // Start mic capture
                            match mic_capture.start() {
                                Ok(audio_rx) => {
                                    info!("Mic capture started");
                                    if transcription_enabled {
                                        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
                                        active_session_cancel = Some(cancel_tx);
                                        let ui_tx = ui_event_tx.clone();
                                        let api_key = assemblyai_api_key.clone();
                                        let worker_url = worker_base_url.clone();
                                        tokio_rt.spawn(run_transcription_bridge(audio_rx, ui_tx, api_key, worker_url, cancel_rx));
                                    }
                                }
                                Err(err) => {
                                    log::error!("Mic capture failed: {}", err);
                                    if let Some(s) = voice_state.apply(VoiceStateTransition::Error(err.to_string())) {
                                        voice_state = s;
                                        render_state.voice_state = voice_state;
                                    }
                                }
                            }
                        }
                    }
                    PushToTalkTransition::Released => {
                        info!("Push-to-talk: RELEASED");
                        mic_capture.stop();

                        if let Some(cancel_tx) = active_session_cancel.take() {
                            if let Some(new_state) = voice_state.apply(VoiceStateTransition::HotkeyReleased) {
                                voice_state = new_state;
                                render_state.voice_state = voice_state;
                            }
                            let _ = cancel_tx.send(());
                        } else {
                            voice_state = VoiceState::Idle;
                            render_state.voice_state = voice_state;
                        }
                    }
                }
            }
        }

        // Update waveform from RMS tracker while listening
        if voice_state == VoiceState::Listening {
            if let Ok(tracker) = rms_tracker.lock() {
                render_state.audio_power_history = tracker.history().to_vec();
                render_state.audio_power_level = tracker.current_level();
            }
        }

        // Poll async events
        while let Ok(event) = ui_event_rx.try_recv() {
            match event {
                UiEvent::PartialTranscript(text) => {
                    info!("Partial transcript: {}", text);
                    last_transcript = Some(text);
                    processing_since = None;
                }
                UiEvent::FinalTranscript(text) => {
                    let transcript = if text.is_empty() {
                        last_transcript.clone().unwrap_or_default()
                    } else {
                        text
                    };

                    if transcript.is_empty() {
                        voice_state = VoiceState::Idle;
                        render_state.voice_state = voice_state;
                        processing_since = None;
                    } else if llm_provider != LlmProvider::Disabled {
                        info!("FINAL transcript: {}", transcript);
                        last_transcript = Some(transcript.clone());
                        processing_since = Some(std::time::Instant::now());
                        claude_pipeline_active = true;

                        let history: Vec<(String, String)> = conversation_history
                            .exchanges()
                            .map(|e| (e.user_transcript.clone(), e.assistant_response.clone()))
                            .collect();

                        let ui_tx = ui_event_tx.clone();
                        let provider = llm_provider.clone();
                        let anthro_key = anthropic_api_key.clone();
                        let oai_key = openai_api_key.clone();
                        let worker_url = worker_base_url.clone();
                        let client = http_client.clone();
                        let cursor_pos = cursor_tracker.get_position();

                        let plat = platform.clone();
                        tokio_rt.spawn(run_llm_pipeline(
                            client, provider, anthro_key, oai_key, worker_url,
                            transcript, cursor_pos, history, plat, ui_tx,
                        ));
                    } else {
                        info!("FINAL transcript (no Claude): {}", transcript);
                        last_transcript = Some(transcript);
                        voice_state = VoiceState::Idle;
                        render_state.voice_state = voice_state;
                        processing_since = None;
                    }
                }
                UiEvent::TranscriptionError(err) => {
                    log::error!("Transcription error: {}", err);
                    voice_state = VoiceState::Idle;
                    render_state.voice_state = voice_state;
                    processing_since = None;
                }
                UiEvent::LlmResponse { spoken_text, pointing_instruction, display_infos } => {
                    if voice_state != VoiceState::Processing {
                        continue;
                    }

                    info!("LLM response: {}", spoken_text);

                    // Record in conversation history
                    if let Some(ref transcript) = last_transcript {
                        conversation_history.add_exchange(transcript.clone(), spoken_text.clone());
                    }

                    // Transition to Responding
                    if let Some(new_state) = voice_state.apply(VoiceStateTransition::ResponseReady {
                        response_text: spoken_text.clone(),
                        pointing_instruction: pointing_instruction.clone(),
                    }) {
                        voice_state = new_state;
                        render_state.voice_state = voice_state;
                    }

                    // Start flight animation if pointing
                    if let Some(ref instruction) = pointing_instruction {
                        let target = core::coordinate_mapper::find_target_display(
                            instruction.screen_number, &display_infos,
                        );
                        if let Some(display) = target {
                            let coord = core::coordinate_mapper::map_screenshot_pixels_to_global_display_coordinates(
                                instruction.screenshot_x, instruction.screenshot_y, display,
                            );
                            render_state.start_flight_to(coord.x, coord.y, spoken_text.clone());
                        }
                    }

                    // Set speech bubble text
                    render_state.speech_bubble_text = spoken_text;
                    render_state.speech_bubble_visible_char_count = render_state.speech_bubble_text.len();
                    render_state.speech_bubble_opacity = 1.0;

                    responding_since = Some(std::time::Instant::now());
                    claude_pipeline_active = false;
                    processing_since = None;
                }
                UiEvent::PipelineError(err) => {
                    log::error!("Pipeline error: {}", err);
                    voice_state = VoiceState::Idle;
                    render_state.voice_state = voice_state;
                    processing_since = None;
                    claude_pipeline_active = false;
                }
            }
        }

        // Processing timeout
        if voice_state == VoiceState::Processing {
            let now = std::time::Instant::now();
            let since = processing_since.get_or_insert(now);
            let timeout_secs = if claude_pipeline_active { 30 } else { 3 };
            if now.duration_since(*since).as_secs() >= timeout_secs {
                log::warn!("Processing timeout — returning to Idle");
                voice_state = VoiceState::Idle;
                render_state.voice_state = voice_state;
                processing_since = None;
                claude_pipeline_active = false;
            }
        }

        // Responding → Idle after speech bubble display + flight complete
        if voice_state == VoiceState::Responding {
            let now = std::time::Instant::now();
            let since = responding_since.get_or_insert(now);
            let elapsed = now.duration_since(*since).as_secs();
            let flight_done = render_state.navigation_mode != CursorNavigationMode::NavigatingToTarget;

            if elapsed >= 4 && flight_done {
                if let Some(new_state) = voice_state.apply(VoiceStateTransition::ResponseComplete) {
                    voice_state = new_state;
                    render_state.voice_state = voice_state;
                    render_state.return_to_following_mouse();
                }
                responding_since = None;
            }
        }

        // Update cursor position
        if render_state.navigation_mode == CursorNavigationMode::FollowingMouse {
            let mouse_position = raylib_handle.get_mouse_position();
            cursor_tracker.update_from_window(mouse_position.x, mouse_position.y);

            let (mx, my) = cursor_tracker.get_position();
            render_state.cursor_x = mx;
            render_state.cursor_y = my;
        }

        // Advance flight animation if active
        if render_state.navigation_mode == CursorNavigationMode::NavigatingToTarget {
            render_state.advance_flight_animation(delta_seconds);
        }

        // Draw the overlay
        let mut draw_handle = raylib_handle.begin_drawing(&raylib_thread);
        renderer::draw_overlay_frame(&mut draw_handle, &render_state);
    }

    info!("Clicky Desktop shutting down");
}

/// Async bridge: mic audio → PCM16 → AssemblyAI WebSocket → transcripts
async fn run_transcription_bridge(
    audio_rx: std_mpsc::Receiver<audio::capture::AudioChunk>,
    ui_tx: std_mpsc::Sender<UiEvent>,
    api_key: Option<String>,
    worker_url: Option<String>,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let http_client = reqwest::Client::new();

    let token = match api::assemblyai::fetch_temporary_streaming_token(
        &http_client, api_key.as_deref(), worker_url.as_deref(),
    ).await {
        Ok(t) => t,
        Err(e) => {
            let _ = ui_tx.send(UiEvent::TranscriptionError(format!("Token fetch failed: {:?}", e)));
            return;
        }
    };

    let mut session = match api::assemblyai::StreamingTranscriptionSession::start(&token).await {
        Ok(s) => s,
        Err(e) => {
            let _ = ui_tx.send(UiEvent::TranscriptionError(format!("Session start failed: {:?}", e)));
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
                &chunk.samples, chunk.sample_rate, chunk.channels,
            );
            buffer.extend_from_slice(&pcm16);
            if buffer.len() >= MIN_CHUNK_BYTES {
                if async_audio_tx.blocking_send(std::mem::take(&mut buffer)).is_err() { break; }
                buffer = Vec::with_capacity(MIN_CHUNK_BYTES * 2);
            }
        }
        if !buffer.is_empty() { let _ = async_audio_tx.blocking_send(buffer); }
    });

    let ui_tx_for_transcripts = ui_tx.clone();
    let (final_tx, final_rx) = tokio::sync::oneshot::channel::<()>();
    let transcript_handle = tokio::spawn(async move {
        while let Some(update) = transcript_rx.recv().await {
            let is_final = matches!(update, api::assemblyai::TranscriptUpdate::Final(_));
            let event = match update {
                api::assemblyai::TranscriptUpdate::Partial(text) => UiEvent::PartialTranscript(text),
                api::assemblyai::TranscriptUpdate::Final(text) => UiEvent::FinalTranscript(text),
                api::assemblyai::TranscriptUpdate::Error(err) => UiEvent::TranscriptionError(err),
            };
            if ui_tx_for_transcripts.send(event).is_err() { break; }
            if is_final { let _ = final_tx.send(()); break; }
        }
    });

    let _ = cancel_rx.await;
    while let Some(pcm16) = async_audio_rx.recv().await {
        if audio_sender.send(pcm16).await.is_err() { break; }
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

/// Async pipeline: screenshot → LLM API (Claude or OpenAI) → parse response → send to render loop
async fn run_llm_pipeline(
    http_client: reqwest::Client,
    provider: LlmProvider,
    anthropic_api_key: Option<String>,
    openai_api_key: Option<String>,
    worker_base_url: Option<String>,
    transcript: String,
    cursor_position: (f32, f32),
    history_exchanges: Vec<(String, String)>,
    platform: app::platform::PlatformInfo,
    ui_tx: std_mpsc::Sender<UiEvent>,
) {
    // Capture screenshots (blocking — JPEG encoding is CPU-bound)
    let (cx, cy) = cursor_position;
    let capture = match tokio::task::spawn_blocking(move || {
        screenshot::capture_all_screens(cx, cy, &platform)
    }).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            log::error!("Screenshot failed: {} — cannot send to LLM without screen context", e);
            let _ = ui_tx.send(UiEvent::PipelineError(format!("Screenshot failed: {}", e)));
            return;
        }
        Err(e) => {
            log::error!("Screenshot task panicked: {}", e);
            let _ = ui_tx.send(UiEvent::PipelineError("Screenshot task failed".into()));
            return;
        }
    };

    // Build messages with conversation history
    let mut messages = Vec::new();
    for (user_text, assistant_text) in &history_exchanges {
        messages.push(serde_json::json!({"role": "user", "content": user_text}));
        messages.push(serde_json::json!({"role": "assistant", "content": assistant_text}));
    }

    let system_prompt = api::claude::COMPANION_VOICE_RESPONSE_SYSTEM_PROMPT;

    let result: Result<String, String> = match provider {
        LlmProvider::Anthropic | LlmProvider::WorkerProxy => {
            let user_content = api::claude::build_vision_message_content(&transcript, &capture.screenshots);
            messages.push(serde_json::json!({"role": "user", "content": user_content}));

            let model = api::claude::DEFAULT_CLAUDE_MODEL;
            info!("Sending to Claude ({})...", model);

            api::claude::stream_claude_response(
                &http_client,
                anthropic_api_key.as_deref(),
                worker_base_url.as_deref(),
                model, system_prompt, messages, |_| {},
            ).await.map_err(|e| e.to_string())
        }
        LlmProvider::OpenAi => {
            let user_content = api::openai::build_vision_message_content(&transcript, &capture.screenshots);
            messages.push(serde_json::json!({"role": "user", "content": user_content}));

            let model = api::openai::DEFAULT_OPENAI_MODEL;
            info!("Sending to OpenAI ({})...", model);

            api::openai::stream_openai_response(
                &http_client,
                openai_api_key.as_deref().unwrap_or(""),
                model, system_prompt, messages, |_| {},
            ).await.map_err(|e| e.to_string())
        }
        LlmProvider::Disabled => unreachable!(),
    };

    match result {
        Ok(response_text) => {
            let parsed = core::point_parser::parse_claude_response(&response_text);

            let pointing = parsed.pointing.map(|p| PointingInstruction {
                screenshot_x: p.screenshot_pixel_x,
                screenshot_y: p.screenshot_pixel_y,
                label: p.element_label,
                screen_number: p.screen_number,
            });

            let _ = ui_tx.send(UiEvent::LlmResponse {
                spoken_text: parsed.spoken_text,
                pointing_instruction: pointing,
                display_infos: capture.display_infos.clone(),
            });
        }
        Err(e) => {
            log::error!("LLM API error: {}", e);
            let _ = ui_tx.send(UiEvent::PipelineError(e.to_string()));
        }
    }
}

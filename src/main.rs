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

use app::state_machine::{VoiceState, VoiceStateTransition};
use audio::UiEvent;
use audio::capture::MicCapture;
use core::audio_rms::AudioPowerLevelTracker;
use core::pcm16_converter;
use hotkey::{PushToTalkTransition, HotkeyBackend};
use overlay::renderer::{self, CursorNavigationMode, OverlayRenderState};
use tray::TrayMenuEvent;
use log::info;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};

fn main() {
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

    // Worker proxy URL for API calls (holds API keys server-side)
    let worker_base_url = std::env::var("CLICKY_WORKER_URL")
        .unwrap_or_else(|_| "https://your-worker-name.your-subdomain.workers.dev".into());
    let worker_configured = !worker_base_url.contains("your-worker-name");

    if !worker_configured {
        log::warn!("CLICKY_WORKER_URL not set — transcription disabled (mic + waveform still work)");
    }

    // Audio player for TTS playback (scaffold for Phase 3)
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

    // Create the overlay window (transparent, undecorated, topmost)
    // TODO: Detect actual screen size instead of hardcoded 1920x1080
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

                            // Cancel any active session (re-press while listening/processing)
                            if let Some(cancel_tx) = active_session_cancel.take() {
                                let _ = cancel_tx.send(());
                            }

                            // Reset RMS tracker for fresh waveform
                            if let Ok(mut tracker) = rms_tracker.lock() {
                                tracker.reset();
                            }

                            // Start mic capture
                            match mic_capture.start() {
                                Ok(audio_rx) => {
                                    info!("Mic capture started");

                                    // Spawn transcription bridge if worker is configured
                                    if worker_configured {
                                        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
                                        active_session_cancel = Some(cancel_tx);

                                        let ui_tx = ui_event_tx.clone();
                                        let url = worker_base_url.clone();
                                        tokio_rt.spawn(run_transcription_bridge(audio_rx, ui_tx, url, cancel_rx));
                                    }
                                }
                                Err(err) => {
                                    log::error!("Mic capture failed: {}", err);
                                    // Revert to Idle on mic failure
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
                        if let Some(new_state) =
                            voice_state.apply(VoiceStateTransition::HotkeyReleased)
                        {
                            voice_state = new_state;
                            render_state.voice_state = voice_state;

                            // Stop mic capture
                            mic_capture.stop();

                            // Signal transcription session to request final transcript
                            if let Some(cancel_tx) = active_session_cancel.take() {
                                let _ = cancel_tx.send(());
                            }
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

                // Log audio level every 30 frames (~0.5s) to confirm capture is working
                if frame_count % 30 == 0 {
                    info!("Audio RMS: {:.4}", tracker.current_level());
                }
            }
        }

        // Poll async events from transcription bridge
        while let Ok(event) = ui_event_rx.try_recv() {
            match event {
                UiEvent::PartialTranscript(text) => {
                    info!("Partial transcript: {}", text);
                }
                UiEvent::FinalTranscript(text) => {
                    info!("FINAL transcript: {}", text);
                    // TODO Phase 3: trigger screenshot → Claude → TTS pipeline
                    // For now, return to Idle after receiving final transcript
                    if let Some(s) = voice_state.apply(VoiceStateTransition::Error("transcript received".into())) {
                        voice_state = s;
                        render_state.voice_state = voice_state;
                    }
                }
                UiEvent::TranscriptionError(err) => {
                    log::error!("Transcription error: {}", err);
                    if let Some(s) = voice_state.apply(VoiceStateTransition::Error(err)) {
                        voice_state = s;
                        render_state.voice_state = voice_state;
                    }
                }
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

/// Async bridge between mic capture and AssemblyAI transcription.
/// Runs on the tokio runtime. Receives audio chunks from the mic,
/// converts to PCM16, streams to AssemblyAI, and forwards transcripts
/// back to the render loop via ui_event_tx.
async fn run_transcription_bridge(
    audio_rx: std_mpsc::Receiver<audio::capture::AudioChunk>,
    ui_tx: std_mpsc::Sender<UiEvent>,
    worker_base_url: String,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let http_client = reqwest::Client::new();

    // Fetch temporary streaming token from worker proxy
    let token = match api::assemblyai::fetch_temporary_streaming_token(&http_client, &worker_base_url).await {
        Ok(t) => t,
        Err(e) => {
            let _ = ui_tx.send(UiEvent::TranscriptionError(format!("Token fetch failed: {:?}", e)));
            return;
        }
    };

    // Start WebSocket session
    let mut session = match api::assemblyai::StreamingTranscriptionSession::start(&token).await {
        Ok(s) => s,
        Err(e) => {
            let _ = ui_tx.send(UiEvent::TranscriptionError(format!("Session start failed: {:?}", e)));
            return;
        }
    };

    info!("AssemblyAI transcription session started");

    // Split session: take transcript receiver and clone audio sender for concurrent use
    let mut transcript_rx = session.take_transcript_receiver();
    let audio_sender = session.audio_sender();

    // Bridge std::sync::mpsc::Receiver into async with a tokio channel
    let (async_audio_tx, mut async_audio_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

    // Blocking thread reads from cpal's std::sync::mpsc, converts to PCM16, sends to async channel
    tokio::task::spawn_blocking(move || {
        while let Ok(chunk) = audio_rx.recv() {
            let pcm16 = pcm16_converter::convert_float32_to_pcm16_mono(
                &chunk.samples,
                chunk.sample_rate,
                chunk.channels,
            );
            if async_audio_tx.blocking_send(pcm16).is_err() {
                break;
            }
        }
    });

    // Forward transcripts to UI in background
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
            if ui_tx_for_transcripts.send(event).is_err() {
                break;
            }
            if is_final {
                let _ = final_tx.send(());
                break;
            }
        }
    });

    // Forward audio to AssemblyAI until cancel signal
    tokio::select! {
        _ = async {
            while let Some(pcm16) = async_audio_rx.recv().await {
                if audio_sender.send(pcm16).await.is_err() {
                    let _ = ui_tx.send(UiEvent::TranscriptionError("Audio send failed: session closed".into()));
                    break;
                }
            }
        } => {}

        _ = cancel_rx => {
            info!("Requesting final transcript...");
            let _ = session.request_final_transcript().await;
        }
    }

    // Wait for final transcript (up to 3s)
    let _ = tokio::time::timeout(
        tokio::time::Duration::from_secs(3),
        final_rx,
    ).await;

    transcript_handle.abort();
}

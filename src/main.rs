mod api;
mod app;
mod audio;
mod autostart;
mod config;
mod core;
mod cursor_tracker;
mod hotkey;
mod overlay;
mod panel;
mod runtime;
mod screenshot;
mod tray;

use app::state_machine::{VoiceState, VoiceStateTransition};
use audio::capture::MicCapture;
use audio::UiEvent;
use core::audio_rms::AudioPowerLevelTracker;
use core::conversation::ConversationHistory;
use hotkey::PushToTalkTransition;
use log::info;
use overlay::renderer::{self, CursorNavigationMode, OverlayRenderState};
use runtime::LlmProvider;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tray::TrayMenuEvent;

fn main() {
    // Load .env file if present (ignored if missing)
    let _ = dotenvy::dotenv();
    env_logger::init();
    configure_windows_dpi_awareness();

    let mut app_config = config::AppConfig::load();

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

    #[cfg(debug_assertions)]
    let simulation_mode = std::env::var("CLICKY_SIMULATE")
        .map(|v| v != "0")
        .unwrap_or(false);
    #[cfg(not(debug_assertions))]
    let simulation_mode = false;

    #[cfg(debug_assertions)]
    let force_setup_window = std::env::var("CLICKY_FORCE_SETUP_WINDOW")
        .map(|v| v != "0")
        .unwrap_or(false);
    #[cfg(not(debug_assertions))]
    let force_setup_window = false;

    if simulation_mode {
        info!("SIMULATION MODE enabled — skipping real API calls");
    }

    if let Some(config_path) = config::config_file_path() {
        info!("Config file path: {}", config_path.display());
    }

    runtime::log_runtime_config_state(&app_config, simulation_mode);

    if force_setup_window || app_config.needs_onboarding() {
        log::warn!("Setup incomplete — opening onboarding window");
        panel::run_onboarding_blocking(app_config.clone());
        // Reload config after onboarding (user may have saved new keys)
        app_config = config::AppConfig::load();
    }

    // Reusable HTTP client for API calls
    let http_client = reqwest::Client::new();

    // Conversation history (max 10 exchanges)
    let mut conversation_history = ConversationHistory::new();

    // Audio player for TTS playback
    let mut audio_player = match audio::playback::AudioPlayer::new() {
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
    let tray_icon = match tray::ClickyTrayIcon::new(app_config.needs_onboarding()) {
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
    let mut hotkey_manager = hotkey::create(&platform, app_config.push_to_talk_hotkey);
    let mut last_config_poll_at = Instant::now();
    let mut last_config_modified_at = config::config_file_modified_at();

    // Detect overlay bounds. On Windows this spans the full virtual desktop.
    let (overlay_x, overlay_y, screen_w, screen_h) = screenshot::detect_overlay_bounds(&platform);
    info!(
        "Overlay bounds: origin=({}, {}) size={}x{}",
        overlay_x, overlay_y, screen_w, screen_h
    );

    let (mut raylib_handle, raylib_thread) =
        renderer::create_overlay_window(overlay_x, overlay_y, screen_w, screen_h, &platform);

    // Log actual Raylib window size (may differ from requested due to DPI scaling)
    let actual_w = raylib_handle.get_screen_width();
    let actual_h = raylib_handle.get_screen_height();
    info!(
        "Raylib actual window: {}x{} (requested {}x{})",
        actual_w, actual_h, screen_w, screen_h
    );

    // Create platform-appropriate cursor tracker
    #[cfg(target_os = "linux")]
    let cursor_tracker = cursor_tracker::create(&platform, screen_w, screen_h);
    #[cfg(not(target_os = "linux"))]
    let cursor_tracker = cursor_tracker::create(&platform);

    // Initialize overlay render state
    let mut render_state = OverlayRenderState::new();
    render_state.bubble_font = renderer::load_bubble_font(&mut raylib_handle, &raylib_thread);
    let mut voice_state = VoiceState::Idle;

    info!("Entering main render loop at 60fps");

    let mut frame_count: u64 = 0;
    let mut last_transcript: Option<String> = None;
    let mut processing_since: Option<std::time::Instant> = None;
    let mut claude_pipeline_active = false;
    let mut responding_since: Option<std::time::Instant> = None;
    let mut pointing_hold_since: Option<std::time::Instant> = None;

    // Main render loop — runs at 60fps
    while !raylib_handle.window_should_close() {
        frame_count += 1;
        let delta_seconds = raylib_handle.get_frame_time() as f64;
        let assemblyai_api_key = app_config.assemblyai_api_key.clone();
        let anthropic_api_key = app_config.anthropic_api_key.clone();
        let elevenlabs_api_key = app_config.elevenlabs_api_key.clone();
        let elevenlabs_voice_id = app_config.elevenlabs_voice_id.clone();
        let transcription_enabled = assemblyai_api_key.is_some();
        let tts_enabled = elevenlabs_api_key.is_some();
        let llm_provider = if anthropic_api_key.is_some() {
            LlmProvider::Anthropic
        } else {
            LlmProvider::Disabled
        };

        if last_config_poll_at.elapsed() >= Duration::from_millis(750) {
            last_config_poll_at = Instant::now();
            let current_modified_at = config::config_file_modified_at();
            if current_modified_at != last_config_modified_at {
                last_config_modified_at = current_modified_at;
                let reloaded_config = config::AppConfig::load();
                let hotkey_changed =
                    reloaded_config.push_to_talk_hotkey != app_config.push_to_talk_hotkey;
                app_config = reloaded_config;
                log::info!("Reloaded settings from config file");
                runtime::log_runtime_config_state(&app_config, simulation_mode);

                if hotkey_changed {
                    hotkey_manager = hotkey::create(&platform, app_config.push_to_talk_hotkey);
                    log::info!(
                        "Applied new push-to-talk hotkey: {}",
                        app_config.push_to_talk_hotkey.display_name()
                    );
                }
            }
        }

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
                        let latest_config = config::AppConfig::load();
                        panel::open_settings_window(
                            latest_config.clone(),
                            latest_config.needs_onboarding(),
                        );
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
                            pointing_hold_since = None;

                            // Stop TTS playback if active
                            if let Some(ref mut player) = audio_player {
                                player.stop();
                            }

                            // Reset RMS tracker
                            if let Ok(mut tracker) = rms_tracker.lock() {
                                tracker.reset();
                            }

                            // Start mic capture
                            match mic_capture.start() {
                                Ok(audio_rx) => {
                                    info!("Mic capture started");
                                    if transcription_enabled {
                                        let (cancel_tx, cancel_rx) =
                                            tokio::sync::oneshot::channel();
                                        active_session_cancel = Some(cancel_tx);
                                        let ui_tx = ui_event_tx.clone();
                                        let api_key = assemblyai_api_key.clone();
                                        tokio_rt.spawn(runtime::run_transcription_bridge(
                                            audio_rx, ui_tx, api_key, cancel_rx,
                                        ));
                                    }
                                }
                                Err(err) => {
                                    log::error!("Mic capture failed: {}", err);
                                    if let Some(s) = voice_state.apply(VoiceStateTransition::Error)
                                    {
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

                        if simulation_mode {
                            // Simulation: skip transcription, go straight to Processing
                            // and fire a fake LLM response after a short delay
                            if let Some(new_state) =
                                voice_state.apply(VoiceStateTransition::HotkeyReleased)
                            {
                                voice_state = new_state;
                                render_state.voice_state = voice_state;
                            }
                            processing_since = Some(std::time::Instant::now());

                            let ui_tx = ui_event_tx.clone();
                            let sw = screen_w;
                            let sh = screen_h;
                            tokio_rt.spawn(async move {
                                tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

                                // Random target within center 60% of screen
                                let margin_x = sw as f64 * 0.2;
                                let margin_y = sh as f64 * 0.2;
                                let range_x = sw as f64 * 0.6;
                                let range_y = sh as f64 * 0.6;
                                let rand_seed = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .subsec_nanos();
                                let target_x = margin_x + (rand_seed as f64 % range_x);
                                let target_y = margin_y + ((rand_seed / 7) as f64 % range_y);

                                let _ = ui_tx.send(UiEvent::LlmResponse {
                                    spoken_text: "simulated response for ui testing".to_string(),
                                    pointing_instruction: None,
                                    display_infos: vec![],
                                    computer_use_global_coordinate: Some((target_x, target_y)),
                                });
                            });
                        } else if let Some(cancel_tx) = active_session_cancel.take() {
                            if let Some(new_state) =
                                voice_state.apply(VoiceStateTransition::HotkeyReleased)
                            {
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

                if frame_count % 30 == 0 {
                    info!("Audio RMS: {:.4}", tracker.current_level());
                }
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
                        let el_key = elevenlabs_api_key.clone();
                        let el_voice = elevenlabs_voice_id.clone();
                        let client = http_client.clone();
                        let cursor_pos = cursor_tracker.get_position();

                        let plat = platform.clone();
                        tokio_rt.spawn(runtime::run_llm_pipeline(
                            client,
                            provider,
                            anthro_key,
                            el_key,
                            el_voice,
                            tts_enabled,
                            transcript,
                            cursor_pos,
                            history,
                            plat,
                            ui_tx,
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
                UiEvent::LlmResponse {
                    spoken_text,
                    pointing_instruction,
                    display_infos,
                    computer_use_global_coordinate,
                } => {
                    if voice_state != VoiceState::Processing {
                        continue;
                    }

                    info!("LLM response: {}", spoken_text);

                    // Record in conversation history
                    if let Some(ref transcript) = last_transcript {
                        conversation_history.add_exchange(transcript.clone(), spoken_text.clone());
                    }

                    // Transition to Responding
                    if let Some(new_state) = voice_state.apply(VoiceStateTransition::ResponseReady)
                    {
                        voice_state = new_state;
                        render_state.voice_state = voice_state;
                    }

                    // Start flight animation — prefer Computer Use coordinates (more precise)
                    // over POINT tag coordinates parsed from the response text.
                    if let Some((global_x, global_y)) = computer_use_global_coordinate {
                        info!("Using Computer Use coordinate: ({:.1}, {:.1}) — cursor at ({:.1}, {:.1})",
                            global_x, global_y, render_state.cursor_x, render_state.cursor_y);
                        render_state.start_flight_to(
                            global_x - overlay_x as f64,
                            global_y - overlay_y as f64,
                            spoken_text.clone(),
                        );
                    } else if let Some(ref instruction) = pointing_instruction {
                        info!(
                            "POINT tag: screenshot_pixel=({}, {}), label='{}', screen={:?}",
                            instruction.screenshot_x,
                            instruction.screenshot_y,
                            instruction.label,
                            instruction.screen_number
                        );

                        let target = core::coordinate_mapper::find_target_display(
                            instruction.screen_number,
                            &display_infos,
                        );
                        if let Some(display) = target {
                            info!("Target display: screen={} origin=({},{}) display_points={}x{} screenshot_px={}x{}",
                                display.screen_number,
                                display.global_origin_x, display.global_origin_y,
                                display.display_width_points, display.display_height_points,
                                display.screenshot_width_pixels, display.screenshot_height_pixels);

                            let coord = core::coordinate_mapper::map_screenshot_pixels_to_global_display_coordinates(
                                instruction.screenshot_x, instruction.screenshot_y, display,
                            );
                            info!("Mapped coordinate: ({:.1}, {:.1}) — cursor currently at ({:.1}, {:.1})",
                                coord.x, coord.y, render_state.cursor_x, render_state.cursor_y);

                            render_state.start_flight_to(
                                coord.x - overlay_x as f64,
                                coord.y - overlay_y as f64,
                                spoken_text.clone(),
                            );
                        } else {
                            log::warn!(
                                "No matching display found for screen {:?}",
                                instruction.screen_number
                            );
                        }
                    }

                    // Set friendly bubble phrase (full response goes to TTS voice)
                    let is_pointing =
                        computer_use_global_coordinate.is_some() || pointing_instruction.is_some();
                    render_state.speech_bubble_text =
                        core::bubble_text::pick_bubble_phrase(is_pointing);

                    responding_since = Some(std::time::Instant::now());
                    claude_pipeline_active = false;
                    processing_since = None;
                }
                UiEvent::ComputerUseCoordinate((global_x, global_y)) => {
                    if voice_state != VoiceState::Responding {
                        continue;
                    }

                    if render_state.navigation_mode == CursorNavigationMode::FollowingMouse {
                        info!(
                            "Using deferred Computer Use coordinate: ({:.1}, {:.1})",
                            global_x, global_y
                        );
                        render_state.start_flight_to(
                            global_x - overlay_x as f64,
                            global_y - overlay_y as f64,
                            render_state.speech_bubble_text.clone(),
                        );
                    }
                }
                UiEvent::TtsAudio(mp3_bytes) => {
                    if let Some(ref mut player) = audio_player {
                        player.play_mp3(mp3_bytes);
                        info!("TTS playback started");
                    }
                }
                UiEvent::TtsError(err) => {
                    log::warn!("TTS failed: {} — falling back to espeak-ng", err);
                    let spoken = render_state.speech_bubble_text.clone();
                    if !spoken.is_empty() {
                        std::thread::spawn(move || {
                            let _ = std::process::Command::new("espeak-ng").arg(&spoken).spawn();
                        });
                    }
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

        // Responding → hold at target → return flight → Idle
        if voice_state == VoiceState::Responding {
            let now = std::time::Instant::now();
            let since = responding_since.get_or_insert(now);
            let elapsed_total = now.duration_since(*since).as_secs();
            let tts_done = audio_player.as_ref().map_or(true, |p| !p.is_playing());

            match render_state.navigation_mode {
                CursorNavigationMode::NavigatingToTarget => {
                    // Still flying to target — wait
                }
                CursorNavigationMode::PointingAtTarget => {
                    // At target — hold for 2 seconds after TTS finishes, then return
                    if tts_done {
                        let hold_since = pointing_hold_since.get_or_insert(now);
                        let hold_elapsed = now.duration_since(*hold_since).as_secs_f64();
                        if hold_elapsed >= 2.0 {
                            let mouse_pos = cursor_tracker.get_position();
                            render_state.start_return_flight(mouse_pos.0, mouse_pos.1);
                            pointing_hold_since = None;
                        }
                    }
                }
                CursorNavigationMode::ReturningToMouse => {
                    // Flying back — wait for return flight to complete (handled by advance_flight_animation)
                }
                CursorNavigationMode::FollowingMouse => {
                    // Return flight completed → transition to Idle
                    if let Some(new_state) =
                        voice_state.apply(VoiceStateTransition::ResponseComplete)
                    {
                        voice_state = new_state;
                        render_state.voice_state = voice_state;
                    }
                    responding_since = None;
                    pointing_hold_since = None;
                }
            }

            // Safety timeout at 45 seconds
            if elapsed_total >= 45 {
                log::warn!("Responding safety timeout — returning to Idle");
                if let Some(new_state) = voice_state.apply(VoiceStateTransition::ResponseComplete) {
                    voice_state = new_state;
                    render_state.voice_state = voice_state;
                    render_state.return_to_following_mouse();
                }
                responding_since = None;
                pointing_hold_since = None;
            }
        }

        let mouse_position = raylib_handle.get_mouse_position();
        cursor_tracker.update_from_window(mouse_position.x, mouse_position.y);
        let current_mouse_position = cursor_tracker.get_position();
        let current_mouse_local_position = (
            current_mouse_position.0 - overlay_x as f32,
            current_mouse_position.1 - overlay_y as f32,
        );

        // Update cursor position
        if render_state.navigation_mode == CursorNavigationMode::FollowingMouse {
            render_state.cursor_x = current_mouse_local_position.0;
            render_state.cursor_y = current_mouse_local_position.1;
        }

        // Update overlay state (animations, typing, flight)
        render_state.update(delta_seconds, Some(current_mouse_local_position));

        // Draw the overlay
        let mut draw_handle = raylib_handle.begin_drawing(&raylib_thread);
        renderer::draw_overlay_frame(&mut draw_handle, &render_state);
    }

    info!("Clicky Desktop shutting down");
}

#[cfg(target_os = "windows")]
fn configure_windows_dpi_awareness() {
    use windows_sys::Win32::UI::HiDpi::{
        SetProcessDpiAwareness, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, PROCESS_PER_MONITOR_DPI_AWARE,
    };

    let aware_v2 =
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if aware_v2 != 0 {
        log::info!("Enabled Windows DPI awareness: Per-Monitor V2");
        return;
    }

    let per_monitor = unsafe { SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE) };
    if per_monitor >= 0 {
        log::info!("Enabled Windows DPI awareness: Per-Monitor");
    } else {
        log::warn!("Could not enable Windows DPI awareness before overlay setup");
    }
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_dpi_awareness() {}

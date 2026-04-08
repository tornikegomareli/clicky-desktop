mod app;
mod core;
mod api;
mod overlay;
mod tray;
mod panel;
mod hotkey;
mod audio;
mod screenshot;

use app::state_machine::{VoiceState, VoiceStateTransition};
use hotkey::PushToTalkTransition;
use overlay::renderer::{self, CursorNavigationMode, OverlayRenderState};
use tray::TrayMenuEvent;
use log::info;

fn main() {
    env_logger::init();

    let platform = app::platform::detect();
    info!("Clicky Desktop starting on {}", platform);

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

    // Initialize global hotkey
    let hotkey_manager = match hotkey::PushToTalkHotkeyManager::new() {
        Ok(manager) => {
            info!("Global hotkey ready (Ctrl+Space)");
            Some(manager)
        }
        Err(err) => {
            log::warn!("Global hotkey not available: {}", err);
            None
        }
    };

    // Create the overlay window (transparent, undecorated, topmost)
    // TODO: Detect actual screen size instead of hardcoded 1920x1080
    let (mut raylib_handle, raylib_thread) = renderer::create_overlay_window(1920, 1080);

    // Initialize overlay render state
    let mut render_state = OverlayRenderState::new();
    let mut voice_state = VoiceState::Idle;

    info!("Entering main render loop at 60fps");

    // Main render loop — runs at 60fps
    while !raylib_handle.window_should_close() {
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
        if let Some(hotkey) = &hotkey_manager {
            if let Some(transition) = hotkey.poll_hotkey_event() {
                match transition {
                    PushToTalkTransition::Pressed => {
                        info!("Push-to-talk: PRESSED");
                        if let Some(new_state) =
                            voice_state.apply(VoiceStateTransition::HotkeyPressed)
                        {
                            voice_state = new_state;
                            render_state.voice_state = voice_state;
                        }
                    }
                    PushToTalkTransition::Released => {
                        info!("Push-to-talk: RELEASED");
                        if let Some(new_state) =
                            voice_state.apply(VoiceStateTransition::HotkeyReleased)
                        {
                            voice_state = new_state;
                            render_state.voice_state = voice_state;

                            // For demo: simulate a response with pointing after 0.5s
                            // In production, this triggers the screenshot → Claude → TTS pipeline
                        }
                    }
                }
            }
        }

        // Update cursor position from mouse (when following mouse)
        if render_state.navigation_mode == CursorNavigationMode::FollowingMouse {
            let mouse_position = raylib_handle.get_mouse_position();
            render_state.cursor_x = mouse_position.x;
            render_state.cursor_y = mouse_position.y;
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

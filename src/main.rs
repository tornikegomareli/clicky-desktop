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
use hotkey::PushToTalkTransition;
use overlay::renderer::{self, CursorNavigationMode, OverlayRenderState};
use tray::TrayMenuEvent;
use log::info;

fn main() {
    env_logger::init();

    let platform = app::platform::detect();
    info!("Clicky Desktop starting on {}", platform);

    // Initialize GTK (required on Linux for tray-icon's menu system)
    #[cfg(target_os = "linux")]
    {
        gtk::init().expect("Failed to initialize GTK");
    }

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

        // Update cursor position
        if render_state.navigation_mode == CursorNavigationMode::FollowingMouse {
            // Feed Raylib's mouse position (used by fallback tracker on X11)
            let mouse_position = raylib_handle.get_mouse_position();
            cursor_tracker.update_from_window(mouse_position.x, mouse_position.y);

            let (mx, my) = cursor_tracker.get_position();
            render_state.cursor_x = mx;
            render_state.cursor_y = my;

            if frame_count % 60 == 0 {
                info!(
                    "Mouse -> triangle: ({:.0}, {:.0}) | mode: {:?}",
                    render_state.cursor_x, render_state.cursor_y,
                    render_state.navigation_mode
                );
            }
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

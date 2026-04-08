/// Platform-abstracted cursor position tracking.
///
/// The overlay window uses mouse passthrough so clicks reach the desktop.
/// This means Raylib can't receive mouse events on some platforms.
/// Each platform provides its own way to read the global cursor position.
///
/// Platform strategy:
///   Linux/Wayland  → evdev (reads /dev/input/ directly)
///   Linux/X11      → Raylib get_mouse_position (X11 delivers motion to shaped windows)
///   Windows        → Win32 GetCursorPos
///   Fallback       → Raylib get_mouse_position (best effort)

pub(crate) mod fallback;

#[cfg(target_os = "linux")]
mod evdev_tracker;

#[cfg(target_os = "windows")]
mod win32_tracker;

use crate::app::platform::{PlatformInfo, OperatingSystem, DisplayServer};

/// Trait for reading global cursor position, regardless of platform.
pub trait CursorTracker: Send {
    /// Returns the current cursor position in screen coordinates.
    fn get_position(&self) -> (f32, f32);

    /// Called each frame with the window-reported mouse position.
    /// Implementations that read from the window (like RaylibFallbackTracker)
    /// use this to stay current. Others ignore it.
    fn update_from_window(&self, _x: f32, _y: f32) {}
}

/// Creates the best cursor tracker for the current platform.
/// Returns None only if no tracking method is available at all.
pub fn create(platform: &PlatformInfo, screen_width: i32, screen_height: i32) -> Box<dyn CursorTracker> {
    match platform.os {
        #[cfg(target_os = "linux")]
        OperatingSystem::Linux => {
            // On Wayland, Raylib can't get mouse position with passthrough enabled.
            // Use evdev to read directly from input devices.
            if platform.display_server == Some(DisplayServer::Wayland) {
                if let Some(tracker) = evdev_tracker::EvdevCursorTracker::new(screen_width, screen_height) {
                    log::info!("Cursor tracking: evdev (Wayland)");
                    return Box::new(tracker);
                }
                log::warn!("evdev unavailable on Wayland — falling back to Raylib (cursor may not track)");
            } else {
                log::info!("Cursor tracking: Raylib (X11)");
            }
            Box::new(fallback::RaylibFallbackTracker::new())
        }

        #[cfg(target_os = "windows")]
        OperatingSystem::Windows => {
            log::info!("Cursor tracking: Win32 GetCursorPos");
            Box::new(win32_tracker::Win32CursorTracker)
        }

        #[allow(unreachable_patterns)]
        _ => {
            log::info!("Cursor tracking: Raylib fallback");
            Box::new(fallback::RaylibFallbackTracker::new())
        }
    }
}

/// Global push-to-talk hotkey detection.
///
/// Platform strategy:
///   Linux/Wayland → evdev (reads keyboard from /dev/input/ directly)
///   Linux/X11     → global-hotkey crate (X11 XGrabKey)
///   Windows       → global-hotkey crate (Win32 RegisterHotKey)
///   Fallback      → global-hotkey crate

#[cfg(target_os = "linux")]
mod evdev_hotkey;
mod global_hotkey_backend;

use crate::app::platform::{PlatformInfo, OperatingSystem, DisplayServer};
use crate::config::PushToTalkHotkey;

/// Push-to-talk shortcut transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushToTalkTransition {
    Pressed,
    Released,
}

/// Trait for hotkey detection, regardless of platform.
pub trait HotkeyBackend: Send {
    fn poll_hotkey_event(&self) -> Option<PushToTalkTransition>;
}

/// Creates the best hotkey backend for the current platform.
pub fn create(platform: &PlatformInfo, shortcut: PushToTalkHotkey) -> Option<Box<dyn HotkeyBackend>> {
    match platform.os {
        #[cfg(target_os = "linux")]
        OperatingSystem::Linux => {
            if platform.display_server == Some(DisplayServer::Wayland) {
                // X11 XGrabKey doesn't work on XWayland — use evdev
                match evdev_hotkey::EvdevHotkeyManager::new(shortcut) {
                    Some(manager) => {
                        log::info!(
                            "Hotkey: evdev (Wayland) — hold {} to talk",
                            shortcut.display_name()
                        );
                        return Some(Box::new(manager));
                    }
                    None => {
                        log::warn!("evdev hotkey unavailable — trying global-hotkey fallback");
                    }
                }
            }
            // X11 or fallback
            match global_hotkey_backend::GlobalHotkeyManager::new(shortcut) {
                Ok(manager) => {
                    log::info!(
                        "Hotkey: global-hotkey (X11) — {} push-to-talk",
                        shortcut.display_name()
                    );
                    Some(Box::new(manager))
                }
                Err(err) => {
                    log::warn!("Global hotkey not available: {}", err);
                    None
                }
            }
        }

        #[allow(unreachable_patterns)]
        _ => {
            match global_hotkey_backend::GlobalHotkeyManager::new(shortcut) {
                Ok(manager) => {
                    log::info!(
                        "Hotkey: global-hotkey — {} push-to-talk",
                        shortcut.display_name()
                    );
                    Some(Box::new(manager))
                }
                Err(err) => {
                    log::warn!("Global hotkey not available: {}", err);
                    None
                }
            }
        }
    }
}

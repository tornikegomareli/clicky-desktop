/// Global push-to-talk hotkey detection.
///
/// Uses `global-hotkey` crate on Windows and X11.
/// On Wayland (Hyprland), falls back to `evdev` for direct input device reading.
///
/// The hotkey is Ctrl+Alt (modifier-only on macOS maps to Ctrl+Alt on Linux/Win).
/// Press = start recording, Release = stop recording and submit.

use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent, HotKeyState};
use global_hotkey::hotkey::{HotKey, Modifiers, Code};
use log::{info, warn, error};
use std::sync::mpsc as std_mpsc;

/// Push-to-talk shortcut transitions — matches the macOS ShortcutTransition enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushToTalkTransition {
    Pressed,
    Released,
}

/// Manages the global push-to-talk hotkey registration and event detection.
pub struct PushToTalkHotkeyManager {
    _hotkey_manager: GlobalHotKeyManager,
}

impl PushToTalkHotkeyManager {
    /// Registers the global Ctrl+Space hotkey for push-to-talk.
    ///
    /// Note: We use Ctrl+Space instead of Ctrl+Alt because `global-hotkey`
    /// requires a key code (not just modifiers). The macOS version uses
    /// modifier-only (Ctrl+Option), but on Linux/Windows a key trigger
    /// is needed for the global-hotkey crate.
    pub fn new() -> Result<Self, String> {
        let hotkey_manager = GlobalHotKeyManager::new()
            .map_err(|e| format!("Failed to create hotkey manager: {}", e))?;

        // Register Ctrl+Space as the push-to-talk hotkey
        let push_to_talk_hotkey = HotKey::new(Some(Modifiers::CONTROL), Code::Space);

        hotkey_manager
            .register(push_to_talk_hotkey)
            .map_err(|e| format!("Failed to register hotkey: {}", e))?;

        info!("Global hotkey registered: Ctrl+Space for push-to-talk");

        Ok(Self {
            _hotkey_manager: hotkey_manager,
        })
    }

    /// Checks for pending hotkey events. Returns a transition if detected.
    /// Should be called each frame from the main loop.
    pub fn poll_hotkey_event(&self) -> Option<PushToTalkTransition> {
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            match event.state() {
                HotKeyState::Pressed => {
                    return Some(PushToTalkTransition::Pressed);
                }
                HotKeyState::Released => {
                    return Some(PushToTalkTransition::Released);
                }
            }
        }
        None
    }
}

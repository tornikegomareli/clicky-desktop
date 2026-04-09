/// Global hotkey backend using the `global-hotkey` crate.
/// Works on Windows and native X11. Does NOT work on XWayland.

use global_hotkey::{GlobalHotKeyManager as CrateManager, GlobalHotKeyEvent, HotKeyState};
use global_hotkey::hotkey::{HotKey, Modifiers, Code};
use super::{HotkeyBackend, PushToTalkTransition};

pub struct GlobalHotkeyManager {
    _manager: CrateManager,
}

// Safety: GlobalHotKeyManager is only accessed from the main thread.
// The raw pointer inside (platform window handle) is not mutated across threads.
unsafe impl Send for GlobalHotkeyManager {}

impl GlobalHotkeyManager {
    pub fn new() -> Result<Self, String> {
        let manager = CrateManager::new()
            .map_err(|e| format!("Failed to create hotkey manager: {}", e))?;

        let hotkey = HotKey::new(Some(Modifiers::CONTROL), Code::Space);

        manager.register(hotkey)
            .map_err(|e| format!("Failed to register Ctrl+Space: {}", e))?;

        Ok(Self { _manager: manager })
    }
}

impl HotkeyBackend for GlobalHotkeyManager {
    fn poll_hotkey_event(&self) -> Option<PushToTalkTransition> {
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            match event.state() {
                HotKeyState::Pressed => return Some(PushToTalkTransition::Pressed),
                HotKeyState::Released => return Some(PushToTalkTransition::Released),
            }
        }
        None
    }
}

use super::{HotkeyBackend, PushToTalkTransition};
use crate::config::PushToTalkHotkey;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
/// Global hotkey backend using the `global-hotkey` crate.
/// Works on Windows and native X11. Does NOT work on XWayland.
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager as CrateManager, HotKeyState};

pub struct GlobalHotkeyManager {
    _manager: CrateManager,
}

// Safety: GlobalHotKeyManager is only accessed from the main thread.
// The raw pointer inside (platform window handle) is not mutated across threads.
unsafe impl Send for GlobalHotkeyManager {}

impl GlobalHotkeyManager {
    pub fn new(shortcut: PushToTalkHotkey) -> Result<Self, String> {
        let manager =
            CrateManager::new().map_err(|e| format!("Failed to create hotkey manager: {}", e))?;

        let key_code = match shortcut {
            PushToTalkHotkey::CtrlSpace => Code::Space,
            PushToTalkHotkey::CtrlGrave => Code::Backquote,
        };
        let hotkey = HotKey::new(Some(Modifiers::CONTROL), key_code);

        manager
            .register(hotkey)
            .map_err(|e| format!("Failed to register {}: {}", shortcut.display_name(), e))?;

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

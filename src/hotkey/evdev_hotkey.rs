/// Evdev-based hotkey detection for Linux Wayland.
/// Reads keyboard events directly from /dev/input/ since X11 XGrabKey
/// doesn't work on XWayland.
///
/// Push-to-talk: Hold Ctrl+` (tilda/grave) to record, release to stop.
/// Requires user to be in the 'input' group.

use evdev::{Device, InputEventKind, Key};
use std::sync::mpsc;
use super::{HotkeyBackend, PushToTalkTransition};
use crate::config::PushToTalkHotkey;

pub struct EvdevHotkeyManager {
    event_rx: mpsc::Receiver<PushToTalkTransition>,
}

impl EvdevHotkeyManager {
    pub fn new(shortcut: PushToTalkHotkey) -> Option<Self> {
        let device = find_keyboard_device()?;
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            run_hotkey_loop(device, tx, shortcut);
        });

        Some(Self { event_rx: rx })
    }
}

impl HotkeyBackend for EvdevHotkeyManager {
    fn poll_hotkey_event(&self) -> Option<PushToTalkTransition> {
        // Drain all pending events, return only the last one
        // (avoids stale press/release pairs from batched events)
        let mut last = None;
        while let Ok(event) = self.event_rx.try_recv() {
            last = Some(event);
        }
        last
    }
}

fn run_hotkey_loop(
    mut device: Device,
    tx: mpsc::Sender<PushToTalkTransition>,
    shortcut: PushToTalkHotkey,
) {
    log::info!(
        "evdev hotkey listener started on: {}",
        device.name().unwrap_or("unknown")
    );

    let mut ctrl_held = false;
    let mut trigger_held = false;
    let mut combo_active = false;

    loop {
        match device.fetch_events() {
            Ok(events) => {
                for event in events {
                    if let InputEventKind::Key(key) = event.kind() {
                        let pressed = event.value() != 0; // 1=press, 2=repeat, 0=release

                        match key {
                            Key::KEY_LEFTCTRL | Key::KEY_RIGHTCTRL => {
                                ctrl_held = pressed;
                            }
                            Key::KEY_GRAVE if shortcut == PushToTalkHotkey::CtrlGrave => {
                                trigger_held = pressed;
                            }
                            Key::KEY_SPACE if shortcut == PushToTalkHotkey::CtrlSpace => {
                                trigger_held = pressed;
                            }
                            _ => continue,
                        }

                        let both_held = ctrl_held && trigger_held;

                        if both_held && !combo_active {
                            combo_active = true;
                            let _ = tx.send(PushToTalkTransition::Pressed);
                        } else if !both_held && combo_active {
                            combo_active = false;
                            let _ = tx.send(PushToTalkTransition::Released);
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("evdev hotkey read error: {}", e);
                break;
            }
        }
    }
}

/// Find a keyboard device in /dev/input/
fn find_keyboard_device() -> Option<Device> {
    for (path, device) in evdev::enumerate() {
        if let Some(keys) = device.supported_keys() {
            if keys.contains(Key::KEY_LEFTCTRL)
                && keys.contains(Key::KEY_GRAVE)
                && keys.contains(Key::KEY_SPACE)
                && keys.contains(Key::KEY_A)
            {
                log::info!(
                    "Found keyboard device: {} ({:?})",
                    device.name().unwrap_or("unknown"),
                    path
                );
                for (p2, d2) in evdev::enumerate() {
                    if p2 == path {
                        return Some(d2);
                    }
                }
            }
        }
    }
    log::warn!("No keyboard device found in /dev/input/. Is user in 'input' group?");
    None
}

/// Windows cursor tracker using Win32 GetCursorPos.
/// Works regardless of window passthrough state.
use super::CursorTracker;

pub struct Win32CursorTracker;

impl CursorTracker for Win32CursorTracker {
    fn get_position(&self) -> (f32, f32) {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
            let mut point = POINT { x: 0, y: 0 };
            unsafe {
                GetCursorPos(&mut point);
            }
            (point.x as f32, point.y as f32)
        }

        #[cfg(not(target_os = "windows"))]
        {
            (0.0, 0.0)
        }
    }
}

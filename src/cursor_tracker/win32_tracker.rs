use super::CursorTracker;

/// Windows cursor tracker using Win32 GetCursorPos.
/// Works even when the overlay window is click-through.
pub struct Win32CursorTracker;

impl CursorTracker for Win32CursorTracker {
    fn get_position(&self) -> (f32, f32) {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let mut point = POINT { x: 0, y: 0 };
        let ok = unsafe { GetCursorPos(&mut point) };
        if ok == 0 {
            return (0.0, 0.0);
        }

        (point.x as f32, point.y as f32)
    }
}

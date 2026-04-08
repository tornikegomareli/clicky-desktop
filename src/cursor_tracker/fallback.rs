/// Fallback cursor tracker that stores position set by the main loop.
/// Used on X11 (where Raylib can track mouse with passthrough) and
/// as a last resort on any platform.

use std::sync::atomic::{AtomicI32, Ordering};
use super::CursorTracker;

pub struct RaylibFallbackTracker {
    x: AtomicI32,
    y: AtomicI32,
}

impl RaylibFallbackTracker {
    pub fn new() -> Self {
        Self {
            x: AtomicI32::new(0),
            y: AtomicI32::new(0),
        }
    }

    /// Called from the main loop to feed Raylib's mouse position into the tracker.
    pub fn update(&self, x: f32, y: f32) {
        self.x.store(x as i32, Ordering::Relaxed);
        self.y.store(y as i32, Ordering::Relaxed);
    }
}

impl CursorTracker for RaylibFallbackTracker {
    fn get_position(&self) -> (f32, f32) {
        (
            self.x.load(Ordering::Relaxed) as f32,
            self.y.load(Ordering::Relaxed) as f32,
        )
    }

    fn update_from_window(&self, x: f32, y: f32) {
        self.update(x, y);
    }
}

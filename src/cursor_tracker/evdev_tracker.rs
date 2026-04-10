use super::CursorTracker;
/// Linux evdev cursor tracker.
/// Reads raw input events from /dev/input/ to track cursor position
/// independently of the window system. Works on both Wayland and X11.
///
/// Requires the user to be in the 'input' group.
use evdev::{AbsoluteAxisType, Device, InputEventKind, RelativeAxisType};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

pub struct EvdevCursorTracker {
    x: Arc<AtomicI32>,
    y: Arc<AtomicI32>,
}

impl EvdevCursorTracker {
    pub fn new(screen_width: i32, screen_height: i32) -> Option<Self> {
        let mut device = find_pointing_device()?;

        // Read absolute axis ranges for mapping to screen coordinates
        let abs_x_range = device.get_abs_state().ok().map(|states| {
            let info = states[AbsoluteAxisType::ABS_X.0 as usize];
            (info.minimum, info.maximum)
        });
        let abs_y_range = device.get_abs_state().ok().map(|states| {
            let info = states[AbsoluteAxisType::ABS_Y.0 as usize];
            (info.minimum, info.maximum)
        });

        let x = Arc::new(AtomicI32::new(screen_width / 2));
        let y = Arc::new(AtomicI32::new(screen_height / 2));
        let thread_x = Arc::clone(&x);
        let thread_y = Arc::clone(&y);

        std::thread::spawn(move || {
            log::info!(
                "evdev tracker started: {} (abs_x: {:?}, abs_y: {:?})",
                device.name().unwrap_or("unknown"),
                abs_x_range,
                abs_y_range,
            );
            run_event_loop(
                &mut device,
                &thread_x,
                &thread_y,
                screen_width,
                screen_height,
                abs_x_range,
                abs_y_range,
            );
        });

        Some(Self { x, y })
    }
}

impl CursorTracker for EvdevCursorTracker {
    fn get_position(&self) -> (f32, f32) {
        (
            self.x.load(Ordering::Relaxed) as f32,
            self.y.load(Ordering::Relaxed) as f32,
        )
    }
}

fn run_event_loop(
    device: &mut Device,
    x: &AtomicI32,
    y: &AtomicI32,
    screen_width: i32,
    screen_height: i32,
    abs_x_range: Option<(i32, i32)>,
    abs_y_range: Option<(i32, i32)>,
) {
    loop {
        match device.fetch_events() {
            Ok(events) => {
                for event in events {
                    match event.kind() {
                        InputEventKind::RelAxis(axis) => {
                            let delta = event.value();
                            match axis {
                                RelativeAxisType::REL_X => {
                                    let old = x.load(Ordering::Relaxed);
                                    x.store(
                                        (old + delta).clamp(0, screen_width - 1),
                                        Ordering::Relaxed,
                                    );
                                }
                                RelativeAxisType::REL_Y => {
                                    let old = y.load(Ordering::Relaxed);
                                    y.store(
                                        (old + delta).clamp(0, screen_height - 1),
                                        Ordering::Relaxed,
                                    );
                                }
                                _ => {}
                            }
                        }
                        InputEventKind::AbsAxis(axis) => {
                            let val = event.value() as f64;
                            match axis {
                                AbsoluteAxisType::ABS_X => {
                                    let screen_x =
                                        map_abs_to_screen(val, abs_x_range, screen_width);
                                    x.store(screen_x, Ordering::Relaxed);
                                }
                                AbsoluteAxisType::ABS_Y => {
                                    let screen_y =
                                        map_abs_to_screen(val, abs_y_range, screen_height);
                                    y.store(screen_y, Ordering::Relaxed);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                log::error!("evdev read error: {}", e);
                break;
            }
        }
    }
}

/// Maps an absolute axis value to screen pixel coordinate.
fn map_abs_to_screen(val: f64, range: Option<(i32, i32)>, screen_size: i32) -> i32 {
    if let Some((min, max)) = range {
        let r = (max - min).max(1) as f64;
        ((val - min as f64) / r * screen_size as f64) as i32
    } else {
        val as i32
    }
    .clamp(0, screen_size - 1)
}

/// Find the best pointing device in /dev/input/.
/// Prefers absolute devices (tablets, VMs) over relative devices (mice).
fn find_pointing_device() -> Option<Device> {
    let devices: Vec<_> = evdev::enumerate().collect();

    // First: absolute axis devices (tablets, VMs, touchscreens)
    for (path, device) in &devices {
        if let Some(axes) = device.supported_absolute_axes() {
            if axes.contains(AbsoluteAxisType::ABS_X) && axes.contains(AbsoluteAxisType::ABS_Y) {
                let name = device.name().unwrap_or("").to_lowercase();
                if name.contains("joystick") || name.contains("gamepad") {
                    continue;
                }
                log::info!(
                    "Found absolute pointing device: {} ({:?})",
                    device.name().unwrap_or("unknown"),
                    path
                );
                // Re-enumerate to get an owned Device (can't move from reference)
                for (p2, d2) in evdev::enumerate() {
                    if p2 == *path {
                        return Some(d2);
                    }
                }
            }
        }
    }

    // Second: relative axis devices (regular mice)
    for (path, device) in &devices {
        if let Some(axes) = device.supported_relative_axes() {
            if axes.contains(RelativeAxisType::REL_X) && axes.contains(RelativeAxisType::REL_Y) {
                log::info!(
                    "Found relative mouse device: {} ({:?})",
                    device.name().unwrap_or("unknown"),
                    path
                );
                for (p2, d2) in evdev::enumerate() {
                    if p2 == *path {
                        return Some(d2);
                    }
                }
            }
        }
    }

    log::warn!("No pointing device found in /dev/input/. Is user in 'input' group?");
    None
}

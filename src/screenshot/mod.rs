/// Multi-monitor screenshot capture using xcap.
/// Cross-platform: works on Linux (X11/Wayland), Windows, and macOS.

use crate::api::claude::ScreenshotForClaude;
use crate::core::coordinate_mapper::DisplayInfo;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use std::fmt;
use std::io::Cursor;

const MAX_WIDTH: u32 = 1280;
const JPEG_QUALITY: u8 = 80;

pub struct CaptureResult {
    pub screenshots: Vec<ScreenshotForClaude>,
    pub display_infos: Vec<DisplayInfo>,
}

#[derive(Debug)]
pub enum ScreenshotError {
    NoMonitors,
    CaptureError(String),
}

impl fmt::Display for ScreenshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScreenshotError::NoMonitors => write!(f, "No monitors found"),
            ScreenshotError::CaptureError(msg) => write!(f, "Capture error: {}", msg),
        }
    }
}

/// Captures all monitors, scales to max 1280px width, encodes as JPEG.
/// Labels each screenshot with cursor/secondary designation.
///
/// On Wayland where xcap's default capture fails (e.g. GNOME without wlr-screencopy),
/// falls back to X11 capture via XWayland by temporarily unsetting WAYLAND_DISPLAY.
pub fn capture_all_screens(cursor_x: f32, cursor_y: f32) -> Result<CaptureResult, ScreenshotError> {
    let monitors = xcap::Monitor::all()
        .map_err(|e| ScreenshotError::CaptureError(e.to_string()))?;

    if monitors.is_empty() {
        return Err(ScreenshotError::NoMonitors);
    }

    let total = monitors.len();
    let mut screenshots = Vec::with_capacity(total);
    let mut display_infos = Vec::with_capacity(total);

    for (i, monitor) in monitors.iter().enumerate() {
        let screen_num = (i + 1) as u32;

        // Check if cursor is on this monitor
        let mon_x = monitor.x().unwrap_or(0) as f32;
        let mon_y = monitor.y().unwrap_or(0) as f32;
        let mon_w = monitor.width().unwrap_or(1920) as f32;
        let mon_h = monitor.height().unwrap_or(1080) as f32;
        let is_cursor_display = cursor_x >= mon_x
            && cursor_x < mon_x + mon_w
            && cursor_y >= mon_y
            && cursor_y < mon_y + mon_h;

        // Capture — try normally first, fall back to X11 if Wayland capture fails
        let img = match monitor.capture_image() {
            Ok(img) => img,
            Err(e) => {
                log::debug!("Wayland capture failed ({}), trying X11 fallback", e);
                // Temporarily pretend we're on X11 so xcap uses the xorg path
                let saved = std::env::var("XDG_SESSION_TYPE").ok();
                std::env::set_var("XDG_SESSION_TYPE", "x11");
                let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
                if wayland_display.is_some() {
                    std::env::remove_var("WAYLAND_DISPLAY");
                }

                // Re-enumerate monitors with X11 and capture
                let x11_result = xcap::Monitor::all()
                    .and_then(|mons| {
                        mons.get(i)
                            .ok_or_else(|| xcap::XCapError::new(&format!("No X11 monitor {}", i)))
                            .and_then(|m| m.capture_image())
                    });

                // Restore env
                if let Some(val) = saved {
                    std::env::set_var("XDG_SESSION_TYPE", val);
                }
                if let Some(val) = wayland_display {
                    std::env::set_var("WAYLAND_DISPLAY", val);
                }

                x11_result.map_err(|e2| ScreenshotError::CaptureError(
                    format!("Monitor {}: Wayland and X11 capture both failed: {}", screen_num, e2)
                ))?
            }
        };

        // Scale if wider than MAX_WIDTH
        let (orig_w, orig_h) = (img.width(), img.height());
        let scaled = if orig_w > MAX_WIDTH {
            let new_h = (orig_h as f64 * MAX_WIDTH as f64 / orig_w as f64) as u32;
            image::imageops::resize(&img, MAX_WIDTH, new_h, FilterType::Lanczos3)
        } else {
            img
        };

        let (scaled_w, scaled_h) = (scaled.width(), scaled.height());

        // Encode to JPEG
        let mut jpeg_buf = Vec::new();
        {
            let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut jpeg_buf), JPEG_QUALITY);
            encoder.encode_image(&scaled)
                .map_err(|e| ScreenshotError::CaptureError(format!("JPEG encode: {}", e)))?;
        }

        // Build label
        let label = if total == 1 {
            format!("screen 1 of 1 ({}x{} pixels)", scaled_w, scaled_h)
        } else if is_cursor_display {
            format!(
                "screen {} of {} ({}x{} pixels) — cursor is on this screen (primary focus)",
                screen_num, total, scaled_w, scaled_h
            )
        } else {
            format!(
                "screen {} of {} ({}x{} pixels) — secondary screen",
                screen_num, total, scaled_w, scaled_h
            )
        };

        screenshots.push(ScreenshotForClaude {
            jpeg_data: jpeg_buf,
            label,
        });

        display_infos.push(DisplayInfo {
            screen_number: screen_num,
            global_origin_x: mon_x as f64,
            global_origin_y: mon_y as f64,
            display_width_points: mon_w as f64,
            display_height_points: mon_h as f64,
            screenshot_width_pixels: scaled_w,
            screenshot_height_pixels: scaled_h,
            is_cursor_display,
        });
    }

    log::info!("Screenshot captured: {} screen(s), cursor on screen {}",
        total,
        display_infos.iter().find(|d| d.is_cursor_display).map_or(0, |d| d.screen_number)
    );

    Ok(CaptureResult { screenshots, display_infos })
}

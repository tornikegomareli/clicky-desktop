/// Multi-monitor screenshot capture.
///
/// Platform/compositor strategy (all silent, no flash):
///   Hyprland/Sway  → grim (wlroots tool, outputs PNG to stdout)
///   GNOME Wayland  → xcap portal with animations temporarily disabled (no flash)
///   X11            → xcap (XGetImage)
///   Fallback       → xcap default

use crate::api::claude::ScreenshotForClaude;
use crate::core::coordinate_mapper::DisplayInfo;
use crate::app::platform::{PlatformInfo, DisplayServer, WaylandCompositor};
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

/// Captures all monitors using the best method for the current platform.
pub fn capture_all_screens(
    cursor_x: f32,
    cursor_y: f32,
    platform: &PlatformInfo,
) -> Result<CaptureResult, ScreenshotError> {
    if platform.display_server == Some(DisplayServer::Wayland) {
        match &platform.wayland_compositor {
            // Hyprland/Sway: use grim (wlroots, silent)
            Some(WaylandCompositor::Hyprland | WaylandCompositor::Sway) => {
                if let Ok(result) = capture_with_grim(cursor_x, cursor_y) {
                    return Ok(result);
                }
                log::debug!("grim not available, falling back to xcap");
            }

            // GNOME: disable animations around xcap to prevent flash
            _ => {
                return capture_gnome_no_flash(cursor_x, cursor_y);
            }
        }
    }

    // Fallback: xcap (X11, Windows, macOS)
    capture_with_xcap(cursor_x, cursor_y)
}

/// GNOME-specific: briefly disable animations to prevent the screenshot flash.
/// The portal screenshot flash is an animation effect — disabling animations
/// via gsettings suppresses it entirely.
fn capture_gnome_no_flash(cursor_x: f32, cursor_y: f32) -> Result<CaptureResult, ScreenshotError> {
    // Check current animation state
    let was_enabled = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "true")
        .unwrap_or(true);

    // Disable animations before capture
    if was_enabled {
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.desktop.interface", "enable-animations", "false"])
            .output();
    }

    // Capture
    let result = capture_with_xcap(cursor_x, cursor_y);

    // Restore animations
    if was_enabled {
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.desktop.interface", "enable-animations", "true"])
            .output();
    }

    result
}

/// Capture using grim (wlroots compositors). Silent, no flash.
fn capture_with_grim(cursor_x: f32, cursor_y: f32) -> Result<CaptureResult, ScreenshotError> {
    let output = std::process::Command::new("grim")
        .arg("-t").arg("png")
        .arg("-")
        .output()
        .map_err(|e| ScreenshotError::CaptureError(format!("grim not found: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ScreenshotError::CaptureError(format!("grim failed: {}", stderr)));
    }

    let img = image::load_from_memory_with_format(&output.stdout, image::ImageFormat::Png)
        .map_err(|e| ScreenshotError::CaptureError(format!("Failed to decode grim output: {}", e)))?
        .to_rgba8();

    let (orig_w, orig_h) = (img.width(), img.height());
    log::info!("grim captured: {}x{}", orig_w, orig_h);

    let scaled = scale_image(&img);
    let (sw, sh) = (scaled.width(), scaled.height());
    let jpeg_data = encode_jpeg(&scaled)?;

    Ok(CaptureResult {
        screenshots: vec![ScreenshotForClaude {
            jpeg_data,
            label: format!("screen 1 of 1 ({}x{} pixels)", sw, sh),
        }],
        display_infos: vec![DisplayInfo {
            screen_number: 1,
            global_origin_x: 0.0,
            global_origin_y: 0.0,
            display_width_points: orig_w as f64,
            display_height_points: orig_h as f64,
            screenshot_width_pixels: sw,
            screenshot_height_pixels: sh,
            is_cursor_display: true,
        }],
    })
}

/// Capture using xcap (cross-platform).
fn capture_with_xcap(cursor_x: f32, cursor_y: f32) -> Result<CaptureResult, ScreenshotError> {
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

        let mon_x = monitor.x().unwrap_or(0) as f32;
        let mon_y = monitor.y().unwrap_or(0) as f32;
        let mon_w = monitor.width().unwrap_or(1920) as f32;
        let mon_h = monitor.height().unwrap_or(1080) as f32;
        let is_cursor_display = cursor_x >= mon_x
            && cursor_x < mon_x + mon_w
            && cursor_y >= mon_y
            && cursor_y < mon_y + mon_h;

        let img = monitor
            .capture_image()
            .map_err(|e| ScreenshotError::CaptureError(format!("Monitor {}: {}", screen_num, e)))?;

        let scaled = scale_image(&img);
        let (sw, sh) = (scaled.width(), scaled.height());
        let jpeg_data = encode_jpeg(&scaled)?;

        let label = if total == 1 {
            format!("screen 1 of 1 ({}x{} pixels)", sw, sh)
        } else if is_cursor_display {
            format!(
                "screen {} of {} ({}x{} pixels) — cursor is on this screen (primary focus)",
                screen_num, total, sw, sh
            )
        } else {
            format!(
                "screen {} of {} ({}x{} pixels) — secondary screen",
                screen_num, total, sw, sh
            )
        };

        screenshots.push(ScreenshotForClaude { jpeg_data, label });
        display_infos.push(DisplayInfo {
            screen_number: screen_num,
            global_origin_x: mon_x as f64,
            global_origin_y: mon_y as f64,
            display_width_points: mon_w as f64,
            display_height_points: mon_h as f64,
            screenshot_width_pixels: sw,
            screenshot_height_pixels: sh,
            is_cursor_display,
        });
    }

    log::info!("Screenshot captured: {} screen(s), cursor on screen {}",
        total,
        display_infos.iter().find(|d| d.is_cursor_display).map_or(0, |d| d.screen_number)
    );

    Ok(CaptureResult { screenshots, display_infos })
}

fn scale_image(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w > MAX_WIDTH {
        let new_h = (h as f64 * MAX_WIDTH as f64 / w as f64) as u32;
        image::imageops::resize(img, MAX_WIDTH, new_h, FilterType::Lanczos3)
    } else {
        img.clone()
    }
}

fn encode_jpeg(img: &image::RgbaImage) -> Result<Vec<u8>, ScreenshotError> {
    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut buf), JPEG_QUALITY);
    encoder.encode_image(img)
        .map_err(|e| ScreenshotError::CaptureError(format!("JPEG encode: {}", e)))?;
    Ok(buf)
}

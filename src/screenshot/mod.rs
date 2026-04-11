/// Multi-monitor screenshot capture.
///
/// Platform/compositor strategy (all silent, no flash):
///   Hyprland/Sway  → grim (wlroots tool, outputs PNG to stdout)
///   GNOME Wayland  → xcap portal with animations temporarily disabled (no flash)
///   X11            → xcap (XGetImage)
///   Fallback       → xcap default
use crate::api::claude::ScreenshotForClaude;
use crate::app::platform::{DisplayServer, PlatformInfo, WaylandCompositor};
use crate::core::coordinate_mapper::DisplayInfo;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use std::fmt;
use std::io::Cursor;

const MAX_WIDTH: u32 = 1024;
const JPEG_QUALITY: u8 = 80;

#[cfg(target_os = "windows")]
fn get_windows_virtual_desktop_bounds() -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    let origin_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let origin_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    if width > 0 && height > 0 {
        Some((origin_x, origin_y, width, height))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn get_windows_virtual_desktop_bounds() -> Option<(i32, i32, i32, i32)> {
    None
}

#[derive(Clone)]
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
    capture_screens(cursor_x, cursor_y, platform, false)
}

pub fn capture_primary_focus_screen(
    cursor_x: f32,
    cursor_y: f32,
    platform: &PlatformInfo,
) -> Result<CaptureResult, ScreenshotError> {
    capture_screens(cursor_x, cursor_y, platform, true)
}

fn capture_screens(
    cursor_x: f32,
    cursor_y: f32,
    platform: &PlatformInfo,
    cursor_only: bool,
) -> Result<CaptureResult, ScreenshotError> {
    if platform.display_server == Some(DisplayServer::Wayland) {
        match &platform.wayland_compositor {
            // Hyprland/Sway: use grim (wlroots, silent)
            Some(WaylandCompositor::Hyprland | WaylandCompositor::Sway) => {
                if let Ok(result) = capture_with_grim(cursor_x, cursor_y, cursor_only) {
                    return Ok(result);
                }
                log::debug!("grim not available, falling back to xcap");
            }

            // GNOME: disable animations around xcap to prevent flash
            _ => {
                return capture_gnome_no_flash(cursor_x, cursor_y, cursor_only);
            }
        }
    }

    // Fallback: xcap (X11, Windows, macOS)
    capture_with_xcap(cursor_x, cursor_y, cursor_only)
}

/// GNOME-specific: briefly disable animations to prevent the screenshot flash.
/// The portal screenshot flash is an animation effect — disabling animations
/// via gsettings suppresses it entirely.
fn capture_gnome_no_flash(
    cursor_x: f32,
    cursor_y: f32,
    cursor_only: bool,
) -> Result<CaptureResult, ScreenshotError> {
    // Save current state
    let animations_on = gsettings_get_bool("org.gnome.desktop.interface", "enable-animations");
    let sounds_on = gsettings_get_bool("org.gnome.desktop.sound", "event-sounds");

    // Suppress flash (animation) and shutter sound before capture
    if animations_on {
        gsettings_set("org.gnome.desktop.interface", "enable-animations", "false");
    }
    if sounds_on {
        gsettings_set("org.gnome.desktop.sound", "event-sounds", "false");
    }

    let result = capture_with_xcap(cursor_x, cursor_y, cursor_only);

    // Restore
    if animations_on {
        gsettings_set("org.gnome.desktop.interface", "enable-animations", "true");
    }
    if sounds_on {
        gsettings_set("org.gnome.desktop.sound", "event-sounds", "true");
    }

    result
}

fn gsettings_get_bool(schema: &str, key: &str) -> bool {
    std::process::Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "true")
        .unwrap_or(true)
}

fn gsettings_set(schema: &str, key: &str, value: &str) {
    let _ = std::process::Command::new("gsettings")
        .args(["set", schema, key, value])
        .output();
}

/// Monitor geometry from hyprctl/swaymsg, used for accurate coordinate mapping.
#[derive(Debug, Clone)]
struct WlrMonitorInfo {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Queries monitor geometry from Hyprland via `hyprctl monitors -j`.
/// Returns logical (scaled) dimensions — the coordinate space the compositor uses.
fn query_wlr_monitors() -> Option<Vec<WlrMonitorInfo>> {
    let output = std::process::Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .ok()?;

    if !output.status.success() {
        // Try swaymsg as fallback for Sway
        let output = std::process::Command::new("swaymsg")
            .args(["-t", "get_outputs", "--raw"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        return parse_sway_monitors(&output.stdout);
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let monitors = json.as_array()?;

    let mut result = Vec::new();
    for monitor in monitors {
        let name = monitor["name"].as_str().unwrap_or("").to_string();
        let x = monitor["x"].as_f64().unwrap_or(0.0);
        let y = monitor["y"].as_f64().unwrap_or(0.0);
        let width = monitor["width"].as_f64().unwrap_or(1920.0);
        let height = monitor["height"].as_f64().unwrap_or(1080.0);
        let scale = monitor["scale"].as_f64().unwrap_or(1.0);

        // Hyprland reports physical pixels for width/height. The logical
        // (compositor) size used for window positioning is physical / scale.
        let logical_width = width / scale;
        let logical_height = height / scale;

        result.push(WlrMonitorInfo {
            name,
            x,
            y,
            width: logical_width,
            height: logical_height,
        });
    }

    Some(result)
}

/// Parses swaymsg get_outputs JSON.
fn parse_sway_monitors(stdout: &[u8]) -> Option<Vec<WlrMonitorInfo>> {
    let json: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    let outputs = json.as_array()?;

    let mut result = Vec::new();
    for output in outputs {
        if !output["active"].as_bool().unwrap_or(false) {
            continue;
        }
        let name = output["name"].as_str().unwrap_or("").to_string();
        let rect = &output["rect"];
        let x = rect["x"].as_f64().unwrap_or(0.0);
        let y = rect["y"].as_f64().unwrap_or(0.0);
        let width = rect["width"].as_f64().unwrap_or(1920.0);
        let height = rect["height"].as_f64().unwrap_or(1080.0);
        let scale = output["scale"].as_f64().unwrap_or(1.0);

        result.push(WlrMonitorInfo {
            name,
            x,
            y,
            width: width / scale,
            height: height / scale,
        });
    }

    Some(result)
}

/// Capture each monitor individually using grim -o <output_name>.
/// Uses hyprctl/swaymsg to get accurate logical monitor dimensions
/// so coordinate mapping works correctly.
fn capture_with_grim(
    cursor_x: f32,
    cursor_y: f32,
    cursor_only: bool,
) -> Result<CaptureResult, ScreenshotError> {
    let monitors = query_wlr_monitors().ok_or_else(|| {
        ScreenshotError::CaptureError("Cannot query monitors via hyprctl/swaymsg".into())
    })?;

    let filtered_monitors: Vec<&WlrMonitorInfo> = monitors
        .iter()
        .filter(|monitor| {
            let is_cursor_display = cursor_x as f64 >= monitor.x
                && (cursor_x as f64) < monitor.x + monitor.width
                && cursor_y as f64 >= monitor.y
                && (cursor_y as f64) < monitor.y + monitor.height;
            !cursor_only || is_cursor_display
        })
        .collect();

    if filtered_monitors.is_empty() {
        return Err(ScreenshotError::NoMonitors);
    }

    let total = filtered_monitors.len();
    let mut screenshots = Vec::with_capacity(total);
    let mut display_infos = Vec::with_capacity(total);

    for (i, monitor) in filtered_monitors.iter().enumerate() {
        let screen_num = (i + 1) as u32;

        // Capture this specific output
        let output = std::process::Command::new("grim")
            .arg("-o")
            .arg(&monitor.name)
            .arg("-t")
            .arg("png")
            .arg("-")
            .output()
            .map_err(|e| ScreenshotError::CaptureError(format!("grim not found: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ScreenshotError::CaptureError(format!(
                "grim failed for output {}: {}",
                monitor.name, stderr
            )));
        }

        let img = image::load_from_memory_with_format(&output.stdout, image::ImageFormat::Png)
            .map_err(|e| {
                ScreenshotError::CaptureError(format!("Failed to decode grim output: {}", e))
            })?
            .to_rgba8();

        log::info!(
            "grim captured {}: {}x{} pixels (logical {}x{})",
            monitor.name,
            img.width(),
            img.height(),
            monitor.width,
            monitor.height
        );

        let mut scaled = scale_image(&img);
        draw_coordinate_grid(&mut scaled);
        let (sw, sh) = (scaled.width(), scaled.height());
        let jpeg_data = encode_jpeg(&scaled)?;

        let is_cursor_display = cursor_x as f64 >= monitor.x
            && (cursor_x as f64) < monitor.x + monitor.width
            && cursor_y as f64 >= monitor.y
            && (cursor_y as f64) < monitor.y + monitor.height;

        let label = if total == 1 {
            format!("screen 1 of 1 (image dimensions: {}x{} pixels)", sw, sh)
        } else if is_cursor_display {
            format!(
                "screen {} of {} (image dimensions: {}x{} pixels) — cursor is on this screen (primary focus)",
                screen_num, total, sw, sh
            )
        } else {
            format!(
                "screen {} of {} (image dimensions: {}x{} pixels) — secondary screen",
                screen_num, total, sw, sh
            )
        };

        screenshots.push(ScreenshotForClaude { jpeg_data, label });
        // display_width/height_points = LOGICAL size (what the compositor uses
        // for window positioning). This is what the overlay coordinate space uses.
        // screenshot_width/height_pixels = the JPEG dimensions sent to Claude.
        // The coordinate mapper scales from screenshot pixels → logical points.
        display_infos.push(DisplayInfo {
            screen_number: screen_num,
            global_origin_x: monitor.x,
            global_origin_y: monitor.y,
            display_width_points: monitor.width,
            display_height_points: monitor.height,
            screenshot_width_pixels: sw,
            screenshot_height_pixels: sh,
            is_cursor_display,
        });
    }

    log::info!(
        "Screenshot captured: {} screen(s), cursor on screen {}",
        total,
        display_infos
            .iter()
            .find(|d| d.is_cursor_display)
            .map_or(0, |d| d.screen_number)
    );

    Ok(CaptureResult {
        screenshots,
        display_infos,
    })
}

/// Queries the primary monitor's dimensions for overlay window sizing.
/// Falls back to 1920x1080 if detection fails.
pub fn detect_screen_size(platform: &PlatformInfo) -> (i32, i32) {
    let (_, _, width, height) = detect_overlay_bounds(platform);
    (width, height)
}

/// Returns the overlay origin and size.
/// On Windows this covers the full virtual desktop so the overlay can span
/// every monitor, including layouts with negative monitor coordinates.
pub fn detect_overlay_bounds(platform: &PlatformInfo) -> (i32, i32, i32, i32) {
    #[cfg(target_os = "windows")]
    if matches!(platform.os, crate::app::platform::OperatingSystem::Windows) {
        if let Some((origin_x, origin_y, width, height)) = get_windows_virtual_desktop_bounds() {
            log::info!(
                "Detected Windows virtual desktop: origin=({}, {}) size={}x{}",
                origin_x,
                origin_y,
                width,
                height
            );
            return (origin_x, origin_y, width, height);
        }
    }

    // Try wlroots monitor query first (Hyprland/Sway)
    if platform.display_server == Some(DisplayServer::Wayland) {
        if let Some(monitors) = query_wlr_monitors() {
            if let Some(primary) = monitors.first() {
                let w = primary.width as i32;
                let h = primary.height as i32;
                log::info!("Detected screen size: {}x{} (from {})", w, h, primary.name);
                return (0, 0, w, h);
            }
        }
    }

    // Try xcap for X11/other
    if let Ok(monitors) = xcap::Monitor::all() {
        if let Some(m) = monitors.first() {
            let w = m.width().unwrap_or(1920) as i32;
            let h = m.height().unwrap_or(1080) as i32;
            log::info!("Detected screen size: {}x{} (from xcap)", w, h);
            return (0, 0, w, h);
        }
    }

    log::warn!("Could not detect screen size, defaulting to 1920x1080");
    (0, 0, 1920, 1080)
}

/// Capture using xcap (cross-platform).
/// On Windows after GLFW init, the process is per-monitor DPI aware, so
/// xcap, GetCursorPos, and the overlay window all use physical pixels.
fn capture_with_xcap(
    cursor_x: f32,
    cursor_y: f32,
    cursor_only: bool,
) -> Result<CaptureResult, ScreenshotError> {
    let monitors =
        xcap::Monitor::all().map_err(|e| ScreenshotError::CaptureError(e.to_string()))?;

    if monitors.is_empty() {
        return Err(ScreenshotError::NoMonitors);
    }

    let filtered_monitors: Vec<&xcap::Monitor> = monitors
        .iter()
        .filter(|monitor| {
            let mon_x = monitor.x().unwrap_or(0) as f32;
            let mon_y = monitor.y().unwrap_or(0) as f32;
            let mon_w = monitor.width().unwrap_or(1920) as f32;
            let mon_h = monitor.height().unwrap_or(1080) as f32;
            let is_cursor_display = cursor_x >= mon_x
                && cursor_x < mon_x + mon_w
                && cursor_y >= mon_y
                && cursor_y < mon_y + mon_h;
            !cursor_only || is_cursor_display
        })
        .collect();

    if filtered_monitors.is_empty() {
        return Err(ScreenshotError::NoMonitors);
    }

    let total = filtered_monitors.len();
    let mut screenshots = Vec::with_capacity(total);
    let mut display_infos = Vec::with_capacity(total);

    for (i, monitor) in filtered_monitors.iter().enumerate() {
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

        let mut scaled = scale_image(&img);
        draw_coordinate_grid(&mut scaled);
        let (sw, sh) = (scaled.width(), scaled.height());
        let jpeg_data = encode_jpeg(&scaled)?;

        let label = if total == 1 {
            format!("screen 1 of 1 (image dimensions: {}x{} pixels)", sw, sh)
        } else if is_cursor_display {
            format!(
                "screen {} of {} (image dimensions: {}x{} pixels) — cursor is on this screen (primary focus)",
                screen_num, total, sw, sh
            )
        } else {
            format!(
                "screen {} of {} (image dimensions: {}x{} pixels) — secondary screen",
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

    log::info!(
        "Screenshot captured: {} screen(s), cursor on screen {}",
        total,
        display_infos
            .iter()
            .find(|d| d.is_cursor_display)
            .map_or(0, |d| d.screen_number)
    );

    Ok(CaptureResult {
        screenshots,
        display_infos,
    })
}

fn scale_image(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w > MAX_WIDTH {
        let new_h = (h as f64 * MAX_WIDTH as f64 / w as f64) as u32;
        image::imageops::resize(img, MAX_WIDTH, new_h, FilterType::Triangle)
    } else {
        img.clone()
    }
}

/// Draws a subtle coordinate grid on the screenshot to help the LLM
/// estimate coordinates more precisely. Lines every 200px with labels.
fn draw_coordinate_grid(img: &mut image::RgbaImage) {
    let grid_enabled = std::env::var("CLICKY_GRID_OVERLAY")
        .map(|v| v != "0")
        .unwrap_or(false);
    if !grid_enabled {
        return;
    }

    let (w, h) = (img.width(), img.height());
    let line_color = image::Rgba([255, 0, 0, 50]); // semi-transparent red
    let label_color = image::Rgba([255, 0, 0, 160]); // more opaque for text

    // Draw grid lines every 200px
    for x in (200..w).step_by(200) {
        for y in 0..h {
            blend_pixel(img, x, y, line_color);
        }
    }
    for y in (200..h).step_by(200) {
        for x in 0..w {
            blend_pixel(img, x, y, line_color);
        }
    }

    // Label intersections with coordinates
    for gx in (200..w).step_by(200) {
        for gy in (200..h).step_by(200) {
            let label = format!("{},{}", gx, gy);
            draw_tiny_text(img, gx + 2, gy + 2, &label, label_color);
        }
    }
    // Label edges (x axis at top, y axis at left)
    for gx in (200..w).step_by(200) {
        draw_tiny_text(img, gx + 2, 2, &format!("{}", gx), label_color);
    }
    for gy in (200..h).step_by(200) {
        draw_tiny_text(img, 2, gy + 2, &format!("{}", gy), label_color);
    }
}

/// Blend a semi-transparent pixel onto the image.
fn blend_pixel(img: &mut image::RgbaImage, x: u32, y: u32, color: image::Rgba<u8>) {
    if x >= img.width() || y >= img.height() {
        return;
    }
    let bg = img.get_pixel(x, y);
    let alpha = color[3] as f32 / 255.0;
    let inv = 1.0 - alpha;
    let blended = image::Rgba([
        (color[0] as f32 * alpha + bg[0] as f32 * inv) as u8,
        (color[1] as f32 * alpha + bg[1] as f32 * inv) as u8,
        (color[2] as f32 * alpha + bg[2] as f32 * inv) as u8,
        255,
    ]);
    img.put_pixel(x, y, blended);
}

/// Minimal 3x5 bitmap font for digits and comma — no dependencies needed.
fn draw_tiny_text(img: &mut image::RgbaImage, x: u32, y: u32, text: &str, color: image::Rgba<u8>) {
    // 3x5 pixel bitmaps for 0-9 and comma (each row is 3 bits, MSB left)
    const GLYPHS: &[(char, [u8; 5])] = &[
        ('0', [0b111, 0b101, 0b101, 0b101, 0b111]),
        ('1', [0b010, 0b110, 0b010, 0b010, 0b111]),
        ('2', [0b111, 0b001, 0b111, 0b100, 0b111]),
        ('3', [0b111, 0b001, 0b111, 0b001, 0b111]),
        ('4', [0b101, 0b101, 0b111, 0b001, 0b001]),
        ('5', [0b111, 0b100, 0b111, 0b001, 0b111]),
        ('6', [0b111, 0b100, 0b111, 0b101, 0b111]),
        ('7', [0b111, 0b001, 0b001, 0b001, 0b001]),
        ('8', [0b111, 0b101, 0b111, 0b101, 0b111]),
        ('9', [0b111, 0b101, 0b111, 0b001, 0b111]),
        (',', [0b000, 0b000, 0b000, 0b010, 0b100]),
    ];

    let mut cx = x;
    for ch in text.chars() {
        if let Some((_, bitmap)) = GLYPHS.iter().find(|(c, _)| *c == ch) {
            for (row, bits) in bitmap.iter().enumerate() {
                for col in 0..3u32 {
                    if bits & (0b100 >> col) != 0 {
                        let px = cx + col;
                        let py = y + row as u32;
                        if px < img.width() && py < img.height() {
                            img.put_pixel(px, py, color);
                        }
                    }
                }
            }
        }
        cx += 4; // 3px glyph + 1px gap
    }
}

fn encode_jpeg(img: &image::RgbaImage) -> Result<Vec<u8>, ScreenshotError> {
    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut buf), JPEG_QUALITY);
    encoder
        .encode_image(img)
        .map_err(|e| ScreenshotError::CaptureError(format!("JPEG encode: {}", e)))?;
    Ok(buf)
}

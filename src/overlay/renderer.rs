/// Raylib overlay renderer — draws the blue cursor triangle, speech bubbles,
/// waveform visualization, loading animation, and bezier flight animations.
///
/// This is the visual heart of Clicky. It runs at 60fps on a transparent
/// borderless window that overlays the entire virtual desktop.

use raylib::prelude::*;
use crate::app::state_machine::VoiceState;
use crate::core::{bezier_flight, design_system};

/// Navigation mode for the blue cursor triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorNavigationMode {
    /// Cursor follows the real mouse with spring animation
    FollowingMouse,

    /// Cursor is flying to a target element via bezier arc
    NavigatingToTarget,

    /// Cursor has arrived at target, showing speech bubble
    PointingAtTarget,

    /// Cursor is flying back to the mouse position
    ReturningToMouse,
}

/// State for an active bezier flight animation.
pub struct ActiveFlightAnimation {
    pub start_x: f64,
    pub start_y: f64,
    pub control_x: f64,
    pub control_y: f64,
    pub end_x: f64,
    pub end_y: f64,
    pub duration_seconds: f64,
    pub elapsed_seconds: f64,
    pub is_return: bool,
}

impl ActiveFlightAnimation {
    pub fn new(start_x: f64, start_y: f64, end_x: f64, end_y: f64, is_return: bool) -> Self {
        let (control_x, control_y) =
            bezier_flight::compute_control_point(start_x, start_y, end_x, end_y, is_return);
        let duration_seconds =
            bezier_flight::compute_flight_duration_seconds(start_x, start_y, end_x, end_y, is_return);

        Self {
            start_x, start_y,
            control_x, control_y,
            end_x, end_y,
            duration_seconds,
            elapsed_seconds: 0.0,
            is_return,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.elapsed_seconds >= self.duration_seconds
    }

    pub fn linear_progress(&self) -> f64 {
        (self.elapsed_seconds / self.duration_seconds).clamp(0.0, 1.0)
    }
}

/// All mutable state needed to render the overlay each frame.
pub struct OverlayRenderState {
    pub voice_state: VoiceState,
    pub navigation_mode: CursorNavigationMode,
    pub cursor_x: f32,
    pub cursor_y: f32,
    pub cursor_rotation_degrees: f64,
    pub cursor_scale: f64,
    pub active_flight: Option<ActiveFlightAnimation>,
    pub audio_power_level: f64,
    pub audio_power_history: Vec<f64>,
    pub speech_bubble_text: String,
    pub speech_bubble_visible_char_count: usize,
    pub speech_bubble_opacity: f32,
    pub bubble_font: Option<Font>,
}

impl OverlayRenderState {
    pub fn new() -> Self {
        Self {
            voice_state: VoiceState::Idle,
            navigation_mode: CursorNavigationMode::FollowingMouse,
            cursor_x: 0.0,
            cursor_y: 0.0,
            cursor_rotation_degrees: design_system::cursor::DEFAULT_ROTATION_DEGREES,
            cursor_scale: 1.0,
            active_flight: None,
            audio_power_level: 0.0,
            audio_power_history: vec![0.02; 44],
            speech_bubble_text: String::new(),
            speech_bubble_visible_char_count: 0,
            speech_bubble_opacity: 0.0,
            bubble_font: None,
        }
    }

    /// Advances the flight animation by one frame's worth of time.
    pub fn advance_flight_animation(&mut self, delta_seconds: f64) {
        if let Some(flight) = &mut self.active_flight {
            flight.elapsed_seconds += delta_seconds;
            let progress = flight.linear_progress();
            let is_return = flight.is_return;

            let frame = bezier_flight::compute_flight_frame(
                progress,
                flight.start_x, flight.start_y,
                flight.control_x, flight.control_y,
                flight.end_x, flight.end_y,
                is_return,
            );

            self.cursor_x = frame.x as f32;
            self.cursor_y = frame.y as f32;
            self.cursor_rotation_degrees = frame.rotation_radians.to_degrees();
            self.cursor_scale = frame.scale;

            if flight.is_complete() {
                if is_return {
                    self.navigation_mode = CursorNavigationMode::FollowingMouse;
                    self.speech_bubble_opacity = 0.0;
                    self.speech_bubble_visible_char_count = 0;
                } else {
                    self.navigation_mode = CursorNavigationMode::PointingAtTarget;
                }
                self.cursor_rotation_degrees = design_system::cursor::DEFAULT_ROTATION_DEGREES;
                self.cursor_scale = 1.0;
                self.active_flight = None;
            }
        }
    }

    /// Starts a bezier flight from the current cursor position to a target.
    pub fn start_flight_to(&mut self, target_x: f64, target_y: f64, bubble_text: String) {
        let offset = design_system::cursor::OFFSET_FROM_SYSTEM_CURSOR as f64;
        self.navigation_mode = CursorNavigationMode::NavigatingToTarget;
        self.active_flight = Some(ActiveFlightAnimation::new(
            self.cursor_x as f64,
            self.cursor_y as f64,
            target_x - offset,
            target_y - offset,
            false,
        ));
        self.speech_bubble_text = bubble_text;
        self.speech_bubble_visible_char_count = 0;
        self.speech_bubble_opacity = 0.0;
    }

    /// Starts a bezier return flight from current position back to mouse.
    pub fn start_return_flight(&mut self, mouse_x: f32, mouse_y: f32) {
        self.navigation_mode = CursorNavigationMode::ReturningToMouse;
        self.active_flight = Some(ActiveFlightAnimation::new(
            self.cursor_x as f64,
            self.cursor_y as f64,
            mouse_x as f64,
            mouse_y as f64,
            true,
        ));
        self.speech_bubble_opacity = 0.0;
        self.speech_bubble_visible_char_count = 0;
    }

    /// Returns the cursor to following the mouse (instant, no animation).
    pub fn return_to_following_mouse(&mut self) {
        self.navigation_mode = CursorNavigationMode::FollowingMouse;
        self.speech_bubble_opacity = 0.0;
        self.speech_bubble_visible_char_count = 0;
        self.cursor_rotation_degrees = design_system::cursor::DEFAULT_ROTATION_DEGREES;
        self.cursor_scale = 1.0;
    }
}

/// Draws one frame of the overlay. Called 60 times per second from the main loop.
pub fn draw_overlay_frame(
    draw_handle: &mut RaylibDrawHandle,
    render_state: &OverlayRenderState,
) {
    draw_handle.clear_background(Color::BLANK);

    match render_state.voice_state {
        VoiceState::Idle | VoiceState::Responding => {
            draw_cursor_triangle(draw_handle, render_state);

            if render_state.navigation_mode == CursorNavigationMode::PointingAtTarget
                && render_state.speech_bubble_opacity > 0.01
            {
                draw_speech_bubble(draw_handle, render_state);
            }
        }
        VoiceState::Listening => {
            draw_waveform(draw_handle, render_state);
        }
        VoiceState::Processing => {
            draw_loading_arc(draw_handle, render_state);
        }
    }
}

/// Draws the blue equilateral triangle cursor with triangle-shaped bloom effect.
fn draw_cursor_triangle(
    draw_handle: &mut RaylibDrawHandle,
    render_state: &OverlayRenderState,
) {
    let offset = design_system::cursor::OFFSET_FROM_SYSTEM_CURSOR;
    let center_x = render_state.cursor_x + offset;
    let center_y = render_state.cursor_y + offset;
    let base_size = design_system::cursor::TRIANGLE_SIZE * render_state.cursor_scale as f32;
    let rotation_radians = render_state.cursor_rotation_degrees.to_radians() as f32;
    let cos_r = rotation_radians.cos();
    let sin_r = rotation_radians.sin();

    let time = draw_handle.get_time();
    let breath = (time * 1.5).sin() as f32 * 0.06 + 1.0;

    let is_pointing = render_state.navigation_mode == CursorNavigationMode::PointingAtTarget;

    let blue = design_system::colors::OVERLAY_CURSOR_BLUE;
    let blue_r = (blue.0 * 255.0) as u8;
    let blue_g = (blue.1 * 255.0) as u8;
    let blue_b = (blue.2 * 255.0) as u8;

    // Bloom: scaled-up copies of the triangle with decreasing alpha
    let bloom_layers: &[(f32, u8)] = if is_pointing {
        &[(2.8, 8), (2.4, 14), (2.0, 22), (1.6, 35), (1.3, 50)]
    } else {
        &[(2.0, 5), (1.6, 10), (1.3, 18)]
    };

    for &(scale_factor, alpha) in bloom_layers {
        let bloom_size = base_size * scale_factor * breath;
        let verts = triangle_vertices(bloom_size, center_x, center_y, cos_r, sin_r);
        draw_handle.draw_triangle(
            verts[0], verts[1], verts[2],
            Color::new(blue_r, blue_g, blue_b, alpha),
        );
    }

    // Main solid triangle
    let verts = triangle_vertices(base_size, center_x, center_y, cos_r, sin_r);
    draw_handle.draw_triangle(
        verts[0], verts[1], verts[2],
        Color::new(blue_r, blue_g, blue_b, 255),
    );
}

/// Computes rotated equilateral triangle vertices for a given size and center.
fn triangle_vertices(size: f32, cx: f32, cy: f32, cos_r: f32, sin_r: f32) -> [Vector2; 3] {
    let height = size * 0.866;
    let half_base = size / 2.0;
    let raw = [
        (0.0f32, -height * 0.6),
        (-half_base, height * 0.4),
        (half_base, height * 0.4),
    ];
    [
        Vector2::new(cx + raw[0].0 * cos_r - raw[0].1 * sin_r, cy + raw[0].0 * sin_r + raw[0].1 * cos_r),
        Vector2::new(cx + raw[1].0 * cos_r - raw[1].1 * sin_r, cy + raw[1].0 * sin_r + raw[1].1 * cos_r),
        Vector2::new(cx + raw[2].0 * cos_r - raw[2].1 * sin_r, cy + raw[2].0 * sin_r + raw[2].1 * cos_r),
    ]
}

/// Draws the waveform visualization (shown during listening state).
fn draw_waveform(
    draw_handle: &mut RaylibDrawHandle,
    render_state: &OverlayRenderState,
) {
    let center_x = render_state.cursor_x;
    let center_y = render_state.cursor_y;
    let bar_color = ds_color(design_system::colors::WAVEFORM_BAR);

    let bar_count = render_state.audio_power_history.len().min(20);
    let bar_width = 3.0f32;
    let bar_gap = 2.0f32;
    let max_bar_height = 30.0f32;
    let total_width = bar_count as f32 * (bar_width + bar_gap) - bar_gap;
    let start_x = center_x - total_width / 2.0;

    let history_offset = render_state.audio_power_history.len().saturating_sub(bar_count);

    for i in 0..bar_count {
        let power_level = render_state.audio_power_history[history_offset + i];
        let bar_height = (power_level as f32 * max_bar_height).max(2.0);
        let bar_x = start_x + i as f32 * (bar_width + bar_gap);
        let bar_y = center_y - bar_height / 2.0;

        draw_handle.draw_rectangle(
            bar_x as i32,
            bar_y as i32,
            bar_width as i32,
            bar_height as i32,
            bar_color,
        );
    }
}

/// Draws a smooth rotating arc loading animation.
/// A partial blue ring sweeps around the cursor with a leading dot.
fn draw_loading_arc(
    draw_handle: &mut RaylibDrawHandle,
    render_state: &OverlayRenderState,
) {
    let center_x = render_state.cursor_x;
    let center_y = render_state.cursor_y;
    let time = draw_handle.get_time() as f32;

    let blue = design_system::colors::OVERLAY_CURSOR_BLUE;
    let blue_r = (blue.0 * 255.0) as u8;
    let blue_g = (blue.1 * 255.0) as u8;
    let blue_b = (blue.2 * 255.0) as u8;

    let radius = 14.0f32;
    let arc_sweep = 240.0f32; // degrees of arc
    let rotation_speed = 3.0f32; // radians per second
    let base_angle = time * rotation_speed;

    // Draw the arc as a series of small segments
    let segment_count = 40;
    let segment_angle = arc_sweep.to_radians() / segment_count as f32;
    let thickness = 2.5f32;

    for i in 0..segment_count {
        let t = i as f32 / segment_count as f32;
        let angle = base_angle + i as f32 * segment_angle;

        // Alpha fades from tail (transparent) to head (opaque)
        let alpha = (t * t * 200.0 + 30.0).min(230.0) as u8;

        let x1 = center_x + angle.cos() * radius;
        let y1 = center_y + angle.sin() * radius;
        let x2 = center_x + (angle + segment_angle).cos() * radius;
        let y2 = center_y + (angle + segment_angle).sin() * radius;

        draw_handle.draw_line_ex(
            Vector2::new(x1, y1),
            Vector2::new(x2, y2),
            thickness,
            Color::new(blue_r, blue_g, blue_b, alpha),
        );
    }

    // Leading dot at the head of the arc
    let head_angle = base_angle + arc_sweep.to_radians();
    let dot_x = center_x + head_angle.cos() * radius;
    let dot_y = center_y + head_angle.sin() * radius;
    draw_handle.draw_circle(dot_x as i32, dot_y as i32, 3.5, Color::new(blue_r, blue_g, blue_b, 240));

    // Subtle pulsing center dot
    let pulse = (time * 2.0).sin() * 0.5 + 0.5;
    let center_alpha = (pulse * 120.0 + 40.0) as u8;
    draw_handle.draw_circle(
        center_x as i32, center_y as i32, 2.5,
        Color::new(blue_r, blue_g, blue_b, center_alpha),
    );
}

/// Draws the speech bubble — glass-effect with blue tint and white text.
/// Uses a custom TTF font when available, falls back to Raylib default.
fn draw_speech_bubble(
    draw_handle: &mut RaylibDrawHandle,
    render_state: &OverlayRenderState,
) {
    if render_state.speech_bubble_text.is_empty() {
        return;
    }

    let visible_text: String = render_state
        .speech_bubble_text
        .chars()
        .take(render_state.speech_bubble_visible_char_count)
        .collect();

    if visible_text.is_empty() {
        return;
    }

    let offset = design_system::cursor::OFFSET_FROM_SYSTEM_CURSOR;
    let bubble_x = render_state.cursor_x + offset + 12.0;
    let bubble_y = render_state.cursor_y + offset - 12.0;
    let font_size = 18.0f32;
    let spacing = 1.0f32;
    let pad_h = 14.0f32;
    let pad_v = 10.0f32;
    let opacity = render_state.speech_bubble_opacity;

    let blue = design_system::colors::OVERLAY_CURSOR_BLUE;
    let blue_r = (blue.0 * 255.0) as u8;
    let blue_g = (blue.1 * 255.0) as u8;
    let blue_b = (blue.2 * 255.0) as u8;

    // Measure text with custom font or fallback
    let text_size = if let Some(ref font) = render_state.bubble_font {
        font.measure_text(&visible_text, font_size, spacing)
    } else {
        Vector2::new(
            draw_handle.measure_text(&visible_text, font_size as i32) as f32,
            font_size,
        )
    };

    let bubble_width = text_size.x + pad_h * 2.0;
    let bubble_height = font_size + pad_v * 2.0;
    let corner_roundness = 0.5;
    let corner_segments = 10;

    // Shadow
    let shadow_alpha = (opacity * 60.0) as u8;
    draw_handle.draw_rectangle_rounded(
        Rectangle::new(bubble_x + 2.0, bubble_y + 3.0, bubble_width + 2.0, bubble_height + 2.0),
        corner_roundness, corner_segments,
        Color::new(0, 0, 0, shadow_alpha),
    );

    // Glass background — triangle's blue color, semi-transparent
    let bg_alpha = (opacity * 160.0) as u8;
    draw_handle.draw_rectangle_rounded(
        Rectangle::new(bubble_x, bubble_y, bubble_width, bubble_height),
        corner_roundness, corner_segments,
        Color::new(blue_r / 3, blue_g / 3, blue_b, bg_alpha),
    );

    // Subtle lighter inner layer for glass depth
    let inner_alpha = (opacity * 40.0) as u8;
    draw_handle.draw_rectangle_rounded(
        Rectangle::new(bubble_x + 1.0, bubble_y + 1.0, bubble_width - 2.0, bubble_height * 0.45),
        corner_roundness, corner_segments,
        Color::new(180, 200, 255, inner_alpha),
    );

    // Border — thin blue outline
    let border_alpha = (opacity * 80.0) as u8;
    draw_handle.draw_rectangle_rounded_lines(
        Rectangle::new(bubble_x, bubble_y, bubble_width, bubble_height),
        corner_roundness, corner_segments,
        Color::new(blue_r, blue_g, blue_b, border_alpha),
    );

    // Text — white for contrast on blue glass
    let text_alpha = (opacity * 255.0) as u8;
    let text_color = Color::new(255, 255, 255, text_alpha);
    let text_pos = Vector2::new(bubble_x + pad_h, bubble_y + pad_v);

    if let Some(ref font) = render_state.bubble_font {
        draw_handle.draw_text_ex(font, &visible_text, text_pos, font_size, spacing, text_color);
    } else {
        draw_handle.draw_text(
            &visible_text,
            text_pos.x as i32,
            text_pos.y as i32,
            font_size as i32,
            text_color,
        );
    }
}

/// Converts a design system color tuple to a Raylib Color.
fn ds_color(color_tuple: (f32, f32, f32, f32)) -> Color {
    Color::new(
        (color_tuple.0 * 255.0) as u8,
        (color_tuple.1 * 255.0) as u8,
        (color_tuple.2 * 255.0) as u8,
        (color_tuple.3 * 255.0) as u8,
    )
}

/// System font paths to try for the speech bubble (in priority order).
const FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf",
    "/usr/share/fonts/google-droid-sans-fonts/DroidSans.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "/System/Library/Fonts/SFNSText.ttf",
];

/// Loads the best available system font for speech bubbles.
/// Returns None if no font could be loaded (falls back to Raylib default).
pub fn load_bubble_font(rl: &mut RaylibHandle, thread: &RaylibThread) -> Option<Font> {
    for path in FONT_PATHS {
        if std::path::Path::new(path).exists() {
            match rl.load_font_ex(thread, path, 24, None) {
                Ok(font) => {
                    log::info!("Loaded bubble font: {}", path);
                    return Some(font);
                }
                Err(e) => {
                    log::debug!("Failed to load font {}: {}", path, e);
                }
            }
        }
    }
    log::warn!("No system font found — using Raylib default");
    None
}

/// Initializes the Raylib window for the overlay.
pub fn create_overlay_window(
    window_width: i32,
    window_height: i32,
) -> (RaylibHandle, RaylibThread) {
    let (mut raylib_handle, raylib_thread) = raylib::init()
        .size(window_width, window_height)
        .title("clicky-overlay")
        .undecorated()
        .transparent()
        .build();

    unsafe {
        raylib::ffi::SetWindowState(
            raylib::ffi::ConfigFlags::FLAG_WINDOW_TOPMOST as u32
                | raylib::ffi::ConfigFlags::FLAG_WINDOW_MOUSE_PASSTHROUGH as u32,
        );
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, ShowWindow, GWL_EXSTYLE,
            WS_EX_TOOLWINDOW, WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_TOPMOST,
            SW_HIDE, SW_SHOWNOACTIVATE,
        };
        let hwnd = raylib::ffi::GetWindowHandle();
        ShowWindow(hwnd, SW_HIDE);
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            ex_style | WS_EX_TOOLWINDOW as i32 | WS_EX_LAYERED as i32
                     | WS_EX_TRANSPARENT as i32 | WS_EX_TOPMOST as i32,
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        log::info!("Windows overlay: set WS_EX_TOOLWINDOW (hidden from taskbar)");
    }

    raylib_handle.set_target_fps(60);

    (raylib_handle, raylib_thread)
}

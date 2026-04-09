/// Raylib overlay renderer — draws the blue cursor triangle, speech bubbles,
/// waveform visualization, spinner, and bezier flight animations.
///
/// This is the visual heart of Clicky. It runs at 60fps on a transparent
/// borderless window that overlays the entire virtual desktop.

use raylib::prelude::*;
use crate::app::state_machine::VoiceState;
use crate::core::{bezier_flight, design_system};

/// Navigation mode for the blue cursor triangle.
/// Mirrors BuddyNavigationMode from OverlayWindow.swift:90-97.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorNavigationMode {
    /// Cursor follows the real mouse with spring animation
    FollowingMouse,

    /// Cursor is flying to a target element via bezier arc
    NavigatingToTarget,

    /// Cursor has arrived at target, showing speech bubble
    PointingAtTarget,
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
}

impl ActiveFlightAnimation {
    pub fn new(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> Self {
        let (control_x, control_y) =
            bezier_flight::compute_control_point(start_x, start_y, end_x, end_y);
        let duration_seconds =
            bezier_flight::compute_flight_duration_seconds(start_x, start_y, end_x, end_y);

        Self {
            start_x,
            start_y,
            control_x,
            control_y,
            end_x,
            end_y,
            duration_seconds,
            elapsed_seconds: 0.0,
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
        }
    }

    /// Advances the flight animation by one frame's worth of time.
    pub fn advance_flight_animation(&mut self, delta_seconds: f64) {
        if let Some(flight) = &mut self.active_flight {
            flight.elapsed_seconds += delta_seconds;
            let progress = flight.linear_progress();

            let frame = bezier_flight::compute_flight_frame(
                progress,
                flight.start_x,
                flight.start_y,
                flight.control_x,
                flight.control_y,
                flight.end_x,
                flight.end_y,
            );

            self.cursor_x = frame.x as f32;
            self.cursor_y = frame.y as f32;
            self.cursor_rotation_degrees = frame.rotation_radians.to_degrees();
            self.cursor_scale = frame.scale;

            if flight.is_complete() {
                self.navigation_mode = CursorNavigationMode::PointingAtTarget;
                self.cursor_rotation_degrees = design_system::cursor::DEFAULT_ROTATION_DEGREES;
                self.cursor_scale = 1.0;
                self.active_flight = None;
            }
        }
    }

    /// Starts a bezier flight from the current cursor position to a target.
    pub fn start_flight_to(&mut self, target_x: f64, target_y: f64, bubble_text: String) {
        self.navigation_mode = CursorNavigationMode::NavigatingToTarget;
        self.active_flight = Some(ActiveFlightAnimation::new(
            self.cursor_x as f64,
            self.cursor_y as f64,
            target_x,
            target_y,
        ));
        self.speech_bubble_text = bubble_text;
        self.speech_bubble_visible_char_count = 0;
        self.speech_bubble_opacity = 0.0;
    }

    /// Returns the cursor to following the mouse.
    pub fn return_to_following_mouse(&mut self) {
        self.navigation_mode = CursorNavigationMode::FollowingMouse;
        self.speech_bubble_opacity = 0.0;
        self.speech_bubble_visible_char_count = 0;
        self.cursor_rotation_degrees = design_system::cursor::DEFAULT_ROTATION_DEGREES;
        self.cursor_scale = 1.0;
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
            draw_spinner(draw_handle, render_state);
        }
    }
}

/// Draws the blue equilateral triangle cursor with glow.
fn draw_cursor_triangle(
    draw_handle: &mut RaylibDrawHandle,
    render_state: &OverlayRenderState,
) {
    // Offset the triangle from the system cursor so they don't overlap
    let center_x = render_state.cursor_x + 20.0;
    let center_y = render_state.cursor_y + 20.0;
    let size = design_system::cursor::TRIANGLE_SIZE * render_state.cursor_scale as f32;
    let rotation_radians = render_state.cursor_rotation_degrees.to_radians() as f32;

    // Triangle vertices (equilateral, pointing up before rotation)
    let height = size * 0.866; // sqrt(3)/2
    let half_base = size / 2.0;

    // Vertices relative to center, before rotation
    let vertices = [
        (0.0, -height * 0.6),           // top vertex
        (-half_base, height * 0.4),      // bottom-left
        (half_base, height * 0.4),       // bottom-right
    ];

    // Apply rotation and translate to screen position
    let cos_r = rotation_radians.cos();
    let sin_r = rotation_radians.sin();

    let rotated_vertices: Vec<Vector2> = vertices
        .iter()
        .map(|(vx, vy)| {
            let rotated_x = vx * cos_r - vy * sin_r;
            let rotated_y = vx * sin_r + vy * cos_r;
            Vector2::new(center_x + rotated_x, center_y + rotated_y)
        })
        .collect();

    // Draw glow (larger, semi-transparent circle behind the triangle)
    let glow_radius = if render_state.navigation_mode == CursorNavigationMode::PointingAtTarget {
        design_system::cursor::GLOW_RADIUS_POINTING
    } else {
        design_system::cursor::GLOW_RADIUS_IDLE
    };
    let glow_color = ds_color(design_system::colors::OVERLAY_CURSOR_GLOW);
    draw_handle.draw_circle(
        center_x as i32,
        center_y as i32,
        glow_radius * render_state.cursor_scale as f32,
        glow_color,
    );

    // Draw the filled triangle
    let triangle_color = ds_color(design_system::colors::OVERLAY_CURSOR_BLUE);
    draw_handle.draw_triangle(
        rotated_vertices[0],
        rotated_vertices[1],
        rotated_vertices[2],
        triangle_color,
    );
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

/// Draws a processing spinner (shown while waiting for Claude's response).
fn draw_spinner(
    draw_handle: &mut RaylibDrawHandle,
    render_state: &OverlayRenderState,
) {
    let center_x = render_state.cursor_x;
    let center_y = render_state.cursor_y;
    let time = draw_handle.get_time() as f32;
    let spinner_color = ds_color(design_system::colors::STATUS_PROCESSING);

    // Rotating dots
    let dot_count = 8;
    let radius = 12.0f32;
    let dot_radius = 2.5f32;

    for i in 0..dot_count {
        let angle = (i as f32 / dot_count as f32) * std::f32::consts::TAU + time * 4.0;
        let dot_x = center_x + angle.cos() * radius;
        let dot_y = center_y + angle.sin() * radius;

        // Fade dots based on position in the rotation
        let alpha_factor = ((i as f32 / dot_count as f32) * 255.0) as u8;
        let dot_color = Color::new(
            spinner_color.r,
            spinner_color.g,
            spinner_color.b,
            alpha_factor,
        );

        draw_handle.draw_circle(dot_x as i32, dot_y as i32, dot_radius, dot_color);
    }
}

/// Draws the speech bubble next to the cursor when pointing at an element.
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

    let bubble_x = render_state.cursor_x + 20.0;
    let bubble_y = render_state.cursor_y - 30.0;
    let font_size = 14;
    let padding = 10.0f32;

    let text_width = draw_handle.measure_text(&visible_text, font_size) as f32;
    let bubble_width = text_width + padding * 2.0;
    let bubble_height = font_size as f32 + padding * 2.0;

    // Background with rounded corners
    let bg_alpha = (render_state.speech_bubble_opacity * 235.0) as u8;
    let bubble_bg = Color::new(30, 30, 38, bg_alpha);
    draw_handle.draw_rectangle_rounded(
        Rectangle::new(bubble_x, bubble_y, bubble_width, bubble_height),
        0.3,
        8,
        bubble_bg,
    );

    // Text
    let text_alpha = (render_state.speech_bubble_opacity * 255.0) as u8;
    let text_color = Color::new(242, 242, 245, text_alpha);
    draw_handle.draw_text(
        &visible_text,
        (bubble_x + padding) as i32,
        (bubble_y + padding) as i32,
        font_size,
        text_color,
    );
}

/// Initializes the Raylib window for the overlay.
/// Returns the RaylibHandle and RaylibThread needed for the render loop.
///
/// The window is created transparent, undecorated, and topmost.
/// Platform-specific click-through patching happens after this call.
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

    // Set topmost + mouse passthrough (GLFW handles X11 input shape correctly)
    unsafe {
        raylib::ffi::SetWindowState(
            raylib::ffi::ConfigFlags::FLAG_WINDOW_TOPMOST as u32
                | raylib::ffi::ConfigFlags::FLAG_WINDOW_MOUSE_PASSTHROUGH as u32,
        );
    }

    // Windows: hide from taskbar and Alt+Tab by setting WS_EX_TOOLWINDOW.
    // Must hide → change style → show, because Windows caches the taskbar
    // button state when the window is first displayed.
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, ShowWindow, GWL_EXSTYLE,
            WS_EX_TOOLWINDOW, WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_TOPMOST,
            SW_HIDE, SW_SHOWNOACTIVATE,
        };
        let hwnd = raylib::ffi::GetWindowHandle();
        // Hide first so the taskbar button is removed
        ShowWindow(hwnd, SW_HIDE);
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            ex_style | WS_EX_TOOLWINDOW as i32 | WS_EX_LAYERED as i32
                     | WS_EX_TRANSPARENT as i32 | WS_EX_TOPMOST as i32,
        );
        // Show again without activating (no focus steal)
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        log::info!("Windows overlay: set WS_EX_TOOLWINDOW (hidden from taskbar)");
    }

    raylib_handle.set_target_fps(60);

    (raylib_handle, raylib_thread)
}

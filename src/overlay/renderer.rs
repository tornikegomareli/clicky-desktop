use crate::app::platform::PlatformInfo;
use crate::app::state_machine::VoiceState;
use crate::core::{bezier_flight, design_system};
/// Raylib overlay renderer — draws the blue cursor triangle, speech bubbles,
/// waveform visualization, loading animation, and bezier flight animations.
///
/// This is the visual heart of Clicky. It runs at 60fps on a transparent
/// borderless window that overlays the entire virtual desktop.
use raylib::prelude::*;

#[cfg(target_os = "linux")]
use crate::app::platform::DisplayServer;
#[cfg(target_os = "linux")]
use std::ffi::{c_ulong, c_void};
#[cfg(target_os = "linux")]
use x11rb::connection::Connection;
#[cfg(target_os = "linux")]
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as XprotoConnectionExt, PropMode};
#[cfg(target_os = "linux")]
use x11rb::wrapper::ConnectionExt as XprotoWrapperExt;

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
        let duration_seconds = bezier_flight::compute_flight_duration_seconds(
            start_x, start_y, end_x, end_y, is_return,
        );

        Self {
            start_x,
            start_y,
            control_x,
            control_y,
            end_x,
            end_y,
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
    pub speech_bubble_visible_char_progress: f32,
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
            speech_bubble_visible_char_progress: 0.0,
            speech_bubble_visible_char_count: 0,
            speech_bubble_opacity: 0.0,
            bubble_font: None,
        }
    }

    /// Updates the overlay state for the current frame (animations, etc).
    pub fn update(&mut self, delta_seconds: f64, current_mouse_position: Option<(f32, f32)>) {
        // 1. Advance navigation animation if active
        if self.navigation_mode == CursorNavigationMode::NavigatingToTarget {
            self.advance_flight_animation(delta_seconds);
        } else if self.navigation_mode == CursorNavigationMode::ReturningToMouse {
            if let Some((mouse_x, mouse_y)) = current_mouse_position {
                self.advance_return_to_mouse(delta_seconds, mouse_x, mouse_y);
            }
        }

        // 2. Animate speech bubble typing and opacity
        let is_visible = self.navigation_mode == CursorNavigationMode::PointingAtTarget
            || self.navigation_mode == CursorNavigationMode::NavigatingToTarget;

        if is_visible && !self.speech_bubble_text.is_empty() {
            // Fade in opacity
            self.speech_bubble_opacity =
                (self.speech_bubble_opacity + delta_seconds as f32 * 4.0).min(1.0);

            // Typewriter effect: ~30 chars per second
            if self.speech_bubble_visible_char_count < self.speech_bubble_text.len() {
                let char_speed = 30.0; // chars per second
                self.speech_bubble_visible_char_progress =
                    (self.speech_bubble_visible_char_progress + delta_seconds as f32 * char_speed)
                        .min(self.speech_bubble_text.len() as f32);
                self.speech_bubble_visible_char_count =
                    self.speech_bubble_visible_char_progress.floor() as usize;
            }
        } else if self.navigation_mode == CursorNavigationMode::ReturningToMouse
            || self.navigation_mode == CursorNavigationMode::FollowingMouse
        {
            // Fade out opacity
            self.speech_bubble_opacity =
                (self.speech_bubble_opacity - delta_seconds as f32 * 5.0).max(0.0);
            if self.speech_bubble_opacity <= 0.0 {
                self.speech_bubble_visible_char_progress = 0.0;
                self.speech_bubble_visible_char_count = 0;
            }
        }
    }

    fn advance_return_to_mouse(&mut self, delta_seconds: f64, mouse_x: f32, mouse_y: f32) {
        let direction_x = mouse_x - self.cursor_x;
        let direction_y = mouse_y - self.cursor_y;
        let distance = (direction_x * direction_x + direction_y * direction_y).sqrt();

        if distance <= 6.0 {
            self.cursor_x = mouse_x;
            self.cursor_y = mouse_y;
            self.navigation_mode = CursorNavigationMode::FollowingMouse;
            self.cursor_rotation_degrees = design_system::cursor::DEFAULT_ROTATION_DEGREES;
            self.cursor_scale = 1.0;
            self.active_flight = None;
            return;
        }

        let speed = 900.0f32;
        let step = (speed * delta_seconds as f32).min(distance);
        let unit_x = direction_x / distance.max(0.001);
        let unit_y = direction_y / distance.max(0.001);
        self.cursor_x += unit_x * step;
        self.cursor_y += unit_y * step;
        self.cursor_rotation_degrees = (unit_y as f64).atan2(unit_x as f64).to_degrees();

        let pulse = (distance / 240.0).clamp(0.0, 1.0) as f64;
        self.cursor_scale = 1.0 + pulse * 0.05;
        self.active_flight = None;
    }

    /// Advances the flight animation by one frame's worth of time.
    fn advance_flight_animation(&mut self, delta_seconds: f64) {
        if let Some(flight) = &mut self.active_flight {
            flight.elapsed_seconds += delta_seconds;
            let progress = flight.linear_progress();
            let is_return = flight.is_return;

            let frame = bezier_flight::compute_flight_frame(
                progress,
                flight.start_x,
                flight.start_y,
                flight.control_x,
                flight.control_y,
                flight.end_x,
                flight.end_y,
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
                    self.speech_bubble_visible_char_progress = 0.0;
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
        self.speech_bubble_visible_char_progress = 0.0;
        self.speech_bubble_visible_char_count = 0;
        self.speech_bubble_opacity = 0.0;
    }

    /// Starts a bezier return flight from current position back to mouse.
    pub fn start_return_flight(&mut self, mouse_x: f32, mouse_y: f32) {
        self.navigation_mode = CursorNavigationMode::ReturningToMouse;
        self.cursor_rotation_degrees = (mouse_y as f64 - self.cursor_y as f64)
            .atan2(mouse_x as f64 - self.cursor_x as f64)
            .to_degrees();
        self.active_flight = None;
        self.speech_bubble_opacity = 0.0;
        self.speech_bubble_visible_char_progress = 0.0;
        self.speech_bubble_visible_char_count = 0;
    }

    /// Returns the cursor to following the mouse (instant, no animation).
    pub fn return_to_following_mouse(&mut self) {
        self.navigation_mode = CursorNavigationMode::FollowingMouse;
        self.speech_bubble_opacity = 0.0;
        self.speech_bubble_visible_char_progress = 0.0;
        self.speech_bubble_visible_char_count = 0;
        self.cursor_rotation_degrees = design_system::cursor::DEFAULT_ROTATION_DEGREES;
        self.cursor_scale = 1.0;
    }
}

/// Draws one frame of the overlay. Called 60 times per second from the main loop.
pub fn draw_overlay_frame(draw_handle: &mut RaylibDrawHandle, render_state: &OverlayRenderState) {
    draw_handle.clear_background(Color::BLANK);

    match render_state.voice_state {
        VoiceState::Idle | VoiceState::Responding => {
            draw_cursor_triangle(draw_handle, render_state);

            let bubble_visible = (render_state.navigation_mode
                == CursorNavigationMode::PointingAtTarget
                || render_state.navigation_mode == CursorNavigationMode::NavigatingToTarget)
                && render_state.speech_bubble_opacity > 0.01;

            if bubble_visible {
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
fn draw_cursor_triangle(draw_handle: &mut RaylibDrawHandle, render_state: &OverlayRenderState) {
    let offset = design_system::cursor::OFFSET_FROM_SYSTEM_CURSOR;
    let center_x = render_state.cursor_x + offset;
    let center_y = render_state.cursor_y + offset;
    let base_size = design_system::cursor::TRIANGLE_SIZE * render_state.cursor_scale as f32;
    let glow_base_size = design_system::cursor::TRIANGLE_SIZE;
    let rotation_radians = render_state.cursor_rotation_degrees.to_radians() as f32;
    let cos_r = rotation_radians.cos();
    let sin_r = rotation_radians.sin();

    let is_pointing = render_state.navigation_mode == CursorNavigationMode::PointingAtTarget;

    let blue = design_system::colors::OVERLAY_CURSOR_BLUE;
    let blue_r = (blue.0 * 255.0) as u8;
    let blue_g = (blue.1 * 255.0) as u8;
    let blue_b = (blue.2 * 255.0) as u8;

    let halo_radius = 20.0;
    let halo_alpha = if is_pointing { 52 } else { 42 };

    draw_handle.draw_circle_gradient(
        center_x as i32,
        center_y as i32,
        halo_radius,
        Color::new(blue_r, blue_g, blue_b, halo_alpha),
        Color::new(blue_r, blue_g, blue_b, 0),
    );

    if is_pointing {
        draw_handle.draw_circle_gradient(
            center_x as i32,
            center_y as i32,
            halo_radius * 0.72,
            Color::new(255, 255, 255, 24),
            Color::new(255, 255, 255, 0),
        );
    }

    let bloom_layers: &[(f32, u8)] = &[(2.4, 6), (1.9, 12), (1.45, 24)];

    for &(scale_factor, alpha) in bloom_layers {
        let bloom_size = glow_base_size * scale_factor;
        let verts = triangle_vertices(bloom_size, center_x, center_y, cos_r, sin_r);
        draw_handle.draw_triangle(
            verts[0],
            verts[1],
            verts[2],
            Color::new(blue_r, blue_g, blue_b, alpha),
        );
    }

    // Main solid triangle
    let verts = triangle_vertices(base_size, center_x, center_y, cos_r, sin_r);
    draw_handle.draw_triangle(
        verts[0],
        verts[1],
        verts[2],
        Color::new(blue_r, blue_g, blue_b, 255),
    );

    let inner_glint = triangle_vertices(
        base_size * 0.62,
        center_x - 0.8,
        center_y - 1.0,
        cos_r,
        sin_r,
    );
    draw_handle.draw_triangle(
        inner_glint[0],
        inner_glint[1],
        inner_glint[2],
        Color::new(255, 255, 255, if is_pointing { 78 } else { 48 }),
    );

    draw_handle.draw_triangle_lines(
        verts[0],
        verts[1],
        verts[2],
        Color::new(220, 236, 255, if is_pointing { 240 } else { 170 }),
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
        Vector2::new(
            cx + raw[0].0 * cos_r - raw[0].1 * sin_r,
            cy + raw[0].0 * sin_r + raw[0].1 * cos_r,
        ),
        Vector2::new(
            cx + raw[1].0 * cos_r - raw[1].1 * sin_r,
            cy + raw[1].0 * sin_r + raw[1].1 * cos_r,
        ),
        Vector2::new(
            cx + raw[2].0 * cos_r - raw[2].1 * sin_r,
            cy + raw[2].0 * sin_r + raw[2].1 * cos_r,
        ),
    ]
}

/// Draws the waveform visualization (shown during listening state).
fn draw_waveform(draw_handle: &mut RaylibDrawHandle, render_state: &OverlayRenderState) {
    let center_x = render_state.cursor_x;
    let center_y = render_state.cursor_y;
    let bar_color = ds_color(design_system::colors::WAVEFORM_BAR);

    let bar_count = render_state.audio_power_history.len().min(20);
    let bar_width = 3.0f32;
    let bar_gap = 2.0f32;
    let max_bar_height = 30.0f32;
    let total_width = bar_count as f32 * (bar_width + bar_gap) - bar_gap;
    let start_x = center_x - total_width / 2.0;

    let history_offset = render_state
        .audio_power_history
        .len()
        .saturating_sub(bar_count);

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
fn draw_loading_arc(draw_handle: &mut RaylibDrawHandle, render_state: &OverlayRenderState) {
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
    draw_handle.draw_circle(
        dot_x as i32,
        dot_y as i32,
        3.5,
        Color::new(blue_r, blue_g, blue_b, 240),
    );

    // Subtle pulsing center dot
    let pulse = (time * 2.0).sin() * 0.5 + 0.5;
    let center_alpha = (pulse * 120.0 + 40.0) as u8;
    draw_handle.draw_circle(
        center_x as i32,
        center_y as i32,
        2.5,
        Color::new(blue_r, blue_g, blue_b, center_alpha),
    );
}

/// Draws the speech bubble — glass-effect with blue tint and white text.
/// Uses a custom TTF font when available, falls back to Raylib default.
fn draw_speech_bubble(draw_handle: &mut RaylibDrawHandle, render_state: &OverlayRenderState) {
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

    let screen_w = draw_handle.get_screen_width() as f32;
    let screen_h = draw_handle.get_screen_height() as f32;

    let offset = design_system::cursor::OFFSET_FROM_SYSTEM_CURSOR;
    let cursor_center_x = render_state.cursor_x + offset;
    let cursor_center_y = render_state.cursor_y + offset;

    let font_size = 18.0f32;
    let spacing = 1.0f32;
    let pad_h = 15.0f32;
    let pad_v = 11.0f32;
    let line_gap = 4.0f32;
    let opacity = render_state.speech_bubble_opacity;
    let max_text_width = (screen_w * 0.28).clamp(180.0, 320.0);

    let blue = design_system::colors::OVERLAY_CURSOR_BLUE;
    let blue_r = (blue.0 * 255.0) as u8;
    let blue_g = (blue.1 * 255.0) as u8;
    let blue_b = (blue.2 * 255.0) as u8;

    let wrapped_lines = wrap_text_lines(
        draw_handle,
        render_state.bubble_font.as_ref(),
        &visible_text,
        font_size,
        spacing,
        max_text_width,
    );
    if wrapped_lines.is_empty() {
        return;
    }

    let mut text_width = 0.0f32;
    for line in &wrapped_lines {
        text_width = text_width.max(measure_text_width(
            draw_handle,
            render_state.bubble_font.as_ref(),
            line,
            font_size,
            spacing,
        ));
    }
    let line_height = if let Some(ref font) = render_state.bubble_font {
        font.base_size() as f32 * (font_size / font.base_size() as f32)
    } else {
        font_size
    };
    let text_height = wrapped_lines.len() as f32 * line_height
        + (wrapped_lines.len().saturating_sub(1) as f32 * line_gap);

    let bubble_width = text_width + pad_h * 2.0;
    let bubble_height = text_height + pad_v * 2.0;

    let appear = opacity.clamp(0.0, 1.0);
    let rise_offset = (1.0 - appear) * 8.0;
    let mut bubble_x = cursor_center_x + 12.0;
    let mut bubble_y = cursor_center_y - bubble_height - 10.0 + rise_offset;

    if bubble_x + bubble_width > screen_w - 20.0 {
        bubble_x = cursor_center_x - bubble_width - 12.0;
    }
    if bubble_y < 20.0 {
        bubble_y = cursor_center_y + 12.0 - rise_offset * 0.35;
    }
    if bubble_y + bubble_height > screen_h - 20.0 {
        bubble_y = (screen_h - bubble_height - 20.0).max(20.0);
    }

    let corner_radius = 20.0f32;
    let roundness = (corner_radius / (bubble_height.min(bubble_width) / 2.0)).clamp(0.0, 1.0);
    let corner_segments = 14;

    let shadow_alpha = (opacity * 22.0) as u8;
    for i in 1..=2 {
        let spread = i as f32 * 2.0;
        draw_handle.draw_rectangle_rounded(
            Rectangle::new(
                bubble_x + spread * 0.4,
                bubble_y + spread * 0.8 + 1.0,
                bubble_width,
                bubble_height,
            ),
            roundness,
            corner_segments,
            Color::new(0, 0, 0, (shadow_alpha / i) as u8),
        );
    }

    let bg_alpha = (opacity * 236.0) as u8;
    let bubble_blue = Color::new(
        ((blue_r as f32 * 0.60) + 12.0) as u8,
        ((blue_g as f32 * 0.60) + 16.0) as u8,
        ((blue_b as f32 * 0.60) + 18.0) as u8,
        bg_alpha,
    );
    draw_handle.draw_rectangle_rounded(
        Rectangle::new(bubble_x, bubble_y, bubble_width, bubble_height),
        roundness,
        corner_segments,
        bubble_blue,
    );

    draw_handle.draw_rectangle_lines_ex(
        Rectangle::new(
            bubble_x + 1.0,
            bubble_y + 1.0,
            bubble_width - 2.0,
            bubble_height - 2.0,
        ),
        1.0,
        Color::new(255, 255, 255, (opacity * 8.0) as u8),
    );

    let text_alpha = (opacity * 255.0) as u8;
    let text_shadow = Color::new(10, 18, 34, (opacity * 40.0) as u8);
    let text_color = Color::new(255, 255, 255, text_alpha);
    for (index, line) in wrapped_lines.iter().enumerate() {
        let text_pos = Vector2::new(
            bubble_x + pad_h,
            bubble_y + pad_v + index as f32 * (line_height + line_gap),
        );

        if let Some(ref font) = render_state.bubble_font {
            draw_handle.draw_text_ex(
                font,
                line,
                Vector2::new(text_pos.x, text_pos.y + 1.0),
                font_size,
                spacing,
                text_shadow,
            );
            draw_handle.draw_text_ex(font, line, text_pos, font_size, spacing, text_color);
        } else {
            draw_handle.draw_text(
                line,
                text_pos.x as i32,
                (text_pos.y + 1.0) as i32,
                font_size as i32,
                text_shadow,
            );
            draw_handle.draw_text(
                line,
                text_pos.x as i32,
                text_pos.y as i32,
                font_size as i32,
                text_color,
            );
        }
    }
}

fn measure_text_width(
    draw_handle: &RaylibDrawHandle,
    font: Option<&Font>,
    text: &str,
    font_size: f32,
    spacing: f32,
) -> f32 {
    if let Some(font) = font {
        font.measure_text(text, font_size, spacing).x
    } else {
        draw_handle.measure_text(text, font_size as i32) as f32
    }
}

fn wrap_text_lines(
    draw_handle: &RaylibDrawHandle,
    font: Option<&Font>,
    text: &str,
    font_size: f32,
    spacing: f32,
    max_width: f32,
) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in words {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };

        if !current.is_empty()
            && measure_text_width(draw_handle, font, &candidate, font_size, spacing) > max_width
        {
            lines.push(current);
            current = word.to_string();
        } else {
            current = candidate;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
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
#[cfg(target_os = "macos")]
const FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/HelveticaNeue.ttc",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Supplemental/Verdana.ttf",
];

#[cfg(target_os = "windows")]
const FONT_PATHS: &[&str] = &[
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "C:\\Windows\\Fonts\\arial.ttf",
    "C:\\Windows\\Fonts\\verdana.ttf",
];

#[cfg(target_os = "linux")]
const FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/opentype/urw-base35/NimbusSans-Regular.otf",
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf",
];

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const FONT_PATHS: &[&str] = &[];

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
    #[allow(unused_variables)] platform: &PlatformInfo,
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

    #[cfg(target_os = "linux")]
    configure_linux_overlay_window(platform);

    raylib_handle.set_target_fps(60);

    (raylib_handle, raylib_thread)
}

#[cfg(target_os = "linux")]
fn configure_linux_overlay_window(platform: &PlatformInfo) {
    match platform.display_server {
        Some(DisplayServer::X11) => {
            if let Err(error) = apply_x11_overlay_hints() {
                log::warn!(
                    "Linux X11 overlay: failed to set skip-taskbar hints: {}",
                    error
                );
            } else {
                log::info!("Linux X11 overlay: enabled skip-taskbar and skip-pager hints");
            }
        }
        Some(DisplayServer::Wayland) => {
            log::info!(
                "Linux Wayland overlay: dock/taskbar suppression is compositor-limited with the current Raylib/GLFW backend"
            );
        }
        None => {
            log::debug!(
                "Linux overlay: display server unknown, leaving default window-manager hints"
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn apply_x11_overlay_hints() -> Result<(), String> {
    let glfw_window = unsafe { raylib::ffi::GetWindowHandle() };
    if glfw_window.is_null() {
        return Err("Raylib did not return a native window handle".into());
    }

    let x11_window = unsafe { glfwGetX11Window(glfw_window.cast()) };
    if x11_window == 0 {
        return Err("GLFW did not return an X11 window id".into());
    }

    let (connection, _) =
        x11rb::connect(None).map_err(|error| format!("Failed to connect to X11: {error}"))?;

    let net_wm_state = intern_atom(&connection, b"_NET_WM_STATE")?;
    let net_wm_state_skip_taskbar = intern_atom(&connection, b"_NET_WM_STATE_SKIP_TASKBAR")?;
    let net_wm_state_skip_pager = intern_atom(&connection, b"_NET_WM_STATE_SKIP_PAGER")?;
    let net_wm_state_above = intern_atom(&connection, b"_NET_WM_STATE_ABOVE")?;
    let net_wm_window_type = intern_atom(&connection, b"_NET_WM_WINDOW_TYPE")?;
    let net_wm_window_type_utility = intern_atom(&connection, b"_NET_WM_WINDOW_TYPE_UTILITY")?;

    connection
        .change_property32(
            PropMode::REPLACE,
            x11_window,
            net_wm_state,
            AtomEnum::ATOM,
            &[
                net_wm_state_skip_taskbar,
                net_wm_state_skip_pager,
                net_wm_state_above,
            ],
        )
        .map_err(|error| format!("Failed to set _NET_WM_STATE: {error}"))?;

    connection
        .change_property32(
            PropMode::REPLACE,
            x11_window,
            net_wm_window_type,
            AtomEnum::ATOM,
            &[net_wm_window_type_utility],
        )
        .map_err(|error| format!("Failed to set _NET_WM_WINDOW_TYPE: {error}"))?;

    connection
        .flush()
        .map_err(|error| format!("Failed to flush X11 window hints: {error}"))?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn intern_atom<C: Connection>(connection: &C, name: &[u8]) -> Result<u32, String> {
    connection
        .intern_atom(false, name)
        .map_err(|error| {
            format!(
                "Failed to request atom {}: {error}",
                String::from_utf8_lossy(name)
            )
        })?
        .reply()
        .map_err(|error| {
            format!(
                "Failed to read atom {}: {error}",
                String::from_utf8_lossy(name)
            )
        })
        .map(|reply| reply.atom)
}

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn glfwGetX11Window(window: *mut c_void) -> c_ulong;
}

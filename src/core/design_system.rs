/// Design system tokens — colors, spacing, corner radii, and animation constants.
/// Ported from DesignSystem.swift:1-880.
///
/// These values define the visual identity of Clicky's overlay and panel.
/// Colors are stored as (r, g, b, a) tuples in 0.0–1.0 range for
/// direct use with Raylib's Color type or egui's Color32.

/// Color tokens for the overlay and UI.
/// All values are (red, green, blue, alpha) in 0.0–1.0 range.
pub mod colors {
    /// The blue cursor triangle color — #3380FF
    pub const OVERLAY_CURSOR_BLUE: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 1.0);

    /// Glow color around the cursor during flight
    pub const OVERLAY_CURSOR_GLOW: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 0.4);

    /// Background colors (dark theme)
    pub const BACKGROUND: (f32, f32, f32, f32) = (0.067, 0.067, 0.078, 1.0);
    pub const SURFACE_1: (f32, f32, f32, f32) = (0.098, 0.098, 0.118, 1.0);
    pub const SURFACE_2: (f32, f32, f32, f32) = (0.129, 0.129, 0.157, 1.0);

    /// Text colors
    pub const TEXT_PRIMARY: (f32, f32, f32, f32) = (0.95, 0.95, 0.96, 1.0);
    pub const TEXT_SECONDARY: (f32, f32, f32, f32) = (0.65, 0.65, 0.70, 1.0);

    /// Accent blue — #2563EB
    pub const ACCENT: (f32, f32, f32, f32) = (0.145, 0.388, 0.922, 1.0);

    /// Speech bubble background
    pub const SPEECH_BUBBLE_BACKGROUND: (f32, f32, f32, f32) = (0.12, 0.12, 0.15, 0.92);

    /// Status indicator colors
    pub const STATUS_IDLE: (f32, f32, f32, f32) = (0.4, 0.4, 0.45, 1.0);
    pub const STATUS_LISTENING: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 1.0);
    pub const STATUS_PROCESSING: (f32, f32, f32, f32) = (1.0, 0.6, 0.0, 1.0);
    pub const STATUS_RESPONDING: (f32, f32, f32, f32) = (0.2, 0.8, 0.4, 1.0);

    /// Waveform bar color
    pub const WAVEFORM_BAR: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 0.8);
}

/// Spacing tokens in logical pixels.
pub mod spacing {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 20.0;
    pub const XXL: f32 = 24.0;
    pub const XXXL: f32 = 32.0;
}

/// Corner radius tokens in logical pixels.
pub mod corner_radius {
    pub const SMALL: f32 = 6.0;
    pub const MEDIUM: f32 = 8.0;
    pub const LARGE: f32 = 10.0;
    pub const EXTRA_LARGE: f32 = 12.0;
}

/// Animation timing constants in seconds.
pub mod animation {
    pub const FAST: f64 = 0.15;
    pub const NORMAL: f64 = 0.25;
    pub const SLOW: f64 = 0.4;

    /// Character streaming delay range (seconds per character) for speech bubbles.
    /// Ported from OverlayWindow.swift:590 — 30-60ms per character.
    pub const CHARACTER_STREAM_MIN_DELAY: f64 = 0.030;
    pub const CHARACTER_STREAM_MAX_DELAY: f64 = 0.060;

    /// How long the speech bubble stays visible after text finishes streaming
    pub const SPEECH_BUBBLE_HOLD_DURATION: f64 = 3.0;

    /// Fade-out duration for the speech bubble
    pub const SPEECH_BUBBLE_FADE_OUT_DURATION: f64 = 0.5;
}

/// Cursor triangle geometry.
pub mod cursor {
    /// Base size of the equilateral triangle cursor in logical pixels
    pub const TRIANGLE_SIZE: f32 = 18.0;

    /// Default rotation in degrees — the triangle points slightly to the upper-right,
    /// matching the macOS menu bar icon (rotated 35 degrees)
    pub const DEFAULT_ROTATION_DEGREES: f64 = -35.0;

    /// Glow shadow radius at rest
    pub const GLOW_RADIUS_IDLE: f32 = 6.0;

    /// Glow shadow radius during pointing animation
    pub const GLOW_RADIUS_POINTING: f32 = 22.0;

    /// Offset from logical position to avoid overlapping the system cursor.
    /// Flight targets must subtract this so the visual lands on the correct spot.
    pub const OFFSET_FROM_SYSTEM_CURSOR: f32 = 20.0;
}

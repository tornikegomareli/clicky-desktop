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

    /// Waveform bar color
    pub const WAVEFORM_BAR: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 0.8);

    /// Screen annotation colors
    pub const ANNOTATION_HIGHLIGHT: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 0.15);
    pub const ANNOTATION_HIGHLIGHT_BORDER: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 0.6);
    pub const ANNOTATION_CIRCLE: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 0.2);
    pub const ANNOTATION_CIRCLE_BORDER: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 0.7);
    pub const ANNOTATION_ARROW: (f32, f32, f32, f32) = (0.2, 0.5, 1.0, 0.8);
    pub const ANNOTATION_LABEL: (f32, f32, f32, f32) = (0.95, 0.95, 0.96, 0.9);
}

/// Cursor triangle geometry.
pub mod cursor {
    /// Base size of the equilateral triangle cursor in logical pixels
    pub const TRIANGLE_SIZE: f32 = 18.0;

    /// Default rotation in degrees — the triangle points slightly to the upper-right,
    /// matching the macOS menu bar icon (rotated 35 degrees)
    pub const DEFAULT_ROTATION_DEGREES: f64 = -35.0;

    /// Offset from logical position to avoid overlapping the system cursor.
    /// Flight targets must subtract this so the visual lands on the correct spot.
    pub const OFFSET_FROM_SYSTEM_CURSOR: f32 = 20.0;
}

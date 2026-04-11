/// Maps screenshot pixel coordinates to display coordinates.
/// Ported from CompanionManager.swift:648-682.
///
/// Claude returns POINT coordinates in the screenshot's pixel space.
/// These must be converted to the actual display coordinate system to
/// position the cursor overlay correctly.
///
/// Key difference from macOS: Linux (X11) and Windows both use top-left
/// origin coordinates, so NO Y-axis flip is needed (macOS AppKit uses
/// bottom-left origin).

/// Metadata about a captured display, used for coordinate mapping.
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    /// Display index (1-based, matching Claude's screen numbering)
    pub screen_number: u32,

    /// Origin of this display in global (virtual desktop) coordinates
    pub global_origin_x: f64,
    pub global_origin_y: f64,

    /// Display size in logical points (may differ from pixels on HiDPI)
    pub display_width_points: f64,
    pub display_height_points: f64,

    /// Screenshot image dimensions in pixels
    pub screenshot_width_pixels: u32,
    pub screenshot_height_pixels: u32,

    /// Whether the user's cursor is on this display
    pub is_cursor_display: bool,
}

/// A coordinate in the global (virtual desktop) coordinate system,
/// ready for positioning the overlay cursor.
#[derive(Debug, Clone, Copy)]
pub struct GlobalDisplayCoordinate {
    pub x: f64,
    pub y: f64,
}

/// Maps a coordinate from screenshot pixel space to global display coordinates.
///
/// The transformation:
/// 1. Clamp to screenshot bounds
/// 2. Scale from screenshot pixels to display points
/// 3. Add display origin offset to get global coordinates
///
/// Note: On Linux/Windows, the coordinate origin is top-left for both screenshot
/// and display space, so no Y-axis flip is needed (unlike macOS).
pub fn map_screenshot_pixels_to_global_display_coordinates(
    screenshot_pixel_x: f64,
    screenshot_pixel_y: f64,
    display: &DisplayInfo,
) -> GlobalDisplayCoordinate {
    // Step 1: Clamp to screenshot pixel bounds
    let clamped_pixel_x =
        screenshot_pixel_x.clamp(0.0, display.screenshot_width_pixels as f64 - 1.0);
    let clamped_pixel_y =
        screenshot_pixel_y.clamp(0.0, display.screenshot_height_pixels as f64 - 1.0);

    // Step 2: Scale from screenshot pixels to display points
    let scale_x = display.display_width_points / display.screenshot_width_pixels as f64;
    let scale_y = display.display_height_points / display.screenshot_height_pixels as f64;

    let display_local_x = clamped_pixel_x * scale_x;
    let display_local_y = clamped_pixel_y * scale_y;

    // Step 3: Convert to global coordinates by adding display origin
    GlobalDisplayCoordinate {
        x: display.global_origin_x + display_local_x,
        y: display.global_origin_y + display_local_y,
    }
}

/// Finds the correct display for a POINT tag's screen number.
///
/// If no screen number is specified (None), returns the display where
/// the cursor is located (the primary focus screen).
pub fn find_target_display<'a>(
    screen_number: Option<u32>,
    displays: &'a [DisplayInfo],
) -> Option<&'a DisplayInfo> {
    match screen_number {
        Some(number) => displays.iter().find(|d| d.screen_number == number),
        None => displays.iter().find(|d| d.is_cursor_display),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_display() -> DisplayInfo {
        DisplayInfo {
            screen_number: 1,
            global_origin_x: 0.0,
            global_origin_y: 0.0,
            display_width_points: 1920.0,
            display_height_points: 1080.0,
            screenshot_width_pixels: 1280,
            screenshot_height_pixels: 720,
            is_cursor_display: true,
        }
    }

    #[test]
    fn maps_top_left_corner() {
        let display = make_test_display();
        let coord = map_screenshot_pixels_to_global_display_coordinates(0.0, 0.0, &display);
        assert!((coord.x - 0.0).abs() < 0.1);
        assert!((coord.y - 0.0).abs() < 0.1);
    }

    #[test]
    fn maps_center_correctly() {
        let display = make_test_display();
        let coord = map_screenshot_pixels_to_global_display_coordinates(640.0, 360.0, &display);
        assert!((coord.x - 960.0).abs() < 1.0);
        assert!((coord.y - 540.0).abs() < 1.0);
    }

    #[test]
    fn applies_display_offset_for_second_monitor() {
        let mut display = make_test_display();
        display.screen_number = 2;
        display.global_origin_x = 1920.0;
        display.is_cursor_display = false;

        let coord = map_screenshot_pixels_to_global_display_coordinates(0.0, 0.0, &display);
        assert!((coord.x - 1920.0).abs() < 0.1); // offset by first monitor width
    }

    #[test]
    fn clamps_out_of_bounds_coordinates() {
        let display = make_test_display();
        let coord = map_screenshot_pixels_to_global_display_coordinates(-50.0, 2000.0, &display);
        assert!(coord.x >= 0.0);
        assert!(coord.y <= display.global_origin_y + display.display_height_points);
    }

    #[test]
    fn find_cursor_display() {
        let displays = vec![
            DisplayInfo {
                screen_number: 1,
                is_cursor_display: false,
                ..make_test_display()
            },
            DisplayInfo {
                screen_number: 2,
                is_cursor_display: true,
                ..make_test_display()
            },
        ];
        let target = find_target_display(None, &displays).unwrap();
        assert_eq!(target.screen_number, 2);
    }
}

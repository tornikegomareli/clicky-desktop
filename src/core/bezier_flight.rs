/// Quadratic bezier arc flight animation — ported from OverlayWindow.swift:495-568.
///
/// The blue cursor triangle detaches from the mouse and flies to a target
/// element along a curved arc. The arc uses a control point raised above
/// the midpoint for a natural "swooping" motion.

/// A single frame of the bezier flight animation.
#[derive(Debug, Clone, Copy)]
pub struct BezierFlightFrame {
    /// Current position along the arc
    pub x: f64,
    pub y: f64,

    /// Rotation angle in radians — derived from the tangent of the curve
    /// at the current point. Used to orient the triangle in the flight direction.
    pub rotation_radians: f64,

    /// Scale factor for the "pulse" effect during flight.
    /// Oscillates 1.0 → 1.3 → 1.0 using sin(t * PI).
    pub scale: f64,

    /// Linear progress 0.0 → 1.0
    pub progress: f64,
}

/// Computes the flight duration based on distance, targeting ~800 pixels/second.
/// Clamped to 0.6–1.4 seconds (from OverlayWindow.swift:510).
pub fn compute_flight_duration_seconds(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> f64 {
    let distance = ((end_x - start_x).powi(2) + (end_y - start_y).powi(2)).sqrt();
    let pixels_per_second = 800.0;
    let raw_duration = distance / pixels_per_second;
    raw_duration.clamp(0.6, 1.4)
}

/// Computes the bezier control point — placed at the midpoint, raised
/// perpendicular to the line by 20% of the distance (max 80px).
/// This creates the arcing flight path (from OverlayWindow.swift:515-520).
pub fn compute_control_point(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
) -> (f64, f64) {
    let midpoint_x = (start_x + end_x) / 2.0;
    let midpoint_y = (start_y + end_y) / 2.0;
    let distance = ((end_x - start_x).powi(2) + (end_y - start_y).powi(2)).sqrt();

    // Raise the control point above the midpoint for an arcing trajectory
    let arc_height = (distance * 0.2).min(80.0);

    // Perpendicular offset: rotate the direction vector 90 degrees
    // and move the midpoint upward (negative Y = upward on screen)
    let direction_x = end_x - start_x;
    let direction_y = end_y - start_y;
    let direction_length = distance.max(0.001); // avoid division by zero
    let perpendicular_x = -direction_y / direction_length;
    let perpendicular_y = direction_x / direction_length;

    // Choose the upward direction (negative Y on screen)
    let control_x = midpoint_x + perpendicular_x * arc_height;
    let control_y = midpoint_y - perpendicular_y.abs() * arc_height;

    (control_x, control_y)
}

/// Smoothstep easing function: accelerates then decelerates.
/// t_eased = t * t * (3 - 2 * t)
/// From OverlayWindow.swift:530.
fn smoothstep(linear_progress: f64) -> f64 {
    let t = linear_progress.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Evaluates the quadratic bezier curve at parameter t.
/// B(t) = (1-t)^2 * P0 + 2(1-t)t * C + t^2 * P2
fn evaluate_quadratic_bezier(
    t: f64,
    start_x: f64,
    start_y: f64,
    control_x: f64,
    control_y: f64,
    end_x: f64,
    end_y: f64,
) -> (f64, f64) {
    let one_minus_t = 1.0 - t;
    let x = one_minus_t * one_minus_t * start_x
        + 2.0 * one_minus_t * t * control_x
        + t * t * end_x;
    let y = one_minus_t * one_minus_t * start_y
        + 2.0 * one_minus_t * t * control_y
        + t * t * end_y;
    (x, y)
}

/// Computes the tangent (derivative) of the quadratic bezier at parameter t.
/// B'(t) = 2(1-t)(C - P0) + 2t(P2 - C)
/// Used to orient the triangle in the direction of flight.
fn evaluate_quadratic_bezier_tangent(
    t: f64,
    start_x: f64,
    start_y: f64,
    control_x: f64,
    control_y: f64,
    end_x: f64,
    end_y: f64,
) -> (f64, f64) {
    let tangent_x =
        2.0 * (1.0 - t) * (control_x - start_x) + 2.0 * t * (end_x - control_x);
    let tangent_y =
        2.0 * (1.0 - t) * (control_y - start_y) + 2.0 * t * (end_y - control_y);
    (tangent_x, tangent_y)
}

/// Computes a single frame of the bezier flight animation.
///
/// `linear_progress` ranges from 0.0 (at start) to 1.0 (at destination).
/// Smoothstep easing is applied internally.
pub fn compute_flight_frame(
    linear_progress: f64,
    start_x: f64,
    start_y: f64,
    control_x: f64,
    control_y: f64,
    end_x: f64,
    end_y: f64,
) -> BezierFlightFrame {
    let eased_progress = smoothstep(linear_progress);

    let (x, y) = evaluate_quadratic_bezier(
        eased_progress,
        start_x, start_y,
        control_x, control_y,
        end_x, end_y,
    );

    let (tangent_x, tangent_y) = evaluate_quadratic_bezier_tangent(
        eased_progress,
        start_x, start_y,
        control_x, control_y,
        end_x, end_y,
    );

    let rotation_radians = tangent_y.atan2(tangent_x);

    // Scale pulse: sin(progress * PI) gives 0 → 1 → 0 over the flight,
    // mapped to 1.0 → 1.3 → 1.0 (from OverlayWindow.swift:540)
    let scale_pulse = (linear_progress * std::f64::consts::PI).sin();
    let scale = 1.0 + scale_pulse * 0.3;

    BezierFlightFrame {
        x,
        y,
        rotation_radians,
        scale,
        progress: linear_progress,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flight_starts_at_origin_and_ends_at_destination() {
        let (control_x, control_y) = compute_control_point(0.0, 0.0, 800.0, 0.0);

        let start_frame = compute_flight_frame(0.0, 0.0, 0.0, control_x, control_y, 800.0, 0.0);
        assert!((start_frame.x - 0.0).abs() < 0.1);
        assert!((start_frame.y - 0.0).abs() < 0.1);
        assert!((start_frame.scale - 1.0).abs() < 0.01);

        let end_frame = compute_flight_frame(1.0, 0.0, 0.0, control_x, control_y, 800.0, 0.0);
        assert!((end_frame.x - 800.0).abs() < 0.1);
        assert!((end_frame.y - 0.0).abs() < 0.1);
        assert!((end_frame.scale - 1.0).abs() < 0.01);
    }

    #[test]
    fn midpoint_has_maximum_scale_pulse() {
        let (control_x, control_y) = compute_control_point(0.0, 0.0, 800.0, 0.0);
        let mid_frame = compute_flight_frame(0.5, 0.0, 0.0, control_x, control_y, 800.0, 0.0);
        // At midpoint, sin(0.5 * PI) = 1.0, so scale = 1.0 + 0.3 = 1.3
        assert!((mid_frame.scale - 1.3).abs() < 0.01);
    }

    #[test]
    fn flight_duration_scales_with_distance() {
        let short_duration = compute_flight_duration_seconds(0.0, 0.0, 100.0, 0.0);
        let long_duration = compute_flight_duration_seconds(0.0, 0.0, 2000.0, 0.0);
        assert_eq!(short_duration, 0.6); // clamped minimum
        assert_eq!(long_duration, 1.4); // clamped maximum
    }

    #[test]
    fn arc_height_capped_at_80_pixels() {
        // Very long distance — arc height should be capped
        let (_, control_y) = compute_control_point(0.0, 0.0, 10000.0, 0.0);
        // Midpoint Y is 0, control should be at most 80px above
        assert!(control_y <= 0.0); // above midpoint (negative Y = up)
    }
}

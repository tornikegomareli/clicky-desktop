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
    pub scale: f64,
}

/// Forward flight: graceful arc to target element.
const FORWARD_PIXELS_PER_SECOND: f64 = 600.0;
const FORWARD_MIN_DURATION: f64 = 0.8;
const FORWARD_MAX_DURATION: f64 = 1.8;
const FORWARD_ARC_FRACTION: f64 = 0.25;
const FORWARD_ARC_MAX: f64 = 100.0;
const FORWARD_SCALE_PULSE: f64 = 0.15;

/// Return flight: quicker, gentler arc back to mouse.
const RETURN_PIXELS_PER_SECOND: f64 = 900.0;
const RETURN_MIN_DURATION: f64 = 0.4;
const RETURN_MAX_DURATION: f64 = 1.0;
const RETURN_ARC_FRACTION: f64 = 0.15;
const RETURN_ARC_MAX: f64 = 50.0;
const RETURN_SCALE_PULSE: f64 = 0.08;

/// Computes the flight duration based on distance.
/// Forward flights are slower and more graceful; return flights are quicker.
pub fn compute_flight_duration_seconds(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    is_return: bool,
) -> f64 {
    let distance = ((end_x - start_x).powi(2) + (end_y - start_y).powi(2)).sqrt();
    if is_return {
        (distance / RETURN_PIXELS_PER_SECOND).clamp(RETURN_MIN_DURATION, RETURN_MAX_DURATION)
    } else {
        (distance / FORWARD_PIXELS_PER_SECOND).clamp(FORWARD_MIN_DURATION, FORWARD_MAX_DURATION)
    }
}

/// Computes the bezier control point — placed at the midpoint, raised
/// perpendicular to the line. Forward flights have a bigger arc; return flights gentler.
pub fn compute_control_point(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    is_return: bool,
) -> (f64, f64) {
    let midpoint_x = (start_x + end_x) / 2.0;
    let midpoint_y = (start_y + end_y) / 2.0;
    let distance = ((end_x - start_x).powi(2) + (end_y - start_y).powi(2)).sqrt();

    let (arc_fraction, arc_max) = if is_return {
        (RETURN_ARC_FRACTION, RETURN_ARC_MAX)
    } else {
        (FORWARD_ARC_FRACTION, FORWARD_ARC_MAX)
    };
    let arc_height = (distance * arc_fraction).min(arc_max);

    let direction_x = end_x - start_x;
    let direction_y = end_y - start_y;
    let direction_length = distance.max(0.001);
    let perpendicular_x = -direction_y / direction_length;
    let perpendicular_y = direction_x / direction_length;

    let control_x = midpoint_x + perpendicular_x * arc_height;
    let control_y = midpoint_y - perpendicular_y.abs() * arc_height;

    (control_x, control_y)
}

/// Smoothstep easing function: accelerates then decelerates.
fn smoothstep(linear_progress: f64) -> f64 {
    let t = linear_progress.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Evaluates the quadratic bezier curve at parameter t.
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
    let x = one_minus_t * one_minus_t * start_x + 2.0 * one_minus_t * t * control_x + t * t * end_x;
    let y = one_minus_t * one_minus_t * start_y + 2.0 * one_minus_t * t * control_y + t * t * end_y;
    (x, y)
}

/// Computes the tangent (derivative) of the quadratic bezier at parameter t.
fn evaluate_quadratic_bezier_tangent(
    t: f64,
    start_x: f64,
    start_y: f64,
    control_x: f64,
    control_y: f64,
    end_x: f64,
    end_y: f64,
) -> (f64, f64) {
    let tangent_x = 2.0 * (1.0 - t) * (control_x - start_x) + 2.0 * t * (end_x - control_x);
    let tangent_y = 2.0 * (1.0 - t) * (control_y - start_y) + 2.0 * t * (end_y - control_y);
    (tangent_x, tangent_y)
}

/// Computes a single frame of the bezier flight animation.
pub fn compute_flight_frame(
    linear_progress: f64,
    start_x: f64,
    start_y: f64,
    control_x: f64,
    control_y: f64,
    end_x: f64,
    end_y: f64,
    is_return: bool,
) -> BezierFlightFrame {
    let eased_progress = smoothstep(linear_progress);

    let (x, y) = evaluate_quadratic_bezier(
        eased_progress,
        start_x,
        start_y,
        control_x,
        control_y,
        end_x,
        end_y,
    );

    let (tangent_x, tangent_y) = evaluate_quadratic_bezier_tangent(
        eased_progress,
        start_x,
        start_y,
        control_x,
        control_y,
        end_x,
        end_y,
    );

    let rotation_radians = tangent_y.atan2(tangent_x);

    let scale_amount = if is_return {
        RETURN_SCALE_PULSE
    } else {
        FORWARD_SCALE_PULSE
    };
    let scale_pulse = (linear_progress * std::f64::consts::PI).sin();
    let scale = 1.0 + scale_pulse * scale_amount;

    BezierFlightFrame {
        x,
        y,
        rotation_radians,
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flight_starts_at_origin_and_ends_at_destination() {
        let (control_x, control_y) = compute_control_point(0.0, 0.0, 800.0, 0.0, false);

        let start_frame =
            compute_flight_frame(0.0, 0.0, 0.0, control_x, control_y, 800.0, 0.0, false);
        assert!((start_frame.x - 0.0).abs() < 0.1);
        assert!((start_frame.y - 0.0).abs() < 0.1);
        assert!((start_frame.scale - 1.0).abs() < 0.01);

        let end_frame =
            compute_flight_frame(1.0, 0.0, 0.0, control_x, control_y, 800.0, 0.0, false);
        assert!((end_frame.x - 800.0).abs() < 0.1);
        assert!((end_frame.y - 0.0).abs() < 0.1);
        assert!((end_frame.scale - 1.0).abs() < 0.01);
    }

    #[test]
    fn midpoint_has_maximum_scale_pulse() {
        let (control_x, control_y) = compute_control_point(0.0, 0.0, 800.0, 0.0, false);
        let mid_frame =
            compute_flight_frame(0.5, 0.0, 0.0, control_x, control_y, 800.0, 0.0, false);
        // At midpoint, sin(0.5 * PI) = 1.0, so scale = 1.0 + 0.15 = 1.15
        assert!((mid_frame.scale - 1.15).abs() < 0.01);
    }

    #[test]
    fn flight_duration_scales_with_distance() {
        let short_duration = compute_flight_duration_seconds(0.0, 0.0, 100.0, 0.0, false);
        let long_duration = compute_flight_duration_seconds(0.0, 0.0, 2000.0, 0.0, false);
        assert_eq!(short_duration, FORWARD_MIN_DURATION);
        assert_eq!(long_duration, FORWARD_MAX_DURATION);
    }

    #[test]
    fn return_flight_is_faster() {
        let forward = compute_flight_duration_seconds(0.0, 0.0, 600.0, 0.0, false);
        let ret = compute_flight_duration_seconds(0.0, 0.0, 600.0, 0.0, true);
        assert!(ret < forward);
    }

    #[test]
    fn arc_height_capped() {
        let (_, control_y) = compute_control_point(0.0, 0.0, 10000.0, 0.0, false);
        assert!(control_y <= 0.0);
    }

    #[test]
    fn return_arc_is_gentler() {
        let (_, forward_cy) = compute_control_point(0.0, 0.0, 800.0, 0.0, false);
        let (_, return_cy) = compute_control_point(0.0, 0.0, 800.0, 0.0, true);
        // Return arc should be less raised (closer to 0) than forward arc
        assert!(return_cy > forward_cy);
    }
}

use log::{debug, info, warn};
/// Claude Computer Use API client — detects UI element coordinates with
/// Claude's specialized pixel-counting training.
///
/// Ported from ElementLocationDetector.swift:1-335.
///
/// This makes a separate API call from the main vision streaming call.
/// The Computer Use tool definition activates Claude's specialized
/// pixel-counting training, which is significantly more accurate than
/// regular vision API coordinate extraction via POINT tags.
///
/// **Aspect ratio matching**: Instead of always resizing to 1024x768 (4:3),
/// we pick the Anthropic-recommended resolution closest to the display's
/// actual aspect ratio. This avoids distorting the image Claude sees,
/// which significantly improves X-axis coordinate accuracy.
use reqwest::Client;
use serde_json::json;

/// The model used for Computer Use element detection.
/// Same as the main vision model — claude-sonnet-4-6.
pub const COMPUTER_USE_MODEL: &str = "claude-sonnet-4-6";

/// Anthropic-recommended resolutions for Computer Use.
/// We pick the one closest to the actual display aspect ratio to avoid distortion.
/// Higher resolutions get downsampled by the API and degrade precision.
const SUPPORTED_RESOLUTIONS: [(u32, u32, f64); 3] = [
    (1024, 768, 1024.0 / 768.0), // 4:3   = 1.333 (legacy displays)
    (1280, 800, 1280.0 / 800.0), // 16:10  = 1.600 (MacBook, many laptops)
    (1366, 768, 1366.0 / 768.0), // ~16:9  = 1.779 (external monitors)
];

/// Result of a Computer Use element detection call.
#[derive(Debug, Clone)]
pub struct ComputerUseCoordinate {
    /// X coordinate in display-local logical points (not screenshot pixels)
    pub display_local_x: f64,
    /// Y coordinate in display-local logical points (top-left origin)
    pub display_local_y: f64,
}

/// Detects the screen location of a UI element using Claude's Computer Use API.
///
/// Takes the raw screenshot JPEG, resizes it to the best Computer Use resolution
/// for the display's aspect ratio, sends it to Claude with the computer tool,
/// and maps the returned coordinates back to display-local logical points.
///
/// Returns None if no element was detected (conceptual question) or on error.
pub async fn detect_element_location(
    http_client: &Client,
    api_key: &str,
    screenshot_jpeg: &[u8],
    user_question: &str,
    display_width_points: f64,
    display_height_points: f64,
) -> Option<ComputerUseCoordinate> {
    let (cu_width, cu_height) =
        best_computer_use_resolution(display_width_points, display_height_points);

    info!(
        "Computer Use: display is {:.0}x{:.0} (ratio {:.3}), using resolution {}x{}",
        display_width_points,
        display_height_points,
        display_width_points / display_height_points,
        cu_width,
        cu_height
    );

    // Resize screenshot to the Computer Use resolution
    let resized_jpeg = match resize_screenshot(screenshot_jpeg, cu_width, cu_height) {
        Some(data) => data,
        None => {
            warn!("Computer Use: failed to resize screenshot");
            return None;
        }
    };

    // Make the API call
    let raw_coordinate = call_computer_use_api(
        http_client,
        api_key,
        &resized_jpeg,
        user_question,
        cu_width,
        cu_height,
    )
    .await?;

    // Clamp coordinates to valid range — Claude occasionally returns
    // values slightly outside the declared display dimensions
    let clamped_x = raw_coordinate.0.clamp(0.0, cu_width as f64);
    let clamped_y = raw_coordinate.1.clamp(0.0, cu_height as f64);

    // Scale from Computer Use resolution back to display logical points
    let scaled_x = (clamped_x / cu_width as f64) * display_width_points;
    let scaled_y = (clamped_y / cu_height as f64) * display_height_points;

    info!("Computer Use: mapped ({:.0}, {:.0}) in {}x{} → ({:.1}, {:.1}) in {:.0}x{:.0} display coords",
        clamped_x, clamped_y, cu_width, cu_height,
        scaled_x, scaled_y, display_width_points, display_height_points);

    Some(ComputerUseCoordinate {
        display_local_x: scaled_x,
        display_local_y: scaled_y,
    })
}

/// Picks the Anthropic-recommended Computer Use resolution whose aspect ratio
/// is closest to the actual display, minimizing image distortion.
fn best_computer_use_resolution(display_width: f64, display_height: f64) -> (u32, u32) {
    let display_aspect_ratio = display_width / display_height.max(1.0);

    let mut best_width = 1280u32;
    let mut best_height = 800u32;
    let mut smallest_difference = f64::MAX;

    for &(width, height, aspect_ratio) in &SUPPORTED_RESOLUTIONS {
        let difference = (display_aspect_ratio - aspect_ratio).abs();
        if difference < smallest_difference {
            smallest_difference = difference;
            best_width = width;
            best_height = height;
        }
    }

    (best_width, best_height)
}

/// Calls the Claude Computer Use API with a resized screenshot.
/// Returns the raw (x, y) coordinate in Computer Use resolution space, or None.
async fn call_computer_use_api(
    http_client: &Client,
    api_key: &str,
    resized_jpeg: &[u8],
    user_question: &str,
    declared_width: u32,
    declared_height: u32,
) -> Option<(f64, f64)> {
    let base64_screenshot =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, resized_jpeg);

    let user_prompt = format!(
        "The user asked this question while looking at their screen: \"{}\"\n\n\
         Look at the screenshot. If there is a specific UI element (button, link, \
         menu item, text field, icon, etc.) that the user should interact with or \
         is asking about, click on that element.\n\n\
         If the question is purely conceptual (e.g., \"what does HTML mean?\") and \
         there's no specific element to point to, just respond with text saying \
         \"no specific element\".",
        user_question,
    );

    let request_body = json!({
        "model": COMPUTER_USE_MODEL,
        "max_tokens": 256,
        "tools": [
            {
                "type": "computer_20251124",
                "name": "computer",
                "display_width_px": declared_width,
                "display_height_px": declared_height,
            }
        ],
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/jpeg",
                            "data": base64_screenshot,
                        }
                    },
                    {
                        "type": "text",
                        "text": user_prompt,
                    }
                ]
            }
        ]
    });

    let payload_mb = serde_json::to_vec(&request_body)
        .map(|v| v.len())
        .unwrap_or(0) as f64
        / 1_048_576.0;
    debug!(
        "Computer Use: sending {:.1}MB request (declared {}x{})",
        payload_mb, declared_width, declared_height
    );

    let response = http_client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "computer-use-2025-11-24")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            warn!("Computer Use: request failed: {}", e);
            return None;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        warn!(
            "Computer Use: API error {}: {}",
            status,
            &error_body[..error_body.len().min(200)]
        );
        return None;
    }

    let body: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(e) => {
            warn!("Computer Use: failed to parse response: {}", e);
            return None;
        }
    };

    parse_coordinate_from_response(&body)
}

/// Parses the Computer Use API response to extract click coordinates.
/// Claude returns a `tool_use` content block with `{"action": "left_click", "coordinate": [x, y]}`.
fn parse_coordinate_from_response(response: &serde_json::Value) -> Option<(f64, f64)> {
    let content_blocks = response["content"].as_array()?;

    for block in content_blocks {
        if block["type"].as_str() != Some("tool_use") {
            continue;
        }

        let input = &block["input"];
        let coordinate = input["coordinate"].as_array()?;
        if coordinate.len() != 2 {
            continue;
        }

        let x = coordinate[0].as_f64()?;
        let y = coordinate[1].as_f64()?;
        info!("Computer Use: raw coordinate ({:.0}, {:.0})", x, y);
        return Some((x, y));
    }

    // No tool_use block — Claude responded with text (no element to point at)
    info!("Computer Use: no specific element detected (conceptual question)");
    None
}

/// Resizes screenshot JPEG data to the specified Computer Use resolution.
/// Returns JPEG bytes at the target resolution, or None on failure.
fn resize_screenshot(jpeg_data: &[u8], target_width: u32, target_height: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory(jpeg_data).ok()?.to_rgba8();

    let resized = image::imageops::resize(
        &img,
        target_width,
        target_height,
        image::imageops::FilterType::Lanczos3,
    );

    let mut jpeg_buf = Vec::new();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::Cursor::new(&mut jpeg_buf), 85);
    encoder.encode_image(&resized).ok()?;

    Some(jpeg_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_resolution_for_16_10_display() {
        // Typical MacBook / many laptops
        let (w, h) = best_computer_use_resolution(2560.0, 1600.0);
        assert_eq!((w, h), (1280, 800));
    }

    #[test]
    fn best_resolution_for_16_9_display() {
        // Typical external monitor
        let (w, h) = best_computer_use_resolution(1920.0, 1080.0);
        assert_eq!((w, h), (1366, 768));
    }

    #[test]
    fn best_resolution_for_4_3_display() {
        let (w, h) = best_computer_use_resolution(1024.0, 768.0);
        assert_eq!((w, h), (1024, 768));
    }

    #[test]
    fn parse_tool_use_response() {
        let response = serde_json::json!({
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "name": "computer",
                    "input": {
                        "action": "left_click",
                        "coordinate": [640, 400]
                    }
                }
            ]
        });
        let coord = parse_coordinate_from_response(&response).unwrap();
        assert_eq!(coord, (640.0, 400.0));
    }

    #[test]
    fn parse_text_only_response() {
        let response = serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "no specific element"
                }
            ]
        });
        assert!(parse_coordinate_from_response(&response).is_none());
    }
}

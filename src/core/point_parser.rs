use regex::Regex;
use std::sync::LazyLock;

/// Parsed result from Claude's response containing both the spoken text
/// and an optional pointing coordinate.
#[derive(Debug, Clone)]
pub struct PointingParseResult {
    /// The text Claude wants spoken aloud (with the POINT tag stripped)
    pub spoken_text: String,

    /// The pointing instruction, if Claude wants the cursor to fly somewhere.
    /// None if Claude returned [POINT:none].
    pub pointing: Option<ParsedPointingCoordinate>,
}

/// A coordinate extracted from a [POINT:x,y:label:screenN] tag.
/// Coordinates are in screenshot pixel space — they must be mapped
/// to display coordinates before use.
#[derive(Debug, Clone)]
pub struct ParsedPointingCoordinate {
    /// X pixel coordinate in the screenshot image
    pub screenshot_pixel_x: f64,

    /// Y pixel coordinate in the screenshot image
    pub screenshot_pixel_y: f64,

    /// Short label describing the UI element (e.g. "save button", "search bar")
    pub element_label: String,

    /// Which screen the element is on (1-indexed). None means the cursor's screen.
    pub screen_number: Option<u32>,
}

// Regex ported from CompanionManager.swift:786
// Matches: [POINT:x,y:label] or [POINT:x,y:label:screenN] or [POINT:none]
// The tag must appear at the end of the response text.
static POINT_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[POINT:(?:none|(\d+)\s*,\s*(\d+)(?::([^\]:\s][^\]:]*?))?(?::screen(\d+))?)\]\s*$")
        .expect("POINT tag regex must compile")
});

/// Parses Claude's response text to extract the spoken portion and any
/// pointing coordinate tag.
///
/// The [POINT:...] tag always appears at the very end of the response.
/// This function strips it and returns the clean spoken text plus the
/// parsed coordinate (if any).
///
/// # Examples
///
/// ```
/// use clicky_desktop::core::point_parser::parse_claude_response;
///
/// let result = parse_claude_response(
///     "click that save button in the top right [POINT:1100,42:save button]"
/// );
/// assert_eq!(result.spoken_text, "click that save button in the top right");
/// let pointing = result.pointing.unwrap();
/// assert_eq!(pointing.screenshot_pixel_x, 1100.0);
/// assert_eq!(pointing.screenshot_pixel_y, 42.0);
/// assert_eq!(pointing.element_label, "save button");
/// ```
pub fn parse_claude_response(full_response_text: &str) -> PointingParseResult {
    let Some(regex_match) = POINT_TAG_REGEX.find(full_response_text) else {
        // No POINT tag found — entire text is spoken
        return PointingParseResult {
            spoken_text: full_response_text.trim().to_string(),
            pointing: None,
        };
    };

    // Strip the POINT tag to get the spoken text
    let spoken_text = full_response_text[..regex_match.start()].trim().to_string();

    // Check if it's [POINT:none] — the regex captures nothing for "none"
    let captures = POINT_TAG_REGEX.captures(full_response_text).unwrap();
    let pointing = captures.get(1).map(|x_match| {
        let screenshot_pixel_x = x_match.as_str().parse::<f64>().unwrap_or(0.0);
        let screenshot_pixel_y = captures
            .get(2)
            .map(|m| m.as_str().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0);
        let element_label = captures
            .get(3)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let screen_number = captures.get(4).and_then(|m| m.as_str().parse::<u32>().ok());

        ParsedPointingCoordinate {
            screenshot_pixel_x,
            screenshot_pixel_y,
            element_label,
            screen_number,
        }
    });

    PointingParseResult {
        spoken_text,
        pointing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_point_with_label() {
        let result = parse_claude_response("click the save button [POINT:1100,42:save button]");
        assert_eq!(result.spoken_text, "click the save button");
        let pointing = result.pointing.unwrap();
        assert_eq!(pointing.screenshot_pixel_x, 1100.0);
        assert_eq!(pointing.screenshot_pixel_y, 42.0);
        assert_eq!(pointing.element_label, "save button");
        assert_eq!(pointing.screen_number, None);
    }

    #[test]
    fn parse_point_with_screen_number() {
        let result =
            parse_claude_response("that's on your other monitor [POINT:400,300:terminal:screen2]");
        assert_eq!(result.spoken_text, "that's on your other monitor");
        let pointing = result.pointing.unwrap();
        assert_eq!(pointing.screenshot_pixel_x, 400.0);
        assert_eq!(pointing.screenshot_pixel_y, 300.0);
        assert_eq!(pointing.element_label, "terminal");
        assert_eq!(pointing.screen_number, Some(2));
    }

    #[test]
    fn parse_point_none() {
        let result = parse_claude_response("html is a markup language [POINT:none]");
        assert_eq!(result.spoken_text, "html is a markup language");
        assert!(result.pointing.is_none());
    }

    #[test]
    fn parse_no_point_tag() {
        let result = parse_claude_response("just a regular response with no pointing");
        assert_eq!(
            result.spoken_text,
            "just a regular response with no pointing"
        );
        assert!(result.pointing.is_none());
    }

    #[test]
    fn parse_point_with_trailing_whitespace() {
        let result = parse_claude_response("check this out [POINT:500,200:menu bar]  ");
        assert_eq!(result.spoken_text, "check this out");
        let pointing = result.pointing.unwrap();
        assert_eq!(pointing.screenshot_pixel_x, 500.0);
        assert_eq!(pointing.element_label, "menu bar");
    }
}

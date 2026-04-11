use regex::Regex;
use std::sync::LazyLock;

/// A screen annotation that Claude wants drawn on the overlay.
/// Coordinates are in screenshot pixel space — they must be mapped
/// to display coordinates before rendering.
#[derive(Debug, Clone)]
pub enum ScreenAnnotation {
    Highlight {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        label: String,
    },
    Circle {
        x: f64,
        y: f64,
        radius: f64,
        label: String,
    },
    Arrow {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        label: String,
    },
}

/// Parsed result from Claude's response containing both the spoken text,
/// an optional pointing coordinate, and any screen annotations.
#[derive(Debug, Clone)]
pub struct PointingParseResult {
    /// The text Claude wants spoken aloud (with all tags stripped)
    pub spoken_text: String,

    /// The pointing instruction, if Claude wants the cursor to fly somewhere.
    /// None if Claude returned [POINT:none].
    pub pointing: Option<ParsedPointingCoordinate>,

    /// Screen annotations to draw on the overlay (highlights, circles, arrows).
    pub annotations: Vec<ScreenAnnotation>,
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
// The tag must appear at the end of the response text (after any annotation tags).
static POINT_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[POINT:(?:none|(\d+)\s*,\s*(\d+)(?::([^\]:\s][^\]:]*?))?(?::screen(\d+))?)\]")
        .expect("POINT tag regex must compile")
});

// Annotation tag regexes — can appear multiple times anywhere in the response.
static HIGHLIGHT_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[HIGHLIGHT:(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+):([^\]]+)\]")
        .expect("HIGHLIGHT tag regex must compile")
});

static CIRCLE_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[CIRCLE:(\d+)\s*,\s*(\d+)\s*,\s*(\d+):([^\]]+)\]")
        .expect("CIRCLE tag regex must compile")
});

static ARROW_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[ARROW:(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+):([^\]]+)\]")
        .expect("ARROW tag regex must compile")
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
    let mut text = full_response_text.to_string();

    // Parse annotation tags first (can appear multiple times)
    let mut annotations = Vec::new();

    // Collect all annotation matches, then strip them from text
    for caps in HIGHLIGHT_TAG_REGEX.captures_iter(full_response_text) {
        annotations.push(ScreenAnnotation::Highlight {
            x1: caps[1].parse().unwrap_or(0.0),
            y1: caps[2].parse().unwrap_or(0.0),
            x2: caps[3].parse().unwrap_or(0.0),
            y2: caps[4].parse().unwrap_or(0.0),
            label: caps[5].trim().to_string(),
        });
    }
    text = HIGHLIGHT_TAG_REGEX.replace_all(&text, "").to_string();

    for caps in CIRCLE_TAG_REGEX.captures_iter(&text.clone()) {
        annotations.push(ScreenAnnotation::Circle {
            x: caps[1].parse().unwrap_or(0.0),
            y: caps[2].parse().unwrap_or(0.0),
            radius: caps[3].parse().unwrap_or(0.0),
            label: caps[4].trim().to_string(),
        });
    }
    text = CIRCLE_TAG_REGEX.replace_all(&text, "").to_string();

    for caps in ARROW_TAG_REGEX.captures_iter(&text.clone()) {
        annotations.push(ScreenAnnotation::Arrow {
            x1: caps[1].parse().unwrap_or(0.0),
            y1: caps[2].parse().unwrap_or(0.0),
            x2: caps[3].parse().unwrap_or(0.0),
            y2: caps[4].parse().unwrap_or(0.0),
            label: caps[5].trim().to_string(),
        });
    }
    text = ARROW_TAG_REGEX.replace_all(&text, "").to_string();

    // Parse POINT tag (same as before, but on annotation-stripped text)
    let pointing = POINT_TAG_REGEX.captures(&text).and_then(|captures| {
        captures.get(1).map(|x_match| {
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
        })
    });

    // Strip POINT tag from spoken text
    let spoken_text = POINT_TAG_REGEX.replace(&text, "").trim().to_string();

    PointingParseResult {
        spoken_text,
        pointing,
        annotations,
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
        assert!(result.annotations.is_empty());
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

    #[test]
    fn parse_highlight_annotation() {
        let result = parse_claude_response(
            "look at this panel [HIGHLIGHT:100,200,500,400:settings panel]",
        );
        assert_eq!(result.spoken_text, "look at this panel");
        assert!(result.pointing.is_none());
        assert_eq!(result.annotations.len(), 1);
        match &result.annotations[0] {
            ScreenAnnotation::Highlight {
                x1,
                y1,
                x2,
                y2,
                label,
            } => {
                assert_eq!(*x1, 100.0);
                assert_eq!(*y1, 200.0);
                assert_eq!(*x2, 500.0);
                assert_eq!(*y2, 400.0);
                assert_eq!(label, "settings panel");
            }
            _ => panic!("Expected Highlight annotation"),
        }
    }

    #[test]
    fn parse_circle_annotation() {
        let result =
            parse_claude_response("notice this button [CIRCLE:300,150,40:save button]");
        assert_eq!(result.spoken_text, "notice this button");
        assert_eq!(result.annotations.len(), 1);
        match &result.annotations[0] {
            ScreenAnnotation::Circle {
                x,
                y,
                radius,
                label,
            } => {
                assert_eq!(*x, 300.0);
                assert_eq!(*y, 150.0);
                assert_eq!(*radius, 40.0);
                assert_eq!(label, "save button");
            }
            _ => panic!("Expected Circle annotation"),
        }
    }

    #[test]
    fn parse_arrow_annotation() {
        let result = parse_claude_response(
            "drag from here to there [ARROW:200,400,800,300:drag here]",
        );
        assert_eq!(result.spoken_text, "drag from here to there");
        assert_eq!(result.annotations.len(), 1);
        match &result.annotations[0] {
            ScreenAnnotation::Arrow {
                x1,
                y1,
                x2,
                y2,
                label,
            } => {
                assert_eq!(*x1, 200.0);
                assert_eq!(*y1, 400.0);
                assert_eq!(*x2, 800.0);
                assert_eq!(*y2, 300.0);
                assert_eq!(label, "drag here");
            }
            _ => panic!("Expected Arrow annotation"),
        }
    }

    #[test]
    fn parse_multiple_annotations_with_point() {
        let result = parse_claude_response(
            "grab it from the sidebar and drop it in the main panel. [ARROW:200,400,800,300:drag here] [HIGHLIGHT:750,250,950,350:drop target] [POINT:200,400:file icon]",
        );
        assert_eq!(
            result.spoken_text,
            "grab it from the sidebar and drop it in the main panel."
        );
        assert!(result.pointing.is_some());
        let pointing = result.pointing.unwrap();
        assert_eq!(pointing.screenshot_pixel_x, 200.0);
        assert_eq!(pointing.screenshot_pixel_y, 400.0);
        assert_eq!(pointing.element_label, "file icon");
        assert_eq!(result.annotations.len(), 2);
    }

    #[test]
    fn parse_annotations_without_point() {
        let result = parse_claude_response(
            "these two areas are related [HIGHLIGHT:100,100,300,200:source] [HIGHLIGHT:500,100,700,200:destination]",
        );
        assert_eq!(result.spoken_text, "these two areas are related");
        assert!(result.pointing.is_none());
        assert_eq!(result.annotations.len(), 2);
    }
}

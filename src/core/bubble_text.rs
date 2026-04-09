/// Generates a short friendly phrase for the speech bubble.
/// The full LLM response is spoken via TTS — the bubble just shows
/// a brief, warm pointer phrase.

use std::sync::atomic::{AtomicUsize, Ordering};

static PHRASE_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Friendly phrases shown in the bubble when the cursor points at an element.
const POINTING_PHRASES: &[&str] = &[
    "right here!",
    "it's here!",
    "look, it's this one",
    "here it is!",
    "this one right here",
    "check this out",
    "see this?",
    "found it!",
    "over here!",
    "this is the one",
];

/// Phrases when there's no specific element to point at.
const GENERAL_PHRASES: &[&str] = &[
    "hmm, let me think...",
    "here's what I think",
    "so basically...",
    "good question!",
];

/// Returns a short friendly phrase for the speech bubble.
/// Cycles through curated phrases to avoid repetition.
pub fn pick_bubble_phrase(is_pointing: bool) -> String {
    let phrases = if is_pointing { POINTING_PHRASES } else { GENERAL_PHRASES };
    let index = PHRASE_INDEX.fetch_add(1, Ordering::Relaxed) % phrases.len();
    phrases[index].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointing_phrase_not_empty() {
        let phrase = pick_bubble_phrase(true);
        assert!(!phrase.is_empty());
    }

    #[test]
    fn general_phrase_not_empty() {
        let phrase = pick_bubble_phrase(false);
        assert!(!phrase.is_empty());
    }

    #[test]
    fn cycles_through_phrases() {
        let p1 = pick_bubble_phrase(true);
        let p2 = pick_bubble_phrase(true);
        // They should be different (cycling)
        assert_ne!(p1, p2);
    }
}

/// Voice interaction state — mirrors CompanionVoiceState from CompanionManager.swift:17-22.
///
/// The state machine drives the entire UI: what the overlay shows (waveform, spinner,
/// cursor triangle), whether audio is being captured, and when API calls happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    /// Default state. Blue cursor triangle is visible and follows the mouse.
    Idle,

    /// User is holding the push-to-talk hotkey. Waveform is shown in the overlay,
    /// microphone is capturing, and audio is streaming to the transcription provider.
    Listening,

    /// User released the hotkey. Spinner is shown. Waiting for transcription
    /// finalization, then screenshot capture, then Claude API response.
    Processing,

    /// Claude's response has arrived and TTS audio is playing. If a POINT tag
    /// was included, the cursor is flying to or pointing at the target element.
    Responding,
}

/// Events that trigger state transitions. These are sent via channels from
/// the hotkey monitor, audio pipeline, API clients, and TTS playback.
#[derive(Debug, Clone)]
pub enum VoiceStateTransition {
    /// User pressed the push-to-talk hotkey
    HotkeyPressed,

    /// User released the push-to-talk hotkey
    HotkeyReleased,

    /// Transcription + screenshot + Claude response complete, TTS starting
    ResponseReady {
        response_text: String,
        pointing_instruction: Option<PointingInstruction>,
    },

    /// TTS audio finished playing (and pointing animation completed)
    ResponseComplete,

    /// An error occurred — return to idle
    Error(String),
}

/// Parsed from Claude's [POINT:x,y:label:screenN] tag.
#[derive(Debug, Clone)]
pub struct PointingInstruction {
    pub screenshot_x: f64,
    pub screenshot_y: f64,
    pub label: String,
    pub screen_number: Option<u32>,
}

impl VoiceState {
    /// Applies a transition and returns the new state, or None if the
    /// transition is invalid for the current state.
    ///
    /// State machine rules (from CompanionManager.swift:430-461):
    ///   Idle → Listening (on hotkey press)
    ///   Listening → Processing (on hotkey release)
    ///   Processing → Responding (on response ready)
    ///   Responding → Idle (on response complete)
    ///   Any → Idle (on error)
    ///   Listening/Processing/Responding → Listening (on hotkey press — cancel and restart)
    pub fn apply(&self, transition: VoiceStateTransition) -> Option<VoiceState> {
        match (self, &transition) {
            // Normal flow
            (VoiceState::Idle, VoiceStateTransition::HotkeyPressed) => {
                Some(VoiceState::Listening)
            }
            (VoiceState::Listening, VoiceStateTransition::HotkeyReleased) => {
                Some(VoiceState::Processing)
            }
            (VoiceState::Processing, VoiceStateTransition::ResponseReady { .. }) => {
                Some(VoiceState::Responding)
            }
            (VoiceState::Responding, VoiceStateTransition::ResponseComplete) => {
                Some(VoiceState::Idle)
            }

            // Cancel current interaction and restart — user pressed hotkey again
            // while already in a session (matches CompanionManager.swift:475-480)
            (VoiceState::Listening, VoiceStateTransition::HotkeyPressed)
            | (VoiceState::Processing, VoiceStateTransition::HotkeyPressed)
            | (VoiceState::Responding, VoiceStateTransition::HotkeyPressed) => {
                Some(VoiceState::Listening)
            }

            // Error from any state returns to idle
            (_, VoiceStateTransition::Error(_)) => Some(VoiceState::Idle),

            // All other transitions are invalid — ignore them
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_voice_interaction_flow() {
        let mut state = VoiceState::Idle;

        // Press hotkey → listening
        state = state.apply(VoiceStateTransition::HotkeyPressed).unwrap();
        assert_eq!(state, VoiceState::Listening);

        // Release hotkey → processing
        state = state.apply(VoiceStateTransition::HotkeyReleased).unwrap();
        assert_eq!(state, VoiceState::Processing);

        // Response ready → responding
        state = state
            .apply(VoiceStateTransition::ResponseReady {
                response_text: "test response".to_string(),
                pointing_instruction: None,
            })
            .unwrap();
        assert_eq!(state, VoiceState::Responding);

        // Response complete → idle
        state = state.apply(VoiceStateTransition::ResponseComplete).unwrap();
        assert_eq!(state, VoiceState::Idle);
    }

    #[test]
    fn cancel_and_restart_from_processing() {
        let mut state = VoiceState::Processing;

        // Press hotkey while processing → restarts to listening
        state = state.apply(VoiceStateTransition::HotkeyPressed).unwrap();
        assert_eq!(state, VoiceState::Listening);
    }

    #[test]
    fn error_returns_to_idle() {
        let state = VoiceState::Responding;
        let new_state = state
            .apply(VoiceStateTransition::Error("test error".to_string()))
            .unwrap();
        assert_eq!(new_state, VoiceState::Idle);
    }

    #[test]
    fn invalid_transition_returns_none() {
        let state = VoiceState::Idle;
        // Can't release hotkey from idle
        assert!(state.apply(VoiceStateTransition::HotkeyReleased).is_none());
    }
}

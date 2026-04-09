pub mod capture;
pub mod playback;

use std::fmt;
use crate::app::state_machine::PointingInstruction;
use crate::core::coordinate_mapper::DisplayInfo;

/// Events sent from async background tasks to the synchronous render loop.
pub enum UiEvent {
    PartialTranscript(String),
    FinalTranscript(String),
    TranscriptionError(String),
    LlmResponse {
        spoken_text: String,
        pointing_instruction: Option<PointingInstruction>,
        display_infos: Vec<DisplayInfo>,
        /// Pre-computed global coordinate from Computer Use API (more precise than POINT tags).
        /// When present, this takes priority over the POINT tag coordinate.
        computer_use_global_coordinate: Option<(f64, f64)>,
    },
    PipelineError(String),
}

/// Audio subsystem errors. All are recoverable — the app continues without audio.
#[derive(Debug)]
pub enum AudioError {
    NoDevice(String),
    StreamError(String),
    PlaybackError(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NoDevice(msg) => write!(f, "No audio device: {}", msg),
            AudioError::StreamError(msg) => write!(f, "Audio stream error: {}", msg),
            AudioError::PlaybackError(msg) => write!(f, "Playback error: {}", msg),
        }
    }
}

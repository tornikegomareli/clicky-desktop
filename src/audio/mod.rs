pub mod capture;
pub mod playback;

use std::fmt;

/// Events sent from async background tasks to the synchronous render loop.
#[derive(Debug)]
pub enum UiEvent {
    PartialTranscript(String),
    FinalTranscript(String),
    TranscriptionError(String),
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

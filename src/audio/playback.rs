use super::AudioError;
/// MP3 playback using rodio.
/// Plays TTS audio from memory (bytes received from ElevenLabs API).
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::io::Cursor;

/// Manages audio output for TTS playback.
pub struct AudioPlayer {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Option<Sink>,
}

impl AudioPlayer {
    /// Creates a new audio player using the default output device.
    pub fn new() -> Result<Self, AudioError> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| AudioError::PlaybackError(format!("No output device: {}", e)))?;

        Ok(Self {
            _stream,
            stream_handle,
            sink: None,
        })
    }

    /// Plays MP3 audio from a byte buffer.
    pub fn play_mp3(&mut self, mp3_bytes: Vec<u8>) {
        let cursor = Cursor::new(mp3_bytes);
        let source = match Decoder::new(cursor) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to decode MP3: {}", e);
                return;
            }
        };

        let sink = match Sink::try_new(&self.stream_handle) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to create audio sink: {}", e);
                return;
            }
        };

        sink.append(source);
        self.sink = Some(sink);
    }

    /// Returns true if audio is currently playing.
    pub fn is_playing(&self) -> bool {
        self.sink.as_ref().map_or(false, |s| !s.empty())
    }

    /// Stops any currently playing audio.
    pub fn stop(&mut self) {
        self.sink = None;
    }
}

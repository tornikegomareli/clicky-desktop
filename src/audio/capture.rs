/// Microphone capture using cpal.
/// Opens the default input device, streams f32 samples, feeds the RMS tracker
/// for waveform visualization, and sends audio chunks for transcription.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::core::audio_rms::AudioPowerLevelTracker;
use super::AudioError;

/// A chunk of raw audio from the microphone.
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Manages microphone capture lifecycle.
pub struct MicCapture {
    rms_tracker: Arc<Mutex<AudioPowerLevelTracker>>,
    stream: Option<Stream>,
}

impl MicCapture {
    pub fn new(rms_tracker: Arc<Mutex<AudioPowerLevelTracker>>) -> Self {
        Self {
            rms_tracker,
            stream: None,
        }
    }

    /// Opens the default microphone and begins capturing audio.
    /// Returns a receiver for audio chunks (consumed by the transcription bridge).
    pub fn start(&mut self) -> Result<mpsc::Receiver<AudioChunk>, AudioError> {
        let host = cpal::default_host();

        let device = host.default_input_device()
            .ok_or_else(|| AudioError::NoDevice("No default input device found".into()))?;

        let config = device.default_input_config()
            .map_err(|e| AudioError::StreamError(format!("Failed to get input config: {}", e)))?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        log::info!(
            "Mic opened: {} ({}Hz, {} ch, {:?})",
            device.name().unwrap_or_else(|_| "unknown".into()),
            sample_rate, channels, config.sample_format()
        );

        let (tx, rx) = mpsc::channel::<AudioChunk>();
        let rms = Arc::clone(&self.rms_tracker);

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Feed RMS tracker for waveform visualization
                if let Ok(mut tracker) = rms.lock() {
                    tracker.update_with_samples(data);
                }

                // Send chunk to transcription bridge (drop if receiver is gone)
                let _ = tx.send(AudioChunk {
                    samples: data.to_vec(),
                    sample_rate,
                    channels,
                });
            },
            |err| {
                log::error!("Mic stream error: {}", err);
            },
            None, // default latency
        ).map_err(|e| AudioError::StreamError(format!("Failed to build input stream: {}", e)))?;

        stream.play()
            .map_err(|e| AudioError::StreamError(format!("Failed to start stream: {}", e)))?;

        self.stream = Some(stream);
        Ok(rx)
    }

    /// Stops microphone capture and resets the RMS tracker.
    pub fn stop(&mut self) {
        self.stream = None; // dropping the stream stops capture
        if let Ok(mut tracker) = self.rms_tracker.lock() {
            tracker.reset();
        }
        log::info!("Mic stopped");
    }
}

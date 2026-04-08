/// Real-time audio power level calculator for waveform visualization.
/// Ported from BuddyDictationManager.swift:687-720.
///
/// Computes the RMS (root mean square) of audio samples, applies a boost
/// factor, and smooths the result with exponential decay for visually
/// pleasing waveform animation.

/// Constants matching the macOS implementation.
const RMS_BOOST_FACTOR: f64 = 10.2;
const SMOOTHING_DECAY_FACTOR: f64 = 0.72;
const BASELINE_POWER_LEVEL: f64 = 0.02;

/// Tracks audio power level with smoothing for waveform visualization.
///
/// Each call to `update_with_samples` computes the RMS power of the audio
/// buffer, boosts it, and blends with the previous level using exponential
/// decay. This prevents the waveform from flickering and creates smooth
/// amplitude transitions.
pub struct AudioPowerLevelTracker {
    /// Current smoothed power level, range 0.0–1.0
    current_power_level: f64,

    /// Ring buffer of recent power levels for waveform bars.
    /// Length matches the macOS implementation: 44 samples at ~70ms intervals.
    power_history: Vec<f64>,

    /// Maximum entries in the power history ring buffer
    max_history_length: usize,
}

impl AudioPowerLevelTracker {
    pub fn new() -> Self {
        let max_history_length = 44;
        Self {
            current_power_level: BASELINE_POWER_LEVEL,
            power_history: vec![BASELINE_POWER_LEVEL; max_history_length],
            max_history_length,
        }
    }

    /// Updates the power level from a buffer of audio samples (f32, -1.0 to 1.0).
    ///
    /// The algorithm:
    /// 1. Compute RMS: sqrt(sum_of_squares / sample_count)
    /// 2. Boost: rms * 10.2, clamped to [0.0, 1.0]
    /// 3. Smooth: max(boosted, previous_level * 0.72)
    ///
    /// This matches BuddyDictationManager.swift:687-720.
    pub fn update_with_samples(&mut self, audio_samples: &[f32]) {
        if audio_samples.is_empty() {
            self.apply_decay();
            return;
        }

        // RMS calculation
        let sum_of_squares: f64 = audio_samples
            .iter()
            .map(|&sample| (sample as f64) * (sample as f64))
            .sum();
        let rms = (sum_of_squares / audio_samples.len() as f64).sqrt();

        // Boost and clamp
        let boosted_level = (rms * RMS_BOOST_FACTOR).clamp(0.0, 1.0);

        // Smooth with exponential decay — never drop below baseline or decayed previous
        let decayed_previous = self.current_power_level * SMOOTHING_DECAY_FACTOR;
        self.current_power_level = boosted_level
            .max(decayed_previous)
            .max(BASELINE_POWER_LEVEL);

        // Push to history ring buffer
        self.push_to_history(self.current_power_level);
    }

    /// Returns the current smoothed power level (0.0–1.0).
    pub fn current_level(&self) -> f64 {
        self.current_power_level
    }

    /// Returns the power history for waveform rendering.
    /// Most recent sample is at the end.
    pub fn history(&self) -> &[f64] {
        &self.power_history
    }

    /// Resets the power level to baseline (for when recording stops).
    pub fn reset(&mut self) {
        self.current_power_level = BASELINE_POWER_LEVEL;
        self.power_history.fill(BASELINE_POWER_LEVEL);
    }

    fn apply_decay(&mut self) {
        self.current_power_level *= SMOOTHING_DECAY_FACTOR;
        if self.current_power_level < BASELINE_POWER_LEVEL {
            self.current_power_level = BASELINE_POWER_LEVEL;
        }
        self.push_to_history(self.current_power_level);
    }

    fn push_to_history(&mut self, level: f64) {
        if self.power_history.len() >= self.max_history_length {
            self.power_history.remove(0);
        }
        self.power_history.push(level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_stays_at_baseline() {
        let mut tracker = AudioPowerLevelTracker::new();
        let silence = vec![0.0f32; 1024];
        tracker.update_with_samples(&silence);
        // RMS of silence is 0, but decay keeps it above baseline
        assert!(tracker.current_level() >= BASELINE_POWER_LEVEL);
    }

    #[test]
    fn loud_signal_produces_high_level() {
        let mut tracker = AudioPowerLevelTracker::new();
        let loud_signal = vec![0.5f32; 1024];
        tracker.update_with_samples(&loud_signal);
        assert!(tracker.current_level() > 0.5);
    }

    #[test]
    fn level_decays_after_silence() {
        let mut tracker = AudioPowerLevelTracker::new();
        let loud_signal = vec![0.5f32; 1024];
        tracker.update_with_samples(&loud_signal);
        let loud_level = tracker.current_level();

        let silence = vec![0.0f32; 1024];
        tracker.update_with_samples(&silence);
        assert!(tracker.current_level() < loud_level);
    }

    #[test]
    fn history_has_correct_length() {
        let tracker = AudioPowerLevelTracker::new();
        assert_eq!(tracker.history().len(), 44);
    }

    #[test]
    fn reset_clears_to_baseline() {
        let mut tracker = AudioPowerLevelTracker::new();
        let loud_signal = vec![0.8f32; 1024];
        tracker.update_with_samples(&loud_signal);
        tracker.reset();
        assert!((tracker.current_level() - BASELINE_POWER_LEVEL).abs() < f64::EPSILON);
    }
}

/// PCM16 audio format converter — resamples and converts audio to 16kHz mono PCM16.
/// Ported from BuddyPCM16AudioConverter in BuddyAudioConversionSupport.swift:11-70.
///
/// Microphone input typically arrives at 44100Hz or 48000Hz float32.
/// The transcription provider requires 16000Hz signed 16-bit
/// little-endian mono PCM. This module handles the conversion.

/// Target sample rate for all transcription providers.
pub const TARGET_SAMPLE_RATE: u32 = 16000;

/// Converts float32 audio samples at an arbitrary sample rate to PCM16 mono
/// at the target sample rate (16kHz).
///
/// Uses simple linear interpolation for resampling. For production quality,
/// consider using the `rubato` crate for sinc-based resampling, but linear
/// interpolation is sufficient for speech transcription.
///
/// # Arguments
/// * `input_samples` - Float32 samples in range [-1.0, 1.0]
/// * `input_sample_rate` - Source sample rate (e.g. 44100 or 48000)
/// * `input_channels` - Number of channels in the input (1 = mono, 2 = stereo)
///
/// # Returns
/// Raw PCM16 bytes (little-endian, mono, 16kHz) suitable for streaming
/// to AssemblyAI.
pub fn convert_float32_to_pcm16_mono(
    input_samples: &[f32],
    input_sample_rate: u32,
    input_channels: u16,
) -> Vec<u8> {
    // Step 1: Convert stereo to mono by averaging channels
    let mono_samples = if input_channels > 1 {
        downmix_to_mono(input_samples, input_channels)
    } else {
        input_samples.to_vec()
    };

    // Step 2: Resample to target rate using linear interpolation
    let resampled = if input_sample_rate != TARGET_SAMPLE_RATE {
        resample_linear(&mono_samples, input_sample_rate, TARGET_SAMPLE_RATE)
    } else {
        mono_samples
    };

    // Step 3: Convert float32 [-1.0, 1.0] to signed 16-bit little-endian
    let mut pcm16_bytes = Vec::with_capacity(resampled.len() * 2);
    for sample in &resampled {
        let clamped = sample.clamp(-1.0, 1.0);
        let scaled = (clamped * i16::MAX as f32) as i16;
        pcm16_bytes.extend_from_slice(&scaled.to_le_bytes());
    }

    pcm16_bytes
}

/// Downmixes multi-channel audio to mono by averaging all channels per frame.
fn downmix_to_mono(interleaved_samples: &[f32], channel_count: u16) -> Vec<f32> {
    let channel_count = channel_count as usize;
    let frame_count = interleaved_samples.len() / channel_count;
    let mut mono_samples = Vec::with_capacity(frame_count);

    for frame_index in 0..frame_count {
        let mut sum = 0.0f32;
        for channel_index in 0..channel_count {
            sum += interleaved_samples[frame_index * channel_count + channel_index];
        }
        mono_samples.push(sum / channel_count as f32);
    }

    mono_samples
}

/// Resamples audio using linear interpolation.
/// Simple but effective for speech — higher quality resampling via `rubato`
/// can be added later if needed.
fn resample_linear(
    input_samples: &[f32],
    source_sample_rate: u32,
    target_sample_rate: u32,
) -> Vec<f32> {
    if input_samples.is_empty() {
        return Vec::new();
    }

    let sample_rate_ratio = source_sample_rate as f64 / target_sample_rate as f64;
    let output_length = (input_samples.len() as f64 / sample_rate_ratio).ceil() as usize;
    let mut output_samples = Vec::with_capacity(output_length);

    for output_index in 0..output_length {
        let source_position = output_index as f64 * sample_rate_ratio;
        let source_index = source_position as usize;
        let fractional_part = source_position - source_index as f64;

        let sample = if source_index + 1 < input_samples.len() {
            // Linear interpolation between adjacent samples
            let current = input_samples[source_index] as f64;
            let next = input_samples[source_index + 1] as f64;
            (current + fractional_part * (next - current)) as f32
        } else if source_index < input_samples.len() {
            input_samples[source_index]
        } else {
            0.0
        };

        output_samples.push(sample);
    }

    output_samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_16khz_sample_count() {
        // 1 second of 48kHz mono audio = 48000 samples
        let input = vec![0.0f32; 48000];
        let output = convert_float32_to_pcm16_mono(&input, 48000, 1);
        // 16000 samples * 2 bytes each = 32000 bytes
        assert_eq!(output.len(), 32000);
    }

    #[test]
    fn stereo_downmixed_to_mono() {
        // Stereo: left=1.0, right=-1.0 → mono should be ~0.0
        let stereo_input = vec![1.0f32, -1.0f32];
        let mono = downmix_to_mono(&stereo_input, 2);
        assert_eq!(mono.len(), 1);
        assert!((mono[0] - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn pcm16_encoding_correct() {
        // A single sample at 16kHz, value 0.5 → should be ~16383 as i16
        let input = vec![0.5f32];
        let output = convert_float32_to_pcm16_mono(&input, TARGET_SAMPLE_RATE, 1);
        let value = i16::from_le_bytes([output[0], output[1]]);
        assert!((value - 16383).abs() <= 1);
    }

    #[test]
    fn silence_produces_zero_bytes() {
        let silence = vec![0.0f32; 100];
        let output = convert_float32_to_pcm16_mono(&silence, TARGET_SAMPLE_RATE, 1);
        for chunk in output.chunks(2) {
            let value = i16::from_le_bytes([chunk[0], chunk[1]]);
            assert_eq!(value, 0);
        }
    }
}

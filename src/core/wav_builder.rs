/// WAV file builder — constructs a valid RIFF/WAVE header for PCM16 mono audio.
/// Ported from BuddyWAVFileBuilder in BuddyAudioConversionSupport.swift:72-108.
///
/// The WAV format is used by the OpenAI transcription fallback, which requires
/// audio uploaded as a standard WAV file rather than raw PCM.

/// Builds a complete WAV file (header + data) from raw PCM16 audio bytes.
///
/// The resulting file is a standard RIFF WAVE file:
/// - Format: PCM (format code 1)
/// - Channels: 1 (mono)
/// - Bits per sample: 16
/// - Sample rate: as specified (typically 16000 Hz)
///
/// # WAV Header Layout (44 bytes)
/// ```text
/// Offset  Size  Field           Value
/// 0       4     ChunkID         "RIFF"
/// 4       4     ChunkSize       file_size - 8
/// 8       4     Format          "WAVE"
/// 12      4     Subchunk1ID     "fmt "
/// 16      4     Subchunk1Size   16 (PCM)
/// 20      2     AudioFormat     1 (PCM)
/// 22      2     NumChannels     1 (mono)
/// 24      4     SampleRate      e.g. 16000
/// 28      4     ByteRate        sample_rate * channels * bits/8
/// 32      2     BlockAlign      channels * bits/8
/// 34      2     BitsPerSample   16
/// 36      4     Subchunk2ID     "data"
/// 40      4     Subchunk2Size   pcm_data.len()
/// 44+     var   Data            raw PCM16 bytes
/// ```
pub fn build_wav_file(pcm16_audio_data: &[u8], sample_rate: u32) -> Vec<u8> {
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_chunk_size = pcm16_audio_data.len() as u32;
    let file_size = 36 + data_chunk_size; // Total size minus RIFF header (8 bytes)

    let mut wav_file = Vec::with_capacity(44 + pcm16_audio_data.len());

    // RIFF header
    wav_file.extend_from_slice(b"RIFF");
    wav_file.extend_from_slice(&file_size.to_le_bytes());
    wav_file.extend_from_slice(b"WAVE");

    // fmt subchunk
    wav_file.extend_from_slice(b"fmt ");
    wav_file.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (PCM = 16)
    wav_file.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat (PCM = 1)
    wav_file.extend_from_slice(&channels.to_le_bytes());
    wav_file.extend_from_slice(&sample_rate.to_le_bytes());
    wav_file.extend_from_slice(&byte_rate.to_le_bytes());
    wav_file.extend_from_slice(&block_align.to_le_bytes());
    wav_file.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data subchunk
    wav_file.extend_from_slice(b"data");
    wav_file.extend_from_slice(&data_chunk_size.to_le_bytes());
    wav_file.extend_from_slice(pcm16_audio_data);

    wav_file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_has_correct_magic_bytes() {
        let pcm_data = vec![0u8; 100];
        let wav = build_wav_file(&pcm_data, 16000);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn wav_header_is_44_bytes() {
        let pcm_data = vec![0u8; 100];
        let wav = build_wav_file(&pcm_data, 16000);

        // Total size = 44 byte header + 100 bytes data
        assert_eq!(wav.len(), 144);
    }

    #[test]
    fn wav_data_chunk_size_matches_input() {
        let pcm_data = vec![0u8; 256];
        let wav = build_wav_file(&pcm_data, 16000);

        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 256);
    }

    #[test]
    fn wav_sample_rate_encoded_correctly() {
        let pcm_data = vec![0u8; 100];
        let wav = build_wav_file(&pcm_data, 16000);

        let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(sample_rate, 16000);
    }

    #[test]
    fn wav_format_is_pcm_mono_16bit() {
        let pcm_data = vec![0u8; 100];
        let wav = build_wav_file(&pcm_data, 16000);

        let audio_format = u16::from_le_bytes([wav[20], wav[21]]);
        let num_channels = u16::from_le_bytes([wav[22], wav[23]]);
        let bits_per_sample = u16::from_le_bytes([wav[34], wav[35]]);

        assert_eq!(audio_format, 1); // PCM
        assert_eq!(num_channels, 1); // mono
        assert_eq!(bits_per_sample, 16);
    }
}

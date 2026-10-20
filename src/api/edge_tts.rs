use log::{info, warn};
use std::process::Command;

/// Free Text-to-Speech using Edge-TTS (Microsoft Neural Voices).
/// Requires `edge-tts` python package or utility installed on Linux,
/// or falls back to system speech.
pub async fn synthesize_speech(text: &str, voice: Option<&str>) -> Result<Vec<u8>, String> {
    let voice = voice.unwrap_or("es-ES-AlvaroNeural"); // natural default, or en-US-ChristopherNeural
    let temp_file = format!("/tmp/clicky_edgetts_{}.mp3", std::process::id());

    info!("Synthesizing with edge-tts (voice: {voice})...");

    let output = tokio::task::spawn_blocking({
        let text = text.to_string();
        let voice = voice.to_string();
        let temp_file = temp_file.clone();
        move || {
            Command::new("edge-tts")
                .args(["--text", &text, "--voice", &voice, "--write-media", &temp_file])
                .output()
        }
    })
    .await
    .map_err(|e| format!("Task error: {e}"))?;

    match output {
        Ok(out) if out.status.success() => {
            let bytes = std::fs::read(&temp_file)
                .map_err(|e| format!("Failed to read generated MP3: {e}"))?;
            let _ = std::fs::remove_file(&temp_file);
            Ok(bytes)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            warn!("edge-tts failed: {stderr}");
            Err(format!("edge-tts failed: {stderr}"))
        }
        Err(e) => {
            warn!("edge-tts not found or failed to execute: {e}");
            Err(format!("edge-tts execution error: {e}"))
        }
    }
}

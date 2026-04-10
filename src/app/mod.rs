pub mod platform;
pub mod state_machine;
use log::info;
use platform::PlatformInfo;
use state_machine::{VoiceState, VoiceStateTransition};
use tokio::sync::mpsc;

/// Central application state
/// Owns the voice state machine, API clients, conversation history, and
/// coordinates the push-to-talk → screenshot → Claude → TTS → pointing pipeline.
pub struct AppState {
    pub platform: PlatformInfo,
    pub voice_state: VoiceState,
    pub conversation_history: crate::core::conversation::ConversationHistory,
    pub claude_api_base_url: String,
    voice_state_sender: mpsc::Sender<VoiceStateTransition>,
    voice_state_receiver: mpsc::Receiver<VoiceStateTransition>,
}

impl AppState {
    pub fn new(platform: PlatformInfo) -> Self {
        let (voice_state_sender, voice_state_receiver) = mpsc::channel(32);

        Self {
            platform,
            voice_state: VoiceState::Idle,
            conversation_history: crate::core::conversation::ConversationHistory::new(),
            claude_api_base_url: String::from(
                "https://your-worker-name.your-subdomain.workers.dev",
            ),
            voice_state_sender,
            voice_state_receiver,
        }
    }

    pub fn voice_state_sender(&self) -> mpsc::Sender<VoiceStateTransition> {
        self.voice_state_sender.clone()
    }

    /// Main application loop. Processes voice state transitions and orchestrates
    /// the full pipeline: hotkey → mic → transcription → screenshot → Claude → TTS → pointing.
    pub async fn run(&mut self) {
        info!("Clicky Desktop starting on {}", self.platform);
        info!("Voice state: {:?}", self.voice_state);

        // In a full implementation, this loop processes state transitions from
        // the hotkey monitor, audio capture, API responses, and overlay events.
        // For now, it demonstrates the event loop structure.
        while let Some(transition) = self.voice_state_receiver.recv().await {
            let previous_state = self.voice_state;
            if let Some(new_state) = self.voice_state.apply(transition) {
                self.voice_state = new_state;
                info!("Voice state: {:?} → {:?}", previous_state, self.voice_state);
            }
        }

        info!("Clicky Desktop shutting down");
    }
}

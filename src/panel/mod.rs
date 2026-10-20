#[cfg(target_os = "windows")]
use crate::config::{config_file_path, AppConfig};

#[cfg(not(target_os = "windows"))]
mod native {
    use crate::autostart;
    use crate::config::{
        config_file_path, AppConfig, LlmProviderType, PushToTalkHotkey, SttProviderType,
        TtsProviderType,
    };
    use eframe::egui;
    use std::sync::atomic::{AtomicBool, Ordering};

    static SETTINGS_WINDOW_OPEN: AtomicBool = AtomicBool::new(false);

    /// Run the onboarding/setup window on the main thread (blocking).
    /// Must be called before Raylib initialization since both need the main thread.
    pub fn run_onboarding_blocking(initial_config: AppConfig) {
        run_settings_native(initial_config, true);
    }

    pub fn open_settings_window(initial_config: AppConfig, onboarding_mode: bool) {
        if SETTINGS_WINDOW_OPEN.swap(true, Ordering::SeqCst) {
            log::info!("Settings window already open");
            return;
        }

        std::thread::spawn(move || {
            run_settings_native(initial_config, onboarding_mode);
            SETTINGS_WINDOW_OPEN.store(false, Ordering::SeqCst);
        });
    }

    fn run_settings_native(initial_config: AppConfig, onboarding_mode: bool) {
        let window_title = if onboarding_mode {
            "Clicky Setup"
        } else {
            "Clicky Settings"
        };

        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([580.0, 680.0])
                .with_min_inner_size([500.0, 520.0])
                .with_title(window_title),
            ..Default::default()
        };

        let app = SettingsApp::new(initial_config, onboarding_mode);
        let result = eframe::run_native(
            window_title,
            native_options,
            Box::new(|_cc| Ok(Box::new(app))),
        );

        if let Err(error) = result {
            log::error!("Failed to open settings window: {}", error);
        }
    }

    struct SettingsApp {
        onboarding_mode: bool,
        llm_provider: LlmProviderType,
        anthropic_api_key: String,
        custom_llm_base_url: String,
        custom_llm_api_key: String,
        custom_llm_model: String,

        stt_provider: SttProviderType,
        assemblyai_api_key: String,
        groq_api_key: String,
        custom_stt_base_url: String,
        custom_stt_api_key: String,
        custom_stt_model: String,

        tts_provider: TtsProviderType,
        elevenlabs_api_key: String,
        elevenlabs_voice_id: String,
        custom_tts_base_url: String,
        custom_tts_api_key: String,
        custom_tts_model: String,
        custom_tts_voice: String,

        push_to_talk_hotkey: PushToTalkHotkey,
        autostart_enabled: bool,
        status_message: Option<String>,
        status_is_error: bool,
    }

    impl SettingsApp {
        fn new(config: AppConfig, onboarding_mode: bool) -> Self {
            Self {
                onboarding_mode,
                llm_provider: config.llm_provider,
                anthropic_api_key: config.anthropic_api_key.unwrap_or_default(),
                custom_llm_base_url: config.custom_llm_base_url.unwrap_or_default(),
                custom_llm_api_key: config.custom_llm_api_key.unwrap_or_default(),
                custom_llm_model: config.custom_llm_model.unwrap_or_default(),

                stt_provider: config.stt_provider,
                assemblyai_api_key: config.assemblyai_api_key.unwrap_or_default(),
                groq_api_key: config.groq_api_key.unwrap_or_default(),
                custom_stt_base_url: config.custom_stt_base_url.unwrap_or_default(),
                custom_stt_api_key: config.custom_stt_api_key.unwrap_or_default(),
                custom_stt_model: config.custom_stt_model.unwrap_or_default(),

                tts_provider: config.tts_provider,
                elevenlabs_api_key: config.elevenlabs_api_key.unwrap_or_default(),
                elevenlabs_voice_id: config.elevenlabs_voice_id.unwrap_or_default(),
                custom_tts_base_url: config.custom_tts_base_url.unwrap_or_default(),
                custom_tts_api_key: config.custom_tts_api_key.unwrap_or_default(),
                custom_tts_model: config.custom_tts_model.unwrap_or_default(),
                custom_tts_voice: config.custom_tts_voice.unwrap_or_default(),

                push_to_talk_hotkey: config.push_to_talk_hotkey,
                autostart_enabled: config.autostart_enabled,
                status_message: None,
                status_is_error: false,
            }
        }

        fn save(&mut self) -> Result<(), String> {
            let config = AppConfig {
                llm_provider: self.llm_provider,
                anthropic_api_key: normalized_value(&self.anthropic_api_key),
                custom_llm_base_url: normalized_value(&self.custom_llm_base_url),
                custom_llm_api_key: normalized_value(&self.custom_llm_api_key),
                custom_llm_model: normalized_value(&self.custom_llm_model),

                stt_provider: self.stt_provider,
                assemblyai_api_key: normalized_value(&self.assemblyai_api_key),
                groq_api_key: normalized_value(&self.groq_api_key),
                custom_stt_base_url: normalized_value(&self.custom_stt_base_url),
                custom_stt_api_key: normalized_value(&self.custom_stt_api_key),
                custom_stt_model: normalized_value(&self.custom_stt_model),

                tts_provider: self.tts_provider,
                elevenlabs_api_key: normalized_value(&self.elevenlabs_api_key),
                elevenlabs_voice_id: normalized_value(&self.elevenlabs_voice_id),
                custom_tts_base_url: normalized_value(&self.custom_tts_base_url),
                custom_tts_api_key: normalized_value(&self.custom_tts_api_key),
                custom_tts_model: normalized_value(&self.custom_tts_model),
                custom_tts_voice: normalized_value(&self.custom_tts_voice),

                push_to_talk_hotkey: self.push_to_talk_hotkey,
                autostart_enabled: self.autostart_enabled,
            };

            // Validation based on chosen providers
            match config.llm_provider {
                LlmProviderType::Anthropic => {
                    if config.anthropic_api_key.is_none() {
                        return Err("Anthropic API key is required when using Claude".into());
                    }
                }
                LlmProviderType::OpenRouter | LlmProviderType::Gemini | LlmProviderType::Mistral => {
                    if config.custom_llm_api_key.is_none() {
                        return Err(format!("API key is required for {}", config.llm_provider.display_name()));
                    }
                }
                LlmProviderType::Custom => {
                    if config.custom_llm_base_url.is_none() {
                        return Err("Base URL is required for Custom OpenAI-compatible LLM".into());
                    }
                }
                LlmProviderType::Ollama => {} // Ollama runs locally without key
            }

            match config.stt_provider {
                SttProviderType::AssemblyAI => {
                    if config.assemblyai_api_key.is_none() {
                        return Err("AssemblyAI API key is required".into());
                    }
                }
                SttProviderType::Groq => {
                    if config.groq_api_key.is_none() {
                        return Err("Groq API key is required for Whisper STT".into());
                    }
                }
                SttProviderType::OpenAiCompat => {
                    if config.custom_stt_base_url.is_none() && config.custom_stt_api_key.is_none() {
                        return Err("Custom Whisper Base URL or API key is required".into());
                    }
                }
            }

            match config.tts_provider {
                TtsProviderType::ElevenLabs => {
                    if config.elevenlabs_api_key.is_none() {
                        return Err("ElevenLabs API key is required".into());
                    }
                }
                TtsProviderType::OpenAiCompat => {
                    if config.custom_tts_base_url.is_none() && config.custom_tts_api_key.is_none() {
                        return Err("Custom TTS Base URL or API key is required".into());
                    }
                }
                TtsProviderType::EdgeTts | TtsProviderType::System => {} // Free / no key
            }

            config.save()?;

            if autostart::is_supported() {
                autostart::set_enabled(config.autostart_enabled)?;
            }

            Ok(())
        }
    }

    impl eframe::App for SettingsApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(8.0);
                    ui.heading(if self.onboarding_mode {
                        "Set up Clicky"
                    } else {
                        "Clicky Settings"
                    });
                    ui.add_space(4.0);
                    ui.label(
                        "Configure your preferred Vision LLM, Voice Transcription (STT), and Text-to-Speech (TTS) providers.",
                    );
                    if let Some(config_path) = config_file_path() {
                        ui.small(format!("Config file: {}", config_path.display()));
                    }
                    ui.add_space(12.0);

                    // 1. VISION LLM SECTION
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.strong("1. Vision LLM Provider");
                        ui.label("Analyzes screenshots and answers your voice questions.");
                        ui.add_space(4.0);

                        egui::ComboBox::from_id_salt("llm_provider_combo")
                            .selected_text(self.llm_provider.display_name())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.llm_provider, LlmProviderType::Anthropic, LlmProviderType::Anthropic.display_name());
                                ui.selectable_value(&mut self.llm_provider, LlmProviderType::OpenRouter, LlmProviderType::OpenRouter.display_name());
                                ui.selectable_value(&mut self.llm_provider, LlmProviderType::Ollama, LlmProviderType::Ollama.display_name());
                                ui.selectable_value(&mut self.llm_provider, LlmProviderType::Gemini, LlmProviderType::Gemini.display_name());
                                ui.selectable_value(&mut self.llm_provider, LlmProviderType::Mistral, LlmProviderType::Mistral.display_name());
                                ui.selectable_value(&mut self.llm_provider, LlmProviderType::Custom, LlmProviderType::Custom.display_name());
                            });

                        ui.add_space(6.0);
                        match self.llm_provider {
                            LlmProviderType::Anthropic => {
                                ui.label("Anthropic API key:");
                                ui.add(egui::TextEdit::singleline(&mut self.anthropic_api_key).password(true).desired_width(f32::INFINITY));
                                ui.small("Uses Claude 3.5/3.7 Sonnet with native Computer Use.");
                            }
                            LlmProviderType::OpenRouter => {
                                ui.label("OpenRouter API key:");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_api_key).password(true).desired_width(f32::INFINITY));
                                ui.label("Model (free or paid):");
                                if self.custom_llm_model.trim().is_empty() {
                                    self.custom_llm_model = LlmProviderType::OpenRouter.default_model().to_string();
                                }
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_model).desired_width(f32::INFINITY));
                                ui.small("Supports free vision models like google/gemini-2.0-flash-exp:free or meta-llama/llama-3.2-11b-vision-instruct:free");
                            }
                            LlmProviderType::Ollama => {
                                ui.label("Ollama Base URL:");
                                if self.custom_llm_base_url.trim().is_empty() {
                                    self.custom_llm_base_url = LlmProviderType::Ollama.default_base_url().to_string();
                                }
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_base_url).desired_width(f32::INFINITY));
                                ui.label("Model name:");
                                if self.custom_llm_model.trim().is_empty() {
                                    self.custom_llm_model = LlmProviderType::Ollama.default_model().to_string();
                                }
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_model).desired_width(f32::INFINITY));
                                ui.small("Runs 100% locally and free. (e.g. llama3.2-vision or qwen2.5-vl)");
                            }
                            LlmProviderType::Gemini => {
                                ui.label("Google AI Studio / Gemini API key:");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_api_key).password(true).desired_width(f32::INFINITY));
                                ui.label("Model:");
                                if self.custom_llm_model.trim().is_empty() {
                                    self.custom_llm_model = LlmProviderType::Gemini.default_model().to_string();
                                }
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_model).desired_width(f32::INFINITY));
                                ui.small("Generous free tier with Gemini 2.0 Flash.");
                            }
                            LlmProviderType::Mistral => {
                                ui.label("Mistral API key:");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_api_key).password(true).desired_width(f32::INFINITY));
                                ui.label("Model:");
                                if self.custom_llm_model.trim().is_empty() {
                                    self.custom_llm_model = LlmProviderType::Mistral.default_model().to_string();
                                }
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_model).desired_width(f32::INFINITY));
                                ui.small("Uses Pixtral multimodal vision models.");
                            }
                            LlmProviderType::Custom => {
                                ui.label("Base URL (e.g. http://localhost:8000/v1):");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_base_url).desired_width(f32::INFINITY));
                                ui.label("API key (optional):");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_api_key).password(true).desired_width(f32::INFINITY));
                                ui.label("Model name:");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_llm_model).desired_width(f32::INFINITY));
                            }
                        }
                    });

                    ui.add_space(10.0);

                    // 2. STT TRANSCRIPTION SECTION
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.strong("2. Speech Transcription (STT)");
                        ui.label("Converts your spoken microphone audio to text.");
                        ui.add_space(4.0);

                        egui::ComboBox::from_id_salt("stt_provider_combo")
                            .selected_text(self.stt_provider.display_name())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.stt_provider, SttProviderType::AssemblyAI, SttProviderType::AssemblyAI.display_name());
                                ui.selectable_value(&mut self.stt_provider, SttProviderType::Groq, SttProviderType::Groq.display_name());
                                ui.selectable_value(&mut self.stt_provider, SttProviderType::OpenAiCompat, SttProviderType::OpenAiCompat.display_name());
                            });

                        ui.add_space(6.0);
                        match self.stt_provider {
                            SttProviderType::AssemblyAI => {
                                ui.label("AssemblyAI API key:");
                                ui.add(egui::TextEdit::singleline(&mut self.assemblyai_api_key).password(true).desired_width(f32::INFINITY));
                                ui.small("Real-time streaming transcription via WebSocket.");
                            }
                            SttProviderType::Groq => {
                                ui.label("Groq API key (gsk_...):");
                                ui.add(egui::TextEdit::singleline(&mut self.groq_api_key).password(true).desired_width(f32::INFINITY));
                                ui.small("Ultra-fast Whisper (~300ms) with generous free tier.");
                            }
                            SttProviderType::OpenAiCompat => {
                                ui.label("Base URL (e.g. http://localhost:8000/v1):");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_stt_base_url).desired_width(f32::INFINITY));
                                ui.label("API key (optional):");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_stt_api_key).password(true).desired_width(f32::INFINITY));
                                ui.label("Model:");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_stt_model).desired_width(f32::INFINITY));
                            }
                        }
                    });

                    ui.add_space(10.0);

                    // 3. TTS VOICE SECTION
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.strong("3. Text-to-Speech (TTS)");
                        ui.label("Speaks Clicky's response back to you.");
                        ui.add_space(4.0);

                        egui::ComboBox::from_id_salt("tts_provider_combo")
                            .selected_text(self.tts_provider.display_name())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.tts_provider, TtsProviderType::ElevenLabs, TtsProviderType::ElevenLabs.display_name());
                                ui.selectable_value(&mut self.tts_provider, TtsProviderType::EdgeTts, TtsProviderType::EdgeTts.display_name());
                                ui.selectable_value(&mut self.tts_provider, TtsProviderType::OpenAiCompat, TtsProviderType::OpenAiCompat.display_name());
                                ui.selectable_value(&mut self.tts_provider, TtsProviderType::System, TtsProviderType::System.display_name());
                            });

                        ui.add_space(6.0);
                        match self.tts_provider {
                            TtsProviderType::ElevenLabs => {
                                ui.label("ElevenLabs API key:");
                                ui.add(egui::TextEdit::singleline(&mut self.elevenlabs_api_key).password(true).desired_width(f32::INFINITY));
                                ui.label("Voice ID (optional):");
                                ui.add(egui::TextEdit::singleline(&mut self.elevenlabs_voice_id).desired_width(f32::INFINITY));
                            }
                            TtsProviderType::EdgeTts => {
                                ui.label("Voice (e.g. es-ES-AlvaroNeural, en-US-ChristopherNeural):");
                                if self.custom_tts_voice.trim().is_empty() {
                                    self.custom_tts_voice = "es-ES-AlvaroNeural".to_string();
                                }
                                ui.add(egui::TextEdit::singleline(&mut self.custom_tts_voice).desired_width(f32::INFINITY));
                                ui.small("100% Free high-fidelity neural voices. No API key required.");
                            }
                            TtsProviderType::OpenAiCompat => {
                                ui.label("Base URL (e.g. https://api.openai.com/v1):");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_tts_base_url).desired_width(f32::INFINITY));
                                ui.label("API key (optional):");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_tts_api_key).password(true).desired_width(f32::INFINITY));
                                ui.label("Voice (e.g. alloy, echo):");
                                ui.add(egui::TextEdit::singleline(&mut self.custom_tts_voice).desired_width(f32::INFINITY));
                            }
                            TtsProviderType::System => {
                                ui.label("Uses local espeak-ng synthesizer. Fast and offline, but robotic.");
                            }
                        }
                    });

                    ui.add_space(10.0);

                    // 4. GENERAL APP SETTINGS
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.strong("4. App Settings");
                        ui.label("Push-to-talk hotkey:");
                        egui::ComboBox::from_id_salt("push_to_talk_hotkey")
                            .selected_text(self.push_to_talk_hotkey.display_name())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.push_to_talk_hotkey,
                                    PushToTalkHotkey::CtrlSpace,
                                    PushToTalkHotkey::CtrlSpace.display_name(),
                                );
                                ui.selectable_value(
                                    &mut self.push_to_talk_hotkey,
                                    PushToTalkHotkey::CtrlGrave,
                                    PushToTalkHotkey::CtrlGrave.display_name(),
                                );
                            });

                        if autostart::is_supported() {
                            ui.add_space(6.0);
                            ui.checkbox(
                                &mut self.autostart_enabled,
                                "Start Clicky automatically on login",
                            );
                        }
                    });

                    ui.add_space(14.0);
                    if let Some(message) = &self.status_message {
                        let color = if self.status_is_error {
                            egui::Color32::from_rgb(220, 92, 92)
                        } else {
                            egui::Color32::from_rgb(90, 180, 120)
                        };
                        ui.colored_label(color, message);
                        ui.add_space(8.0);
                    }

                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            match self.save() {
                                Ok(()) => {
                                    self.status_message = Some(
                                        "Saved. Settings apply automatically within about a second."
                                            .to_string(),
                                    );
                                    self.status_is_error = false;
                                }
                                Err(error) => {
                                    self.status_message = Some(error);
                                    self.status_is_error = true;
                                }
                            }
                        }

                        if ui.button("Save and Close").clicked() {
                            match self.save() {
                                Ok(()) => {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                                Err(error) => {
                                    self.status_message = Some(error);
                                    self.status_is_error = true;
                                }
                            }
                        }

                        if !self.onboarding_mode && ui.button("Close").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(10.0);
                });
            });
        }
    }

    fn normalized_value(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub use native::{open_settings_window, run_onboarding_blocking};

#[cfg(target_os = "windows")]
pub fn run_onboarding_blocking(_initial_config: AppConfig) {
    show_windows_settings_fallback(true);
}

#[cfg(target_os = "windows")]
pub fn open_settings_window(_initial_config: AppConfig, onboarding_mode: bool) {
    show_windows_settings_fallback(onboarding_mode);
}

#[cfg(target_os = "windows")]
fn show_windows_settings_fallback(onboarding_mode: bool) {
    let config_path = config_file_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "%APPDATA%\\clicky-desktop\\config.toml".to_string());

    let title = if onboarding_mode {
        "Clicky Setup"
    } else {
        "Clicky Settings"
    };
    let message = format!(
        "Windows settings UI is temporarily disabled in this build.\n\nEdit your configuration in:\n{}\n\nEnvironment variables still override the config file.",
        config_path
    );

    log::warn!("{}", message.replace('\n', " "));
    show_message_box(title, &message);
}

#[cfg(target_os = "windows")]
fn show_message_box(title: &str, message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    let title_wide = to_wide(title);
    let message_wide = to_wide(message);

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message_wide.as_ptr(),
            title_wide.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(target_os = "windows")]
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

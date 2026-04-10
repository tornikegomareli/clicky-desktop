use crate::autostart;
use crate::config::{config_file_path, AppConfig, PushToTalkHotkey};
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};

static SETTINGS_WINDOW_OPEN: AtomicBool = AtomicBool::new(false);

pub fn open_settings_window(initial_config: AppConfig, onboarding_mode: bool) {
    if SETTINGS_WINDOW_OPEN.swap(true, Ordering::SeqCst) {
        log::info!("Settings window already open");
        return;
    }

    std::thread::spawn(move || {
        let window_title = if onboarding_mode {
            "Clicky Setup"
        } else {
            "Clicky Settings"
        };

        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([520.0, 540.0])
                .with_min_inner_size([480.0, 500.0])
                .with_title(window_title),
            ..Default::default()
        };

        let app = SettingsApp::new(initial_config, onboarding_mode);
        let result = eframe::run_native(
            window_title,
            native_options,
            Box::new(|_cc| Ok(Box::new(app))),
        );

        SETTINGS_WINDOW_OPEN.store(false, Ordering::SeqCst);

        if let Err(error) = result {
            log::error!("Failed to open settings window: {}", error);
        }
    });
}

struct SettingsApp {
    onboarding_mode: bool,
    anthropic_api_key: String,
    assemblyai_api_key: String,
    elevenlabs_api_key: String,
    elevenlabs_voice_id: String,
    push_to_talk_hotkey: PushToTalkHotkey,
    autostart_enabled: bool,
    status_message: Option<String>,
    status_is_error: bool,
}

impl SettingsApp {
    fn new(config: AppConfig, onboarding_mode: bool) -> Self {
        Self {
            onboarding_mode,
            anthropic_api_key: config.anthropic_api_key.unwrap_or_default(),
            assemblyai_api_key: config.assemblyai_api_key.unwrap_or_default(),
            elevenlabs_api_key: config.elevenlabs_api_key.unwrap_or_default(),
            elevenlabs_voice_id: config.elevenlabs_voice_id.unwrap_or_default(),
            push_to_talk_hotkey: config.push_to_talk_hotkey,
            autostart_enabled: config.autostart_enabled,
            status_message: None,
            status_is_error: false,
        }
    }

    fn save(&mut self) -> Result<(), String> {
        let config = AppConfig {
            anthropic_api_key: normalized_value(&self.anthropic_api_key),
            assemblyai_api_key: normalized_value(&self.assemblyai_api_key),
            elevenlabs_api_key: normalized_value(&self.elevenlabs_api_key),
            elevenlabs_voice_id: normalized_value(&self.elevenlabs_voice_id),
            push_to_talk_hotkey: self.push_to_talk_hotkey,
            autostart_enabled: self.autostart_enabled,
        };

        if config.anthropic_api_key.is_none() {
            return Err("Anthropic API key is required".into());
        }
        if config.assemblyai_api_key.is_none() {
            return Err("AssemblyAI API key is required".into());
        }
        if config.elevenlabs_api_key.is_none() {
            return Err("ElevenLabs API key is required".into());
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
            ui.add_space(8.0);
            ui.heading(if self.onboarding_mode {
                "Set up Clicky"
            } else {
                "Clicky Settings"
            });
            ui.add_space(6.0);
            ui.label(
                "Clicky needs Anthropic, AssemblyAI, and ElevenLabs credentials to run the voice pipeline.",
            );
            if let Some(config_path) = config_file_path() {
                ui.small(format!("Config file: {}", config_path.display()));
            }
            ui.add_space(16.0);

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("Setup checklist");
                ui.small(format!(
                    "Anthropic: {}",
                    if self.anthropic_api_key.trim().is_empty() {
                        "missing"
                    } else {
                        "configured"
                    }
                ));
                ui.small(format!(
                    "AssemblyAI: {}",
                    if self.assemblyai_api_key.trim().is_empty() {
                        "missing"
                    } else {
                        "configured"
                    }
                ));
                ui.small(format!(
                    "ElevenLabs key: {}",
                    if self.elevenlabs_api_key.trim().is_empty() {
                        "missing"
                    } else {
                        "configured"
                    }
                ));
                ui.small(format!(
                    "ElevenLabs voice: {}",
                    if self.elevenlabs_voice_id.trim().is_empty() {
                        "not set, default voice will be used if supported"
                    } else {
                        "configured"
                    }
                ));
                ui.small(format!(
                    "Push-to-talk hotkey: {}",
                    self.push_to_talk_hotkey.display_name()
                ));
                if autostart::is_supported() {
                    ui.small(format!(
                        "Autostart: {}",
                        if self.autostart_enabled { "enabled" } else { "disabled" }
                    ));
                }
            });

            ui.add_space(14.0);

            ui.label("Anthropic API key");
            ui.add(
                egui::TextEdit::singleline(&mut self.anthropic_api_key)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(10.0);
            ui.label("AssemblyAI API key");
            ui.add(
                egui::TextEdit::singleline(&mut self.assemblyai_api_key)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(10.0);
            ui.label("ElevenLabs API key");
            ui.add(
                egui::TextEdit::singleline(&mut self.elevenlabs_api_key)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(10.0);
            ui.label("ElevenLabs voice ID");
            ui.add(
                egui::TextEdit::singleline(&mut self.elevenlabs_voice_id)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(10.0);
            ui.label("Push-to-talk hotkey");
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
            ui.small("Saved settings are picked up automatically within about a second.");

            if autostart::is_supported() {
                ui.add_space(10.0);
                ui.checkbox(&mut self.autostart_enabled, "Start Clicky automatically on login");
            }

            ui.add_space(18.0);
            if let Some(message) = &self.status_message {
                let color = if self.status_is_error {
                    egui::Color32::from_rgb(220, 92, 92)
                } else {
                    egui::Color32::from_rgb(90, 180, 120)
                };
                ui.colored_label(color, message);
                ui.add_space(10.0);
            }

            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    match self.save() {
                        Ok(()) => {
                            self.status_message = Some(
                                "Saved. Settings apply automatically within about a second.".to_string(),
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

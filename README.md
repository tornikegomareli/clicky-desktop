# Clicky Desktop

Clicky is a Linux-first desktop companion that lives in the system tray, listens on push-to-talk, looks at your screen, answers with voice, and can point at UI elements with a blue overlay triangle.

## Current Scope

- Linux-first release target, with Windows builds kept working in CI
- Anthropic for vision + response generation
- AssemblyAI for live transcription
- ElevenLabs for text-to-speech
- Raylib overlay for cursor-following, target pointing, and speech bubbles
- System tray app with persisted settings, hotkey selection, and autostart

OpenAI and worker-proxy flows are intentionally out of the active product path.

## How It Works

1. Hold the global push-to-talk hotkey.
2. Clicky records microphone audio and streams it to AssemblyAI.
3. On release, it captures screenshots of all monitors.
4. Transcript plus screenshots are sent to Claude.
5. Claude responds conversationally and can include a point target.
6. ElevenLabs speaks the response.
7. The overlay triangle flies to the target and returns to the cursor.

## Requirements

- Rust toolchain
- `espeak-ng` installed for TTS fallback
- Linux:
  - GTK 3 development packages for the tray UI
  - OpenGL/X11/Wayland development libraries for Raylib
- Windows:
  - Rust MSVC toolchain

## First-Run Setup

On first launch, Clicky opens a setup window and asks for:

- `ANTHROPIC_API_KEY`
- `ASSEMBLYAI_API_KEY`
- `ELEVENLABS_API_KEY`
- optional `ELEVENLABS_VOICE_ID`

Saved settings are stored in a platform-specific config directory. The resolved config path is logged at startup and shown inside the setup window.

## Local Development

```bash
cargo build
cargo run
```

Environment variables in `.env` still override the saved config, which is useful for local development.

## Useful Debug Flags

Debug builds only:

```bash
CLICKY_FORCE_SETUP_WINDOW=1 cargo run
CLICKY_SIMULATE=1 cargo run
```

## Config

- `.env` is loaded automatically in local development
- saved settings are persisted by the app UI
- `.env` values override saved settings when both are present

## Testing

```bash
cargo check
cargo test
```

GitHub Actions runs PR validation with:

- Linux tests
- Linux release build
- Windows release build

## Project Layout

- `src/main.rs`: application bootstrap and render-loop entrypoint
- `src/runtime/`: async transcription, screenshot, Claude, TTS, and Computer Use pipeline
- `src/api/`: Anthropic, AssemblyAI, ElevenLabs, Computer Use integrations
- `src/overlay/`: transparent overlay renderer
- `src/panel/`: egui setup/settings window
- `src/screenshot/`: multi-monitor capture
- `src/hotkey/`: global push-to-talk backends
- `src/cursor_tracker/`: global cursor tracking per platform
- `src/config/`: persisted app configuration
- `src/autostart/`: login-start integration

## Known Limitations

- Linux X11 overlay taskbar/dock suppression remains unresolved.
- Wayland/Hyprland overlay behavior is still constrained by the current Raylib/GLFW backend.
- Packaging and signed distributables are not finished yet.

## Release Target

The current `0.1` target is a developer release with:

- working local setup flow
- Linux and Windows CI builds
- basic PR validation
- contributor-facing README and issue templates
- packaging plan defined, even if final distributables are still pending

# Clicky Desktop

System tray app with push-to-talk voice input. User holds a hotkey, mic captures audio, streams to AssemblyAI for transcription, takes screenshot of all monitors, sends transcript + screenshots to Claude/OpenAI with vision, response is spoken via ElevenLabs TTS, and a blue triangle cursor overlay flies to UI elements Claude references via bezier arc animation.

## Tech Stack

Rust, Raylib (overlay rendering), tokio (async), reqwest (HTTP), tokio-tungstenite (WebSocket), cpal (mic capture), rodio (audio playback), xcap/grim (screenshots), tray-icon (system tray), global-hotkey + evdev (hotkeys).

## Architecture

- `src/main.rs` — 60fps Raylib render loop wiring everything together
- `src/app/` — platform detection (OS, X11/Wayland, Hyprland/Sway) + voice state machine (idle/listening/processing/responding)
- `src/core/` — portable algorithms: bezier flight, POINT tag parser, audio RMS, WAV builder, PCM16 converter, coordinate mapper, conversation history, design system tokens
- `src/api/` — Claude SSE streaming, OpenAI, ElevenLabs TTS, AssemblyAI WebSocket transcription. All support direct API key mode + Cloudflare Worker proxy mode
- `src/overlay/` — Raylib transparent overlay: cursor triangle, waveform, spinner, speech bubbles, bezier flight animation
- `src/tray/` — system tray icon with menu
- `src/panel/` — settings panel (placeholder)
- `src/hotkey/` — global push-to-talk: global-hotkey crate on X11/Win, evdev on Wayland
- `src/audio/` — mic capture (cpal) + MP3 playback (rodio)
- `src/screenshot/` — multi-monitor capture: grim per-output on Hyprland/Sway, xcap on X11/Win, GNOME animation suppression
- `src/cursor_tracker/` — mouse position: evdev on Wayland, Raylib fallback on X11

## Config

Via `.env`: `ASSEMBLYAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `ELEVENLABS_API_KEY`, `CLICKY_WORKER_URL`

## Build & Run

```bash
cargo build
cargo run          # loads .env automatically via dotenvy
```

## Implementation Phases

- **Phase 1 DONE**: Cargo project, platform detection, state machine, system tray, global hotkey, Raylib transparent overlay with click-through, cursor tracking (evdev + fallback).
- **Phase 2 DONE**: cpal mic capture -> PCM16 16kHz conversion -> AssemblyAI WebSocket streaming with turn-based transcripts -> waveform visualization -> full hotkey->transcript pipeline with audio drain and cancel/restart.
- **Phase 3 DONE**: Multi-monitor screenshots (grim per-output on Hyprland, xcap on X11), Claude + OpenAI vision API with SSE streaming, POINT tag parsing, coordinate mapping (screenshot pixels -> logical display coords), conversation history, bezier flight to pointed elements.
- **Phase 4 NEXT**: Wire TTS audio playback — LLM response -> ElevenLabs TTS -> MP3 playback via AudioPlayer, state transitions Processing->Responding->Idle, dual-mode (direct API key + worker proxy) for elevenlabs.rs, espeak-ng fallback.
- **Phase 5**: Cursor animation polish — POINT coordinates -> bezier flight with proper timing, character-by-character speech bubble streaming, return flight to mouse after pointing.
- **Phase 6**: Settings + packaging — egui settings panel, config persistence, .deb/AppImage packaging, autostart, Hyprland config snippet.

## Conventions

- Clear variable names, no abbreviations.
- Comments explain "why" not "what".
- Don't add features beyond what's asked.
- Don't fix warnings unless asked.
- Read files before editing.
- Dual-mode API pattern: direct API key for dev, Cloudflare Worker proxy for production (same pattern across claude.rs, assemblyai.rs, and elevenlabs.rs).
- `UiEvent` enum is the bridge between async tasks and the sync 60fps render loop via `std::sync::mpsc`.

## Key Patterns

- **Voice state machine**: `VoiceState` enum with `apply(VoiceStateTransition)` — Idle -> Listening -> Processing -> Responding -> Idle
- **Async bridge**: tokio tasks communicate with the Raylib render loop via `std_mpsc::channel<UiEvent>`
- **LLM pipeline**: transcript + screenshots -> Claude/OpenAI API -> parse POINT tags -> UiEvent::LlmResponse
- **Audio player**: `AudioPlayer` struct wraps rodio, has `play_mp3(bytes)`, `is_playing()`, `stop()` — already implemented in `src/audio/playback.rs`
- **ElevenLabs client**: `elevenlabs::synthesize_speech()` currently only supports worker proxy mode — Phase 4 adds direct API key mode

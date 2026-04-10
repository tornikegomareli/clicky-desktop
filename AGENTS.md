# Project Agents

This file is the persistent Codex context for `clicky-desktop`. Keep it focused on stable architecture, conventions, and workflow notes that are useful every turn.

## Primary References

- Read [CLAUDE.md](/Users/tgomareli/Development/clicky-desktop/CLAUDE.md) when deeper product context or implementation phase notes are needed.
- Rust-focused assistance should use the installed skills under `~/.codex/skills`.
- Preferred Rust skills for this repo:
  - `rust-router` for general Rust questions and routing
  - `rust-learner` for Rust/crate version lookups
  - `coding-guidelines` for style and conventions
  - `unsafe-checker` for unsafe code and FFI review
  - `m01-ownership` through `m15-anti-pattern` for focused Rust problem-solving

## Product Overview

`clicky-desktop` is a system tray desktop app with push-to-talk voice input.

High-level flow:

1. User holds a global hotkey.
2. Microphone audio is captured and streamed to AssemblyAI for transcription.
3. On release, the app captures screenshots of all monitors.
4. Transcript plus screenshots are sent to Claude or OpenAI vision.
5. Claude Computer Use can return precise screen coordinates.
6. Response audio is synthesized with ElevenLabs, with `espeak-ng` fallback.
7. A blue triangle overlay animates to the detected UI target and returns to the cursor.

## Tech Stack

- Rust
- `raylib` for the transparent overlay renderer
- `tokio` for async runtime
- `reqwest` and `tokio-tungstenite` for HTTP and WebSocket APIs
- `cpal` for microphone capture
- `rodio` for MP3 playback
- `xcap` and `grim` for screenshots
- `tray-icon` for the system tray
- `global-hotkey` and `evdev` for push-to-talk hotkeys

## Code Layout

- `src/main.rs`
  - 60 FPS Raylib render loop that wires together hotkeys, async tasks, audio, overlay, and state transitions.
- `src/app/`
  - Platform detection and the `VoiceState` state machine.
- `src/core/`
  - Portable logic: bezier flight, POINT tag parsing, bubble text, RMS tracking, WAV building, PCM16 conversion, coordinate mapping, conversation history, design tokens.
- `src/api/`
  - Claude SSE, Claude Computer Use, OpenAI, ElevenLabs, and AssemblyAI integrations.
  - Important pattern: all APIs support both direct API-key mode and Cloudflare Worker proxy mode.
- `src/overlay/`
  - Transparent Raylib overlay: triangle cursor, bloom, waveform, loading arc, speech bubble, flight animation.
- `src/tray/`
  - Tray icon and menu integration.
- `src/panel/`
  - Settings panel placeholder.
- `src/hotkey/`
  - Global push-to-talk implementations.
- `src/audio/`
  - Microphone capture and MP3 playback.
- `src/screenshot/`
  - Multi-monitor capture.
- `src/cursor_tracker/`
  - Global cursor tracking across Wayland, X11, and Windows.

## Core Runtime Patterns

- Voice state machine:
  - `VoiceState` transitions are `Idle -> Listening -> Processing -> Responding -> Idle`.
- Async bridge:
  - Tokio tasks communicate back to the synchronous render loop through `std::sync::mpsc::channel<UiEvent>`.
- LLM pipeline:
  - transcript + screenshots -> vision model -> parse POINT tags -> optional Computer Use coordinate detection -> UI event back to render loop.
- TTS pipeline:
  - spoken response -> ElevenLabs synthesis -> `UiEvent::TtsAudio` -> `AudioPlayer::play_mp3()`.
- Computer Use:
  - `api::computer_use::detect_element_location()` is used for precise coordinates and is preferred over POINT tags when available.
- Cursor animation:
  - `FollowingMouse -> NavigatingToTarget -> PointingAtTarget -> ReturningToMouse -> FollowingMouse`.

## Platform Notes

- Wayland cursor tracking uses `evdev` because Raylib mouse tracking does not work reliably with click-through passthrough windows.
- X11 uses Raylib fallback cursor tracking.
- Screenshot capture differs by compositor and display server.

## Config

Environment variables used by the app:

- `ASSEMBLYAI_API_KEY`
- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- `ELEVENLABS_API_KEY`
- `ELEVENLABS_VOICE_ID`
- `CLICKY_WORKER_URL`

`dotenvy` loads `.env` automatically in `cargo run`.

## Build And Run

```bash
cargo build
cargo run
```

## Rust Defaults

When creating new Rust crates or editing package metadata, prefer:

```toml
[package]
edition = "2024"
rust-version = "1.85"

[lints.rust]
unsafe_code = "warn"

[lints.clippy]
all = "warn"
pedantic = "warn"
```

Note: the current repo `Cargo.toml` may still be on older settings. Follow the repo’s current state unless the task includes modernizing it.

## Project Conventions

- Keep code simple.
- Simplicity is more important than cleverness.
- If something is uncertain and the risk is meaningful, stop and ask.
- Use clear variable names, no unnecessary abbreviations.
- Comments should explain why, not what.
- Do not add features beyond the request.
- Do not fix warnings unless asked.
- Read files before editing.
- Preserve the dual-mode API pattern consistently across integrations.
- Treat `UiEvent` as the async-to-render-loop boundary.

## Current Product Status

- Phase 1 complete: app shell, platform detection, tray, hotkey, overlay basics, cursor tracking.
- Phase 2 complete: mic capture, PCM conversion, AssemblyAI streaming, waveform, hotkey-to-transcript pipeline.
- Phase 3 complete: screenshots, vision APIs, POINT parsing, coordinate mapping, conversation history, cursor flight, Computer Use.
- Phase 4 complete: ElevenLabs TTS, playback, fallback behavior, lifecycle integration.
- Phase 5 complete: cursor animation polish, return flight, bubble styling, font, loading animation.
- Phase 6 pending: settings, config persistence, packaging, autostart.

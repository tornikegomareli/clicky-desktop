# Clicky Desktop

System tray app with push-to-talk voice input. User holds a hotkey, mic captures audio, streams to AssemblyAI for transcription, takes screenshot of all monitors, sends transcript + screenshots to Claude with vision + Computer Use API for precise element detection, response is spoken via ElevenLabs TTS (espeak-ng fallback), and a blue triangle cursor overlay flies to UI elements along a bezier arc animation.

## Tech Stack

Rust, Raylib (overlay rendering), tokio (async), reqwest (HTTP), tokio-tungstenite (WebSocket), cpal (mic capture), rodio (audio playback), xcap/grim (screenshots), tray-icon (system tray), global-hotkey + evdev (hotkeys).

## Architecture

- `src/main.rs` — 60fps Raylib render loop wiring everything together
- `src/app/` — platform detection (OS, X11/Wayland, Hyprland/Sway) + voice state machine (idle/listening/processing/responding)
- `src/core/` — portable algorithms: bezier flight (forward + return), POINT tag parser, bubble text phrases, audio RMS, WAV builder, PCM16 converter, coordinate mapper, conversation history, design system tokens
- `src/api/` — Claude SSE streaming, Claude Computer Use API, OpenAI, ElevenLabs TTS, AssemblyAI WebSocket transcription. All support direct API key mode + Cloudflare Worker proxy mode
- `src/overlay/` — Raylib transparent overlay: cursor triangle with bloom effect, waveform, rotating arc loader, glass speech bubbles with custom TTF font, bezier flight animation (forward + return)
- `src/tray/` — system tray icon with menu
- `src/panel/` — settings panel (placeholder)
- `src/hotkey/` — global push-to-talk: global-hotkey crate on X11/Win, evdev on Wayland
- `src/audio/` — mic capture (cpal) + MP3 playback (rodio)
- `src/screenshot/` — multi-monitor capture: grim per-output on Hyprland/Sway, xcap on X11/Win, GNOME animation suppression
- `src/cursor_tracker/` — mouse position: evdev on Wayland, Raylib fallback on X11

## Config

Via `.env`: `ASSEMBLYAI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `ELEVENLABS_API_KEY`, `ELEVENLABS_VOICE_ID`, `CLICKY_WORKER_URL`

## Build & Run

```bash
cargo build
cargo run          # loads .env automatically via dotenvy
```

## Implementation Phases

- **Phase 1 DONE**: Cargo project, platform detection, state machine, system tray, global hotkey, Raylib transparent overlay with click-through, cursor tracking (evdev + fallback).
- **Phase 2 DONE**: cpal mic capture -> PCM16 16kHz conversion -> AssemblyAI WebSocket streaming with turn-based transcripts -> waveform visualization -> full hotkey->transcript pipeline with audio drain and cancel/restart.
- **Phase 3 DONE**: Multi-monitor screenshots (grim per-output on Hyprland, xcap on X11), Claude + OpenAI vision API with SSE streaming, POINT tag parsing, coordinate mapping (screenshot pixels -> logical display coords), conversation history, bezier flight to pointed elements. Claude Computer Use API for precise element coordinate detection.
- **Phase 4 DONE**: ElevenLabs TTS wired end-to-end — LLM response -> synthesize_speech() -> MP3 playback via AudioPlayer. Dual-mode (direct API key + worker proxy) for elevenlabs.rs. TTS fires in pipeline parallel with Computer Use. Responding->Idle waits for TTS completion. espeak-ng fallback on TTS failure. Hotkey press stops playback.
- **Phase 5 DONE**: Cursor animation polish — tuned bezier flight (slower forward 600px/s, faster return 900px/s), animated return flight to mouse with gentler arc, 2-second hold at target before return. Triangle-shaped bloom effect (scaled transparent copies). Glass speech bubble with blue tint, custom TTF font (AdwaitaSans), white text, curated friendly phrases. Rotating arc loading animation.
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
- **LLM pipeline**: transcript + screenshots -> Claude vision API -> parse POINT tags -> Computer Use API (parallel with TTS) -> UiEvent::LlmResponse
- **Computer Use**: `api::computer_use::detect_element_location()` — resizes screenshot to best aspect-ratio resolution (1024x768, 1280x800, 1366x768), sends with `computer_20251124` tool, parses `tool_use` response for precise click coordinates. Uses `claude-sonnet-4-6`.
- **TTS pipeline**: LLM spoken_text -> `api::elevenlabs::synthesize_speech()` (fires in parallel with Computer Use from `run_llm_pipeline`) -> UiEvent::TtsAudio -> `AudioPlayer::play_mp3()`. Model: `eleven_multilingual_v2`. espeak-ng fallback on error.
- **Audio player**: `AudioPlayer` struct wraps rodio — `play_mp3(bytes)`, `is_playing()`, `stop()`
- **Flight animation**: Forward flight (600px/s, 25% arc, 0.8-1.8s) -> PointingAtTarget (2s hold) -> Return flight (900px/s, 15% arc, 0.4-1.0s) -> FollowingMouse
- **Cursor navigation modes**: `FollowingMouse` -> `NavigatingToTarget` -> `PointingAtTarget` -> `ReturningToMouse` -> `FollowingMouse`
- **Speech bubble**: Glass effect with triangle's blue tint, custom TTF font (AdwaitaSans), white text, curated friendly phrases via `core::bubble_text::pick_bubble_phrase()`
- **Triangle bloom**: Scaled-up transparent copies of the triangle shape, gentle breathing animation

# Clicky for Linux and Windows

[![CI](https://github.com/tornikegomareli/clicky-desktop/actions/workflows/ci.yml/badge.svg)](https://github.com/tornikegomareli/clicky-desktop/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/tornikegomareli/clicky-desktop)](https://github.com/tornikegomareli/clicky-desktop/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-linux%20%7C%20windows-brightgreen.svg)]()

Cross-platform implementation of [Clicky](https://github.com/farzaa/clicky) for Linux and Windows users, an AI companion that lives in your system tray, listens to your voice, looks at your screen, educates you and points at things.

Hold the push-to-talk hotkey, ask a question, release. Clicky can see your screen, sends it with your transcript to Claude, speaks the answer back, and flies a blue triangle cursor to the UI element it's referencing.

Built with Rust and Raylib. Runs on Linux and Windows.

> This is an early release. I've tested heavily on Linux (Hyprland, X11) and Windows, but there's a good chance I missed something. If you hit a problem, [open an issue](https://github.com/tornikegomareli/clicky-desktop/issues) — I'm motivated to spend my evenings fixing bugs and building features.

## How it works

1. Hold the push-to-talk hotkey (Ctrl+Space by default)
2. Speak your question
3. Release — Clicky captures your screen and transcribes your voice
4. Screenshots + transcript go to Claude, which responds about what's on your screen
5. The response is spoken aloud via ElevenLabs
6. A blue cursor flies to the relevant UI element on screen

## API keys

You need keys from three services:

- [Anthropic](https://console.anthropic.com/) — Claude for vision responses and [Computer Use](https://docs.anthropic.com/en/docs/agents-and-tools/computer-use) for precise UI element detection
- [AssemblyAI](https://www.assemblyai.com/) — real-time voice transcription
- [ElevenLabs](https://elevenlabs.io/) — text-to-speech

On first launch, Clicky opens a setup window where you enter these. They're stored locally in your platform's config directory and only sent to their respective APIs.

## Download

Grab the latest release from the [Releases page](https://github.com/tornikegomareli/clicky-desktop/releases).

- **Linux**: extract the tarball, run `./clicky-desktop`
- **Windows**: extract the zip, run `clicky-desktop.exe`

## Building from source

You need the [Rust toolchain](https://rustup.rs/) installed.

### Linux (Debian/Ubuntu)

```bash
sudo apt install pkg-config libgtk-3-dev libasound2-dev libpipewire-0.3-dev \
  libx11-dev libxi-dev libxrandr-dev libxcursor-dev libxinerama-dev \
  libgl1-mesa-dev libgbm-dev libwayland-dev libxkbcommon-dev \
  libxdo-dev libudev-dev libssl-dev cmake
```

### Linux (Fedora)

```bash
sudo dnf install gcc pkg-config gtk3-devel alsa-lib-devel pipewire-devel \
  libX11-devel libXi-devel libXrandr-devel libXcursor-devel libXinerama-devel \
  mesa-libGL-devel libgbm-devel wayland-devel libxkbcommon-devel \
  libxdo-devel systemd-devel openssl-devel cmake
```

### Windows

No extra system dependencies — just the Rust MSVC toolchain.

### Build and run

```bash
git clone https://github.com/tornikegomareli/clicky-desktop
cd clicky-desktop
cargo build --release
./target/release/clicky-desktop
```

## Hyprland users

Add these window rules to your `hyprland.conf` for proper overlay behavior:

```
windowrulev2 = float, class:clicky-overlay
windowrulev2 = pin, class:clicky-overlay
windowrulev2 = nofocus, class:clicky-overlay
windowrulev2 = noshadow, class:clicky-overlay
windowrulev2 = noborder, class:clicky-overlay
windowrulev2 = noanim, class:clicky-overlay
```

## Debug flags

```bash
CLICKY_SIMULATE=1 cargo run          # test animations without API calls
CLICKY_FORCE_SETUP_WINDOW=1 cargo run # re-open the setup window
```

## Credits

This is a cross-platform implementation of the original [Clicky](https://github.com/farzaa/clicky) by [@farzaa](https://github.com/farzaa), which is a macOS-native app built with Swift.
## License

[MIT](LICENSE)

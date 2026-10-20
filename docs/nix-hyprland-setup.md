# Running Clicky Desktop on NixOS & Hyprland

This guide provides complete instructions for setting up, running, and configuring **Clicky Desktop** on NixOS / Nix with the Hyprland Wayland compositor.

---

## 1. NixOS System Configuration

Clicky uses `evdev` to capture global hotkeys and mouse coordinates directly from `/dev/input/` under Wayland, bypassing XWayland limitations.

Add your user to the `input` group in `/etc/nixos/configuration.nix`:

```nix
users.users.<your_username> = {
  isNormalUser = true;
  extraGroups = [ "wheel" "input" "video" "audio" ];
};
```

Apply the changes:

```bash
sudo nixos-rebuild switch
```

> **Important**: You must log out and log back in (or reboot) for group permissions on `/dev/input/` to take effect.

---

## 2. Running Clicky

### Option A: Direct Run via Flake (Recommended)

Run directly without manual compilation:

```bash
# From within the cloned repository:
nix run

# Or directly from GitHub:
nix run github:tornikegomareli/clicky-desktop
```

### Option B: Adding to NixOS / Home Manager Flake

In your system's `flake.nix`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    clicky-desktop.url = "github:tornikegomareli/clicky-desktop";
  };

  outputs = { self, nixpkgs, clicky-desktop, ... }: {
    nixosConfigurations.myhostname = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ({ pkgs, ... }: {
          environment.systemPackages = [
            clicky-desktop.packages.${pkgs.system}.default
            pkgs.grim # For Wayland screen capture
          ];
        })
      ];
    };
  };
}
```

### Option C: Development Shell

If you are modifying or hacking on the codebase:

```bash
nix develop
# or: nix-shell

cargo run
```

---

## 3. Hyprland Configuration (`hyprland.conf`)

Add the following rules and shortcuts to your `~/.config/hypr/hyprland.conf`:

### Window Rules for Overlay

Clicky creates a transparent Raylib overlay window. These rules ensure it behaves strictly as a non-intrusive floating overlay and does not capture window focus, draw borders, or trigger animations:

```ini
# Clicky transparent overlay rules
windowrulev2 = float, class:clicky-overlay
windowrulev2 = pin, class:clicky-overlay
windowrulev2 = nofocus, class:clicky-overlay
windowrulev2 = noshadow, class:clicky-overlay
windowrulev2 = noborder, class:clicky-overlay
windowrulev2 = noanim, class:clicky-overlay
```

---

## 4. Emergency Kill Switch (si Clicky se vuelve loco)

If Clicky ever hangs, enters a loop, or gets stuck pointing or rendering, compositor-level shortcuts bypass the application event loop completely.

### Method 1: Hyprland Keybinding (Instant SIGKILL)

Add an emergency kill keybind to your `hyprland.conf`:

```ini
# Emergency kill-switch: Super + Shift + Escape kills Clicky immediately
bind = $mainMod SHIFT, Escape, exec, pkill -9 -f clicky-desktop
```

### Method 2: Dedicated Helper Script

You can also bind the included script [scripts/kill-clicky.sh](../scripts/kill-clicky.sh), which sends a graceful `SIGTERM` first, waits 1 second, and escalates to `SIGKILL (-9)` if necessary, showing a desktop notification:

```ini
bind = $mainMod SHIFT, Escape, exec, ~/.config/hypr/scripts/kill-clicky.sh
# or from repo root:
# bind = $mainMod SHIFT, Escape, exec, /path/to/clicky-desktop/scripts/kill-clicky.sh
```

Or run it directly from any terminal or launcher (`rofi`/`wofi`):

```bash
./scripts/kill-clicky.sh
```

---

## 5. Free & Local AI Providers Setup

Clicky can run completely free or offline using open models, local GPUs (NVIDIA/CUDA), or free API tiers without requiring Anthropic, AssemblyAI, or ElevenLabs.

You can configure them via the graphical Settings window (`CLICKY_FORCE_SETUP_WINDOW=1 nix run` or via tray icon) or directly in `~/.config/clicky-desktop/config.toml`.

### A. Local NVIDIA GPU Whisper (STT)

If you have an NVIDIA GPU, you can run a local Whisper server with CUDA acceleration (such as `faster-whisper-server` or `whisper.cpp`):

```bash
# Example running faster-whisper-server with CUDA:
docker run --gpus all -p 8000:8000 fedirz/faster-whisper-server:latest-cuda
```

In `config.toml` (or Clicky Settings):
```toml
stt_provider = "openaicompat"
custom_stt_base_url = "http://localhost:8000/v1"
custom_stt_model = "Systran/faster-whisper-large-v3"
```

### B. Free Cloud Whisper (Groq)

Transcribes in ~300ms with a free API key from [Groq](https://console.groq.com/):

```toml
stt_provider = "groq"
groq_api_key = "gsk_..."
```

### C. Local Ollama (Vision LLM)

Run local multimodal vision models on your GPU:

```bash
ollama run llama3.2-vision
# or: ollama run qwen2.5-vl
```

In `config.toml`:
```toml
llm_provider = "ollama"
custom_llm_base_url = "http://localhost:11434/v1"
custom_llm_model = "llama3.2-vision"
```

### D. Free OpenRouter / Gemini (Vision LLM)

Use free multimodal vision models via OpenRouter or Google AI Studio:

```toml
# OpenRouter (free tier)
llm_provider = "openrouter"
custom_llm_api_key = "sk-or-..."
custom_llm_model = "google/gemini-2.0-flash-exp:free"

# Google AI Studio / Gemini (free tier)
# llm_provider = "gemini"
# custom_llm_api_key = "AIzaSy..."
# custom_llm_model = "gemini-2.0-flash"
```

### E. Free High-Fidelity TTS (Edge-TTS)

Uses Microsoft Edge's natural neural voices without needing any API key:

```toml
tts_provider = "edgetts"
custom_tts_voice = "es-ES-AlvaroNeural" # or "en-US-ChristopherNeural"
```

---

## 6. Troubleshooting & FAQ

| Problem | Cause | Solution |
| :--- | :--- | :--- |
| **`evdev tracker failed: Permission denied`** | User is not in the `input` group or session was not reloaded. | Verify with `groups`. Ensure `extraGroups = [ "input" ];` in NixOS and re-login. |
| **Screenshots are blank or fail** | `grim` is missing. | Install `pkgs.grim` in NixOS or run via the flake package which bundles `grim`. |
| **Overlay window steals focus** | Hyprland `nofocus` rule is not matching. | Check window class using `hyprctl clients` and ensure `windowrulev2 = nofocus, class:clicky-overlay` is active. |
| **Push-to-talk hotkey doesn't trigger** | Keyboard device was not detected by evdev. | Check Clicky logs with `RUST_LOG=info clicky-desktop` to see which device was selected. |

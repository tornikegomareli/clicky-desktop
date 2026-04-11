#!/usr/bin/env bash
set -euo pipefail

# Package the Linux release binary into a distributable tarball.
# Expected to run after `cargo build --release` in CI or locally.

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
ARCHIVE_NAME="clicky-desktop-v${VERSION}-linux-x86_64"
DIST_DIR="dist"
STAGING_DIR="${DIST_DIR}/${ARCHIVE_NAME}"

echo "Packaging ${ARCHIVE_NAME}..."

rm -rf "${STAGING_DIR}"
mkdir -p "${STAGING_DIR}"

# Binary
cp target/release/clicky-desktop "${STAGING_DIR}/"

# Desktop entry and icon
cp assets/clicky-desktop.desktop "${STAGING_DIR}/"
cp assets/icon-256.png "${STAGING_DIR}/clicky-desktop.png"
cp assets/icon.svg "${STAGING_DIR}/clicky-desktop.svg"

# Install instructions
cat > "${STAGING_DIR}/INSTALL.md" << 'INSTALL_EOF'
# Installing Clicky Desktop on Linux

## Quick start

```bash
# Run directly
./clicky-desktop
```

## System install

```bash
# Copy binary
sudo cp clicky-desktop /usr/local/bin/

# Copy icon
sudo cp clicky-desktop.svg /usr/share/icons/hicolor/scalable/apps/
sudo cp clicky-desktop.png /usr/share/icons/hicolor/256x256/apps/

# Copy desktop entry
sudo cp clicky-desktop.desktop /usr/share/applications/

# Update icon cache
sudo gtk-update-icon-cache /usr/share/icons/hicolor/ 2>/dev/null || true
```

## Runtime dependencies

- OpenGL drivers (mesa)
- PulseAudio or PipeWire (for audio capture)
- GTK 3 (for system tray)
- `grim` (for Wayland screenshot capture, optional)

On Fedora/RHEL:
```bash
sudo dnf install mesa-dri-drivers pipewire gtk3 grim
```

On Debian/Ubuntu:
```bash
sudo apt install mesa-utils libgtk-3-0 grim
```

## Hyprland users

Add to your `hyprland.conf`:
```
windowrulev2 = float, class:clicky-overlay
windowrulev2 = pin, class:clicky-overlay
windowrulev2 = nofocus, class:clicky-overlay
windowrulev2 = noshadow, class:clicky-overlay
windowrulev2 = noborder, class:clicky-overlay
windowrulev2 = noanim, class:clicky-overlay
```

## First run

On first launch Clicky opens a setup window asking for API keys
(Anthropic, AssemblyAI, ElevenLabs). Settings are saved to
`~/.config/clicky-desktop/config.toml`.
INSTALL_EOF

# Create tarball
cd "${DIST_DIR}"
tar czf "${ARCHIVE_NAME}.tar.gz" "${ARCHIVE_NAME}"
rm -rf "${ARCHIVE_NAME}"

echo "Created ${DIST_DIR}/${ARCHIVE_NAME}.tar.gz"

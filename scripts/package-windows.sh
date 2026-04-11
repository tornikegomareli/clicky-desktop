#!/usr/bin/env bash
set -euo pipefail

# Package the Windows release binary into a distributable zip.
# Expected to run after `cargo build --release` in CI or locally.
# Uses bash even on Windows (GitHub Actions provides Git Bash).

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
ARCHIVE_NAME="clicky-desktop-v${VERSION}-windows-x86_64"
DIST_DIR="dist"
STAGING_DIR="${DIST_DIR}/${ARCHIVE_NAME}"

echo "Packaging ${ARCHIVE_NAME}..."

rm -rf "${STAGING_DIR}"
mkdir -p "${STAGING_DIR}"

# Binary
cp target/release/clicky-desktop.exe "${STAGING_DIR}/"

# Icon
cp assets/icon.ico "${STAGING_DIR}/clicky-desktop.ico"

# Install instructions
cat > "${STAGING_DIR}/README.txt" << 'README_EOF'
Clicky Desktop for Windows
==========================

1. Run clicky-desktop.exe
2. On first launch, a setup window will ask for your API keys:
   - Anthropic API key (for Claude vision + responses)
   - AssemblyAI API key (for voice transcription)
   - ElevenLabs API key (for text-to-speech)
3. Settings are saved to %APPDATA%\clicky-desktop\config.toml

Push-to-talk hotkey: Ctrl+Space (configurable in settings)

For more info: https://github.com/tornikegomareli/clicky-desktop
README_EOF

# Create zip
cd "${DIST_DIR}"
if command -v 7z &> /dev/null; then
    7z a "${ARCHIVE_NAME}.zip" "${ARCHIVE_NAME}"
elif command -v zip &> /dev/null; then
    zip -r "${ARCHIVE_NAME}.zip" "${ARCHIVE_NAME}"
else
    # PowerShell fallback on Windows
    powershell -Command "Compress-Archive -Path '${ARCHIVE_NAME}' -DestinationPath '${ARCHIVE_NAME}.zip'"
fi
rm -rf "${ARCHIVE_NAME}"

echo "Created ${DIST_DIR}/${ARCHIVE_NAME}.zip"

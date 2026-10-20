#!/usr/bin/env bash
# Emergency kill-switch for Clicky Desktop
# Usage: ./scripts/kill-clicky.sh or bind to a Hyprland key combination.

set -e

APP_NAME="clicky-desktop"

if ! pgrep -f "$APP_NAME" > /dev/null; then
    echo "Clicky Desktop is not running."
    exit 0
fi

echo "Stopping Clicky Desktop..."

# First try SIGTERM
pkill -15 -f "$APP_NAME" || true

# Wait up to 1 second
for _ in {1..10}; do
    if ! pgrep -f "$APP_NAME" > /dev/null; then
        echo "Clicky Desktop stopped gracefully."
        if command -v notify-send > /dev/null 2>&1; then
            notify-send "Clicky Desktop" "Clicky process terminated gracefully" -i process-stop
        fi
        exit 0
    fi
    sleep 0.1
done

# If still running, force kill with SIGKILL (-9)
echo "Clicky Desktop did not respond. Force killing (SIGKILL)..."
pkill -9 -f "$APP_NAME" || true

if command -v notify-send > /dev/null 2>&1; then
    notify-send "Clicky Desktop" "Clicky was force-killed (SIGKILL)" -u critical -i process-stop
fi

echo "Clicky Desktop killed."

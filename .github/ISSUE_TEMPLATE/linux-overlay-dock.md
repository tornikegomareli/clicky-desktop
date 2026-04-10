---
name: Linux overlay dock visibility
about: Track the unresolved Linux dock/taskbar visibility problem for the overlay window.
title: "Linux overlay still appears in dock or taskbar"
labels: bug, linux
---

## Summary

The transparent overlay window can still appear as a normal application in some Linux desktop environments and compositors.

## Current state

- X11 behavior is not reliably hidden across environments
- Wayland and Hyprland are constrained by the current Raylib/GLFW backend
- the app still works, but the overlay presence in the dock/taskbar is not acceptable product behavior

## Expected result

The overlay should behave like a background utility window rather than a regular dock/taskbar app.

## Investigation notes

- confirm exact behavior on X11, GNOME, KDE, Sway, and Hyprland
- evaluate whether GLFW/Raylib can provide the necessary window-manager hints
- if not, consider a compositor-specific Linux overlay backend or layer-shell approach

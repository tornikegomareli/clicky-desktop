use std::fmt;

/// Identifies the display server protocol in use on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    X11,
    Wayland,
}

/// Identifies the Wayland compositor, if applicable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaylandCompositor {
    Hyprland,
    Sway,
    Other,
}

/// Complete platform information gathered at startup.
/// Used throughout the app to select the correct backend for overlays,
/// hotkeys, screen capture, and cursor tracking.
#[derive(Debug, Clone)]
pub struct PlatformInfo {
    pub os: OperatingSystem,
    pub display_server: Option<DisplayServer>,
    pub wayland_compositor: Option<WaylandCompositor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingSystem {
    Linux,
    Windows,
    MacOS,
}

impl fmt::Display for PlatformInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.os {
            OperatingSystem::Windows => write!(formatter, "Windows"),
            OperatingSystem::MacOS => write!(formatter, "macOS"),
            OperatingSystem::Linux => {
                let display_server_label = match self.display_server {
                    Some(DisplayServer::X11) => "X11",
                    Some(DisplayServer::Wayland) => "Wayland",
                    None => "unknown display server",
                };

                if let Some(compositor) = &self.wayland_compositor {
                    write!(formatter, "Linux ({}, {:?})", display_server_label, compositor)
                } else {
                    write!(formatter, "Linux ({})", display_server_label)
                }
            }
        }
    }
}

/// Detects the current platform at startup by inspecting environment variables
/// and compile-time target OS.
///
/// On Linux, checks `$XDG_SESSION_TYPE` for X11 vs Wayland, and
/// `$HYPRLAND_INSTANCE_SIGNATURE` to identify Hyprland.
pub fn detect() -> PlatformInfo {
    let os = detect_operating_system();
    let display_server = detect_display_server(os);
    let wayland_compositor = detect_wayland_compositor(display_server);

    PlatformInfo {
        os,
        display_server,
        wayland_compositor,
    }
}

fn detect_operating_system() -> OperatingSystem {
    if cfg!(target_os = "linux") {
        OperatingSystem::Linux
    } else if cfg!(target_os = "windows") {
        OperatingSystem::Windows
    } else if cfg!(target_os = "macos") {
        OperatingSystem::MacOS
    } else {
        // Default to Linux for other Unix-like systems
        OperatingSystem::Linux
    }
}

fn detect_display_server(os: OperatingSystem) -> Option<DisplayServer> {
    if os != OperatingSystem::Linux {
        return None;
    }

    // $XDG_SESSION_TYPE is set by the display manager on login
    match std::env::var("XDG_SESSION_TYPE").ok().as_deref() {
        Some("wayland") => Some(DisplayServer::Wayland),
        Some("x11") => Some(DisplayServer::X11),
        _ => {
            // Fallback: check if $WAYLAND_DISPLAY is set (Wayland session)
            // or if $DISPLAY is set (X11 session)
            if std::env::var("WAYLAND_DISPLAY").is_ok() {
                Some(DisplayServer::Wayland)
            } else if std::env::var("DISPLAY").is_ok() {
                Some(DisplayServer::X11)
            } else {
                None
            }
        }
    }
}

fn detect_wayland_compositor(display_server: Option<DisplayServer>) -> Option<WaylandCompositor> {
    if display_server != Some(DisplayServer::Wayland) {
        return None;
    }

    // Hyprland sets $HYPRLAND_INSTANCE_SIGNATURE when running
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return Some(WaylandCompositor::Hyprland);
    }

    // Sway sets $SWAYSOCK
    if std::env::var("SWAYSOCK").is_ok() {
        return Some(WaylandCompositor::Sway);
    }

    Some(WaylandCompositor::Other)
}

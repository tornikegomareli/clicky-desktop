use log::info;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::Icon;
/// System tray icon using the `tray-icon` crate.
/// Cross-platform: works on Windows (shell notification area),
/// Linux (StatusNotifierItem / libappindicator), and macOS.
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Events emitted by the system tray menu.
#[derive(Debug, Clone)]
pub enum TrayMenuEvent {
    ToggleOverlay,
    OpenSettings,
    Quit,
}

/// Manages the system tray icon and menu.
pub struct ClickyTrayIcon {
    _tray_icon: TrayIcon,
    toggle_overlay_menu_item_id: tray_icon::menu::MenuId,
    settings_menu_item_id: tray_icon::menu::MenuId,
    quit_menu_item_id: tray_icon::menu::MenuId,
}

impl ClickyTrayIcon {
    /// Creates and shows the system tray icon with a context menu.
    pub fn new(needs_onboarding: bool) -> Result<Self, String> {
        let toggle_overlay_item = MenuItem::new("Toggle Overlay", true, None);
        let settings_label = if needs_onboarding {
            "Open Setup"
        } else {
            "Settings"
        };
        let settings_item = MenuItem::new(settings_label, true, None);
        let quit_item = MenuItem::new("Quit", true, None);

        let toggle_overlay_menu_item_id = toggle_overlay_item.id().clone();
        let settings_menu_item_id = settings_item.id().clone();
        let quit_menu_item_id = quit_item.id().clone();

        let tray_menu = Menu::new();
        tray_menu
            .append(&toggle_overlay_item)
            .map_err(|e| e.to_string())?;
        tray_menu
            .append(&settings_item)
            .map_err(|e| e.to_string())?;
        tray_menu.append(&quit_item).map_err(|e| e.to_string())?;

        // Create a simple blue triangle icon (16x16 RGBA)
        let icon = create_blue_triangle_icon();

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Clicky — AI Companion")
            .with_icon(icon)
            .build()
            .map_err(|e| format!("Failed to create tray icon: {}", e))?;

        info!("System tray icon created");

        Ok(Self {
            _tray_icon: tray_icon,
            toggle_overlay_menu_item_id,
            settings_menu_item_id,
            quit_menu_item_id,
        })
    }

    /// Checks for pending menu events and returns the corresponding action.
    /// Should be called each frame from the main loop.
    pub fn poll_menu_event(&self) -> Option<TrayMenuEvent> {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id() == &self.toggle_overlay_menu_item_id {
                return Some(TrayMenuEvent::ToggleOverlay);
            }
            if event.id() == &self.settings_menu_item_id {
                return Some(TrayMenuEvent::OpenSettings);
            }
            if event.id() == &self.quit_menu_item_id {
                return Some(TrayMenuEvent::Quit);
            }
        }
        None
    }
}

/// Creates a 16x16 RGBA icon of a blue triangle (matching the cursor design).
fn create_blue_triangle_icon() -> Icon {
    let width = 16u32;
    let height = 16u32;
    let mut rgba_pixels = vec![0u8; (width * height * 4) as usize];

    // Draw a filled triangle: top-center to bottom-left to bottom-right
    // Using a simple scanline fill
    let top_y = 2;
    let bottom_y = 13;
    let center_x = 8.0f32;

    for y in top_y..=bottom_y {
        let progress = (y - top_y) as f32 / (bottom_y - top_y) as f32;
        let half_width = progress * 6.0;
        let left_x = (center_x - half_width).max(0.0) as u32;
        let right_x = (center_x + half_width).min(width as f32 - 1.0) as u32;

        for x in left_x..=right_x {
            let pixel_index = ((y * width + x) * 4) as usize;
            // Blue: #3380FF
            rgba_pixels[pixel_index] = 51; // R
            rgba_pixels[pixel_index + 1] = 128; // G
            rgba_pixels[pixel_index + 2] = 255; // B
            rgba_pixels[pixel_index + 3] = 255; // A
        }
    }

    Icon::from_rgba(rgba_pixels, width, height).expect("Failed to create tray icon from RGBA data")
}

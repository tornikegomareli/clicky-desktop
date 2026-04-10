#[cfg(target_os = "linux")]
use directories::BaseDirs;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::path::PathBuf;

pub fn is_supported() -> bool {
    cfg!(target_os = "linux") || cfg!(target_os = "windows")
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if !is_supported() {
        let _ = enabled;
        return Err("Autostart is not implemented on this platform yet".into());
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let executable_path =
        std::env::current_exe().map_err(|e| format!("Failed to resolve executable path: {e}"))?;

    #[cfg(target_os = "linux")]
    {
        return set_enabled_linux(enabled, &executable_path);
    }

    #[cfg(target_os = "windows")]
    {
        return set_enabled_windows(enabled, &executable_path);
    }

    #[allow(unreachable_code)]
    Err("Autostart is not implemented on this platform yet".into())
}

#[cfg(target_os = "linux")]
fn set_enabled_linux(enabled: bool, executable_path: &PathBuf) -> Result<(), String> {
    let base_dirs = BaseDirs::new().ok_or_else(|| "Home directory unavailable".to_string())?;
    let autostart_dir = base_dirs.config_dir().join("autostart");
    let desktop_file_path = autostart_dir.join("clicky-desktop.desktop");

    if enabled {
        fs::create_dir_all(&autostart_dir)
            .map_err(|e| format!("Failed to create autostart directory: {e}"))?;
        let desktop_entry = format!(
            "[Desktop Entry]\nType=Application\nVersion=1.0\nName=Clicky\nComment=Clicky Desktop\nExec=\"{}\"\nTerminal=false\nCategories=Utility;\nX-GNOME-Autostart-enabled=true\n",
            executable_path.display()
        );
        fs::write(&desktop_file_path, desktop_entry)
            .map_err(|e| format!("Failed to write autostart desktop entry: {e}"))?;
    } else if desktop_file_path.exists() {
        fs::remove_file(&desktop_file_path)
            .map_err(|e| format!("Failed to remove autostart desktop entry: {e}"))?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn set_enabled_windows(enabled: bool, executable_path: &PathBuf) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) = hkcu
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .map_err(|e| format!("Failed to open Run registry key: {e}"))?;

    if enabled {
        run_key
            .set_value("ClickyDesktop", &executable_path.display().to_string())
            .map_err(|e| format!("Failed to enable autostart: {e}"))?;
    } else {
        let _ = run_key.delete_value("ClickyDesktop");
    }

    Ok(())
}

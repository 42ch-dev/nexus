//! V1.92 P1 — secure client-side storage for the remote connection config.
//!
//! Strategy (daemon-runtime.md §16.5):
//!   1. Try the OS keychain / credential manager (`keyring`).
//!   2. If the keychain is unavailable, fall back to a file in the app data dir.
//!
//! The stored value is a JSON string; the TypeScript side owns the schema.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

const SERVICE: &str = "nexus42";
const USERNAME: &str = "connection_config";
const FALLBACK_FILE: &str = "connection_config.json";

fn fallback_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(FALLBACK_FILE))
}

fn read_fallback(app: &AppHandle) -> Option<String> {
    let path = fallback_path(app)?;
    std::fs::read_to_string(path).ok()
}

fn write_fallback(app: &AppHandle, value: &str) -> Result<(), String> {
    let path = fallback_path(app).ok_or("App data dir unavailable")?;
    std::fs::write(&path, value).map_err(|e| format!("Could not write connection config: {e}"))?;
    Ok(())
}

fn delete_fallback(app: &AppHandle) {
    if let Some(path) = fallback_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// Read the saved connection config JSON, or `None` if never saved.
#[tauri::command]
pub fn get_connection_config(app: AppHandle) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE, USERNAME).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(read_fallback(&app)),
        Err(_e) => {
            // Keychain failed; try fallback file as a last resort. Some
            // keyring backends return a generic error even when empty, so
            // treat a missing fallback as None rather than an error.
            Ok(read_fallback(&app))
        }
    }
}

/// Save the connection config JSON. Writes to keychain when possible, otherwise
/// the app data dir.
#[tauri::command]
pub fn set_connection_config(app: AppHandle, config: String) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, USERNAME).map_err(|e| e.to_string())?;
    match entry.set_password(&config) {
        Ok(()) => {
            // Clean up stale fallback file.
            delete_fallback(&app);
            Ok(())
        }
        Err(_) => {
            // Keychain unavailable; fall back to app-data file.
            write_fallback(&app, &config)
        }
    }
}

/// Delete the saved connection config from both keychain and fallback file.
#[tauri::command]
pub fn delete_connection_config(app: AppHandle) -> Result<(), String> {
    if let Ok(entry) = keyring::Entry::new(SERVICE, USERNAME) {
        let _ = entry.delete_password();
    }
    delete_fallback(&app);
    Ok(())
}

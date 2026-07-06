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

/// Abstraction over the OS credential store so unit tests can substitute a stub.
trait CredentialStore: Send + Sync {
    fn get_password(&self) -> Result<String, keyring::Error>;
    fn set_password(&self, password: &str) -> Result<(), keyring::Error>;
    fn delete_password(&self) -> Result<(), keyring::Error>;
}

struct KeyringStore {
    entry: keyring::Entry,
}

impl KeyringStore {
    fn new(service: &str, username: &str) -> Result<Self, keyring::Error> {
        Ok(Self {
            entry: keyring::Entry::new(service, username)?,
        })
    }
}

impl CredentialStore for KeyringStore {
    fn get_password(&self) -> Result<String, keyring::Error> {
        self.entry.get_password()
    }

    fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
        self.entry.set_password(password)
    }

    fn delete_password(&self) -> Result<(), keyring::Error> {
        self.entry.delete_password()
    }
}

fn fallback_path<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(FALLBACK_FILE))
}

fn read_fallback<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<String> {
    let path = fallback_path(app)?;
    std::fs::read_to_string(path).ok()
}

fn write_fallback<R: tauri::Runtime>(app: &AppHandle<R>, value: &str) -> Result<(), String> {
    let path = fallback_path(app).ok_or("App data dir unavailable")?;
    std::fs::write(&path, value).map_err(|e| format!("Could not write connection config: {e}"))?;
    Ok(())
}

fn delete_fallback<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(path) = fallback_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

fn get_connection_config_inner<R: tauri::Runtime>(
    app: &AppHandle<R>,
    store: &dyn CredentialStore,
) -> Result<Option<String>, String> {
    match store.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(read_fallback(app)),
        Err(_e) => {
            // Keychain failed; try fallback file as a last resort. Some
            // keyring backends return a generic error even when empty, so
            // treat a missing fallback as None rather than an error.
            Ok(read_fallback(app))
        }
    }
}

fn set_connection_config_inner<R: tauri::Runtime>(
    app: &AppHandle<R>,
    config: &str,
    store: &dyn CredentialStore,
) -> Result<(), String> {
    match store.set_password(config) {
        Ok(()) => {
            // Clean up stale fallback file.
            delete_fallback(app);
            Ok(())
        }
        Err(_) => {
            // Keychain unavailable; fall back to app-data file.
            write_fallback(app, config)
        }
    }
}

fn delete_connection_config_inner<R: tauri::Runtime>(
    app: &AppHandle<R>,
    store: &dyn CredentialStore,
) -> Result<(), String> {
    let _ = store.delete_password();
    delete_fallback(app);
    Ok(())
}

/// Read the saved connection config JSON, or `None` if never saved.
#[tauri::command]
pub fn get_connection_config(app: AppHandle) -> Result<Option<String>, String> {
    let store = KeyringStore::new(SERVICE, USERNAME).map_err(|e| e.to_string())?;
    get_connection_config_inner(&app, &store)
}

/// Save the connection config JSON. Writes to keychain when possible, otherwise
/// the app data dir.
#[tauri::command]
pub fn set_connection_config(app: AppHandle, config: String) -> Result<(), String> {
    let store = KeyringStore::new(SERVICE, USERNAME).map_err(|e| e.to_string())?;
    set_connection_config_inner(&app, &config, &store)
}

/// Delete the saved connection config from both keychain and fallback file.
#[tauri::command]
pub fn delete_connection_config(app: AppHandle) -> Result<(), String> {
    if let Ok(store) = KeyringStore::new(SERVICE, USERNAME) {
        let _ = delete_connection_config_inner(&app, &store);
    } else {
        delete_fallback(&app);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Unit tests for connection config storage commands (V1.92 P1).
    //!
    //! These tests stub the OS keychain so they are hermetic and do not touch
    //! the user's real credential store.

    use super::*;
    use std::sync::Mutex;

    /// `mock_app()` returns the same app-data directory across invocations, so
    /// fallback-file tests must run serially to avoid cross-test file races.
    static FALLBACK_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Clone, Copy)]
    enum StubResult {
        Ok,
        Err,
    }

    struct StubStore {
        value: Mutex<Option<String>>,
        get: StubResult,
        set: StubResult,
        delete: StubResult,
    }

    impl StubStore {
        fn new(
            value: Option<String>,
            get: StubResult,
            set: StubResult,
            delete: StubResult,
        ) -> Self {
            Self {
                value: Mutex::new(value),
                get,
                set,
                delete,
            }
        }
    }

    impl CredentialStore for StubStore {
        fn get_password(&self) -> Result<String, keyring::Error> {
            match self.get {
                StubResult::Ok => {
                    let guard = self.value.lock().expect("lock");
                    match guard.as_ref() {
                        Some(v) => Ok(v.clone()),
                        None => Err(keyring::Error::NoEntry),
                    }
                }
                StubResult::Err => Err(keyring::Error::NoEntry),
            }
        }

        fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
            match self.set {
                StubResult::Ok => {
                    let mut guard = self.value.lock().expect("lock");
                    *guard = Some(password.to_owned());
                    Ok(())
                }
                StubResult::Err => Err(keyring::Error::NoEntry),
            }
        }

        fn delete_password(&self) -> Result<(), keyring::Error> {
            match self.delete {
                StubResult::Ok => {
                    let mut guard = self.value.lock().expect("lock");
                    *guard = None;
                    Ok(())
                }
                StubResult::Err => Err(keyring::Error::NoEntry),
            }
        }
    }

    fn mock_app() -> tauri::AppHandle<tauri::test::MockRuntime> {
        tauri::test::mock_app().handle().clone()
    }

    fn cleanup_fallback(app: &tauri::AppHandle<tauri::test::MockRuntime>) {
        if let Some(path) = fallback_path(app) {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn get_returns_keychain_value_when_present() {
        let _guard = FALLBACK_TEST_LOCK.lock().expect("lock");
        let app = mock_app();
        cleanup_fallback(&app);
        let store = StubStore::new(
            Some(r#"{"endpointUrl":"https://x","apiKey":"k"}"#.to_owned()),
            StubResult::Ok,
            StubResult::Ok,
            StubResult::Ok,
        );

        let result = get_connection_config_inner(&app, &store)
            .expect("get should succeed")
            .expect("should have a value");
        assert_eq!(result, r#"{"endpointUrl":"https://x","apiKey":"k"}"#);
        // No fallback file should be created.
        assert!(fallback_path(&app).is_some());
        assert!(!fallback_path(&app).unwrap().exists());
    }

    #[test]
    fn get_reads_fallback_file_when_keychain_has_no_entry() {
        let _guard = FALLBACK_TEST_LOCK.lock().expect("lock");
        let app = mock_app();
        cleanup_fallback(&app);
        let expected = r#"{"endpointUrl":"https://fallback","apiKey":"fk"}"#;
        write_fallback(&app, expected).expect("write fallback");
        let store = StubStore::new(None, StubResult::Ok, StubResult::Ok, StubResult::Ok);

        let result = get_connection_config_inner(&app, &store)
            .expect("get should succeed")
            .expect("should read fallback");
        assert_eq!(result, expected);
    }

    #[test]
    fn set_writes_keychain_and_removes_fallback_file() {
        let _guard = FALLBACK_TEST_LOCK.lock().expect("lock");
        let app = mock_app();
        cleanup_fallback(&app);
        write_fallback(&app, "stale").expect("write fallback");
        let store = StubStore::new(None, StubResult::Ok, StubResult::Ok, StubResult::Ok);

        let config = r#"{"endpointUrl":"https://x","apiKey":"k"}"#;
        set_connection_config_inner(&app, config, &store).expect("set should succeed");

        let value = store.value.lock().expect("lock").clone().expect("stored");
        assert_eq!(value, config);
        assert!(!fallback_path(&app).unwrap().exists());
    }

    #[test]
    fn set_falls_back_to_app_data_dir_when_keychain_is_unavailable() {
        let _guard = FALLBACK_TEST_LOCK.lock().expect("lock");
        let app = mock_app();
        cleanup_fallback(&app);
        let store = StubStore::new(None, StubResult::Ok, StubResult::Err, StubResult::Ok);

        let config = r#"{"endpointUrl":"https://x","apiKey":"k"}"#;
        set_connection_config_inner(&app, config, &store).expect("set should succeed");

        assert!(store.value.lock().expect("lock").is_none());
        let fallback_value = read_fallback(&app).expect("fallback should exist");
        assert_eq!(fallback_value, config);
    }

    #[test]
    fn delete_removes_keychain_and_fallback() {
        let _guard = FALLBACK_TEST_LOCK.lock().expect("lock");
        let app = mock_app();
        cleanup_fallback(&app);
        write_fallback(&app, "fallback").expect("write fallback");
        let store = StubStore::new(
            Some(r#"{"endpointUrl":"https://x","apiKey":"k"}"#.to_owned()),
            StubResult::Ok,
            StubResult::Ok,
            StubResult::Ok,
        );

        delete_connection_config_inner(&app, &store).expect("delete should succeed");

        assert!(store.value.lock().expect("lock").is_none());
        assert!(!fallback_path(&app).unwrap().exists());
    }
}

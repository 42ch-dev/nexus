//! V1.92 P1 — secure client-side storage for the remote connection config.
//!
//! Strategy (daemon-runtime.md §16.5):
//!   1. Try the OS keychain / credential manager (`keyring`).
//!   2. If the keychain is unavailable, fall back to a file in the app data dir.
//!
//! The stored value is a JSON string; the TypeScript side owns the schema.
//!
//! keyring 4 note: `Entry::new` forces one-time native-store initialization;
//! on Linux this connects to the Secret Service at construction time, so a
//! missing/unreachable store surfaces as a CONSTRUCTOR error (`NoDefaultStore`,
//! cached for the process), not only as an operation error. Constructor
//! unavailability therefore routes through the same app-data fallback as
//! operation unavailability below.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

const SERVICE: &str = "nexus42";
const USERNAME: &str = "connection_config";
const FALLBACK_FILE: &str = "connection_config.json";

/// Abstraction over the OS credential store so unit tests can substitute a stub.
trait CredentialStore: Send + Sync {
    fn get_password(&self) -> Result<String, keyring::Error>;
    fn set_password(&self, password: &str) -> Result<(), keyring::Error>;
    fn delete_credential(&self) -> Result<(), keyring::Error>;
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

    fn delete_credential(&self) -> Result<(), keyring::Error> {
        self.entry.delete_credential()
    }
}

/// Production fallback resolver: the app-data dir (created on demand) plus the
/// fallback file basename. Tests never call this — they inject a
/// `tempfile::TempDir`-rooted path so no test touches the real
/// Application Support directory.
fn fallback_path<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(FALLBACK_FILE))
}

fn read_fallback(path: Option<&Path>) -> Option<String> {
    std::fs::read_to_string(path?).ok()
}

fn write_fallback(path: Option<&Path>, value: &str) -> Result<(), String> {
    let path = path.ok_or("App data dir unavailable")?;
    std::fs::write(path, value).map_err(|e| format!("Could not write connection config: {e}"))?;
    Ok(())
}

fn delete_fallback(path: Option<&Path>) {
    if let Some(path) = path {
        let _ = std::fs::remove_file(path);
    }
}

/// `store` is `None` when the native store could not be constructed at all
/// (keyring 4 constructor-time initialization failure) — the same fallback
/// decision as an operation-level unavailability.
fn get_connection_config_inner(
    store: Option<&dyn CredentialStore>,
    fallback: Option<&Path>,
) -> Result<Option<String>, String> {
    let native = match store {
        Some(store) => store.get_password(),
        None => Err(keyring::Error::NoEntry),
    };
    match native {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(read_fallback(fallback)),
        Err(_) => {
            // Keychain failed; try fallback file as a last resort. Some
            // keyring backends return a generic error even when empty, so
            // treat a missing fallback as None rather than an error.
            Ok(read_fallback(fallback))
        }
    }
}

fn set_connection_config_inner(
    store: Option<&dyn CredentialStore>,
    fallback: Option<&Path>,
    config: &str,
) -> Result<(), String> {
    let native = match store {
        Some(store) => store.set_password(config),
        None => Err(keyring::Error::NoEntry),
    };
    match native {
        Ok(()) => {
            // Clean up stale fallback file.
            delete_fallback(fallback);
            Ok(())
        }
        Err(_) => {
            // Keychain unavailable; fall back to app-data file.
            write_fallback(fallback, config)
        }
    }
}

fn delete_connection_config_inner(
    store: Option<&dyn CredentialStore>,
    fallback: Option<&Path>,
) -> Result<(), String> {
    // Best-effort native delete when the store is available; always clean up
    // the fallback file. Never claims native deletion succeeded when access
    // was denied — the result is intentionally discarded either way.
    if let Some(store) = store {
        let _ = store.delete_credential();
    }
    delete_fallback(fallback);
    Ok(())
}

/// Read the saved connection config JSON, or `None` if never saved.
#[tauri::command]
pub fn get_connection_config(app: AppHandle) -> Result<Option<String>, String> {
    // Constructor failure (e.g. no Secret Service on Linux) is not an error:
    // the fallback file still serves the config.
    let store = KeyringStore::new(SERVICE, USERNAME).ok();
    let fallback = fallback_path(&app);
    get_connection_config_inner(
        store.as_ref().map(|s| s as &dyn CredentialStore),
        fallback.as_deref(),
    )
}

/// Save the connection config JSON. Writes to keychain when possible, otherwise
/// the app data dir.
#[tauri::command]
pub fn set_connection_config(app: AppHandle, config: String) -> Result<(), String> {
    let store = KeyringStore::new(SERVICE, USERNAME).ok();
    let fallback = fallback_path(&app);
    set_connection_config_inner(
        store.as_ref().map(|s| s as &dyn CredentialStore),
        fallback.as_deref(),
        &config,
    )
}

/// Delete the saved connection config from both keychain and fallback file.
#[tauri::command]
pub fn delete_connection_config(app: AppHandle) -> Result<(), String> {
    let store = KeyringStore::new(SERVICE, USERNAME).ok();
    let fallback = fallback_path(&app);
    delete_connection_config_inner(
        store.as_ref().map(|s| s as &dyn CredentialStore),
        fallback.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    //! Unit tests for connection config storage commands (V1.92 P1).
    //!
    //! These tests stub the OS keychain AND inject a `tempfile::TempDir`-rooted
    //! fallback path, so they are hermetic: they never touch the user's real
    //! credential store or the real Application Support directory. (The
    //! previous mock_app-based fallback resolution could hit the real
    //! app-data root; the path-injection seam removes that hazard, so no
    //! serializing mutex or post-test cleanup of a shared path is needed.)

    use super::*;

    #[derive(Clone, Copy)]
    enum StubResult {
        Ok,
        Err,
        ErrOther,
    }

    struct StubStore {
        value: Mutex<Option<String>>,
        get: StubResult,
        set: StubResult,
        delete: StubResult,
    }

    use std::sync::Mutex;

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
                StubResult::ErrOther => Err(keyring::Error::PlatformFailure(Box::new(
                    std::io::Error::other("keychain unavailable"),
                ))),
            }
        }

        fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
            match self.set {
                StubResult::Ok => {
                    let mut guard = self.value.lock().expect("lock");
                    *guard = Some(password.to_owned());
                    Ok(())
                }
                StubResult::Err | StubResult::ErrOther => Err(keyring::Error::NoEntry),
            }
        }

        fn delete_credential(&self) -> Result<(), keyring::Error> {
            match self.delete {
                StubResult::Ok => {
                    let mut guard = self.value.lock().expect("lock");
                    *guard = None;
                    Ok(())
                }
                StubResult::Err | StubResult::ErrOther => Err(keyring::Error::NoEntry),
            }
        }
    }

    /// An isolated fallback file inside a fresh TempDir (the TempDir must
    /// outlive the test; the returned path includes the fallback basename).
    struct IsolatedFallback {
        _dir: tempfile::TempDir,
        path: PathBuf,
    }

    fn isolated_fallback() -> IsolatedFallback {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join(FALLBACK_FILE);
        IsolatedFallback { _dir: dir, path }
    }

    #[test]
    fn get_returns_keychain_value_when_present() {
        let isolated = isolated_fallback();
        // A stale fallback exists but the native value wins and the fallback
        // is NOT rewritten or deleted on read.
        write_fallback(Some(&isolated.path), "stale").expect("write fallback");
        let store = StubStore::new(
            Some(r#"{"endpointUrl":"https://x","apiKey":"k"}"#.to_owned()),
            StubResult::Ok,
            StubResult::Ok,
            StubResult::Ok,
        );

        let result = get_connection_config_inner(Some(&store), Some(&isolated.path))
            .expect("get should succeed")
            .expect("should have a value");
        assert_eq!(result, r#"{"endpointUrl":"https://x","apiKey":"k"}"#);
        // Native-value precedence: the stale fallback is left untouched.
        assert_eq!(
            read_fallback(Some(&isolated.path)).as_deref(),
            Some("stale")
        );
    }

    #[test]
    fn get_reads_fallback_file_when_keychain_has_no_entry() {
        let isolated = isolated_fallback();
        let expected = r#"{"endpointUrl":"https://fallback","apiKey":"fk"}"#;
        write_fallback(Some(&isolated.path), expected).expect("write fallback");
        let store = StubStore::new(None, StubResult::Ok, StubResult::Ok, StubResult::Ok);

        let result = get_connection_config_inner(Some(&store), Some(&isolated.path))
            .expect("get should succeed")
            .expect("should read fallback");
        assert_eq!(result, expected);
    }

    #[test]
    fn get_reads_fallback_file_when_constructor_is_unavailable() {
        // keyring 4: a failed native-store initialization is cached and every
        // Entry::new fails — the read must still serve the fallback file.
        let isolated = isolated_fallback();
        let expected = r#"{"endpointUrl":"https://fallback","apiKey":"fk"}"#;
        write_fallback(Some(&isolated.path), expected).expect("write fallback");

        let result = get_connection_config_inner(None, Some(&isolated.path))
            .expect("get should succeed")
            .expect("should read fallback");
        assert_eq!(result, expected);
    }

    #[test]
    fn get_returns_none_when_constructor_is_unavailable_and_fallback_is_missing() {
        let isolated = isolated_fallback();
        let result =
            get_connection_config_inner(None, Some(&isolated.path)).expect("get should succeed");
        assert!(result.is_none());
    }

    #[test]
    fn set_writes_keychain_and_removes_fallback_file() {
        let isolated = isolated_fallback();
        write_fallback(Some(&isolated.path), "stale").expect("write fallback");
        let store = StubStore::new(None, StubResult::Ok, StubResult::Ok, StubResult::Ok);

        let config = r#"{"endpointUrl":"https://x","apiKey":"k"}"#;
        set_connection_config_inner(Some(&store), Some(&isolated.path), config)
            .expect("set should succeed");

        let value = store.value.lock().expect("lock").clone().expect("stored");
        assert_eq!(value, config);
        assert!(!isolated.path.exists());
    }

    #[test]
    fn set_falls_back_to_app_data_dir_when_keychain_is_unavailable() {
        let isolated = isolated_fallback();
        let store = StubStore::new(None, StubResult::Ok, StubResult::Err, StubResult::Ok);

        let config = r#"{"endpointUrl":"https://x","apiKey":"k"}"#;
        set_connection_config_inner(Some(&store), Some(&isolated.path), config)
            .expect("set should succeed");

        assert!(store.value.lock().expect("lock").is_none());
        let fallback_value = read_fallback(Some(&isolated.path)).expect("fallback should exist");
        assert_eq!(fallback_value, config);
    }

    #[test]
    fn set_falls_back_to_app_data_dir_when_constructor_is_unavailable() {
        let isolated = isolated_fallback();

        let config = r#"{"endpointUrl":"https://x","apiKey":"k"}"#;
        set_connection_config_inner(None, Some(&isolated.path), config)
            .expect("set should succeed");

        let fallback_value = read_fallback(Some(&isolated.path)).expect("fallback should exist");
        assert_eq!(fallback_value, config);
    }

    #[test]
    fn set_surfaces_error_when_fallback_path_is_unavailable() {
        // Constructor unavailable AND no app-data dir: the write error must
        // surface — never report successful persistence from nowhere.
        let err = set_connection_config_inner(None, None, r#"{"k":"v"}"#)
            .expect_err("set must fail without any persistence target");
        assert!(err.contains("App data dir unavailable"), "{err}");
    }

    #[test]
    fn set_surfaces_fallback_write_error() {
        // Operation failure + unwritable fallback path (a directory where the
        // file should be) → the I/O error surfaces.
        let dir = tempfile::TempDir::new().expect("tempdir");
        let blocking_dir = dir.path().join(FALLBACK_FILE);
        std::fs::create_dir(&blocking_dir).expect("create blocking dir");
        let store = StubStore::new(None, StubResult::Ok, StubResult::Err, StubResult::Ok);

        let err = set_connection_config_inner(Some(&store), Some(&blocking_dir), r#"{"k":"v"}"#)
            .expect_err("set must surface the fallback write error");
        assert!(err.contains("Could not write connection config"), "{err}");
        // The native store was not written either.
        assert!(store.value.lock().expect("lock").is_none());
    }

    #[test]
    fn delete_removes_keychain_and_fallback() {
        let isolated = isolated_fallback();
        write_fallback(Some(&isolated.path), "fallback").expect("write fallback");
        let store = StubStore::new(
            Some(r#"{"endpointUrl":"https://x","apiKey":"k"}"#.to_owned()),
            StubResult::Ok,
            StubResult::Ok,
            StubResult::Ok,
        );

        delete_connection_config_inner(Some(&store), Some(&isolated.path))
            .expect("delete should succeed");

        assert!(store.value.lock().expect("lock").is_none());
        assert!(!isolated.path.exists());
    }

    #[test]
    fn delete_cleans_fallback_when_constructor_is_unavailable() {
        let isolated = isolated_fallback();
        write_fallback(Some(&isolated.path), "fallback").expect("write fallback");

        delete_connection_config_inner(None, Some(&isolated.path)).expect("delete should succeed");

        assert!(!isolated.path.exists());
    }

    #[test]
    fn delete_cleans_fallback_even_when_native_delete_fails() {
        let isolated = isolated_fallback();
        write_fallback(Some(&isolated.path), "fallback").expect("write fallback");
        let store = StubStore::new(
            Some(r#"{"endpointUrl":"https://x","apiKey":"k"}"#.to_owned()),
            StubResult::Ok,
            StubResult::Ok,
            StubResult::Err,
        );

        // Best-effort contract: native failure does not fail the command and
        // the fallback is still cleaned up.
        delete_connection_config_inner(Some(&store), Some(&isolated.path))
            .expect("delete should succeed");
        assert!(!isolated.path.exists());
    }

    #[test]
    fn get_returns_none_when_keychain_fails_generically_and_fallback_is_missing() {
        let isolated = isolated_fallback();
        let store = StubStore::new(None, StubResult::ErrOther, StubResult::Ok, StubResult::Ok);

        let result = get_connection_config_inner(Some(&store), Some(&isolated.path))
            .expect("get should succeed");
        assert!(
            result.is_none(),
            "a missing fallback after a keychain error should return None"
        );
    }
}

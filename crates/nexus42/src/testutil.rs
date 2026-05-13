//! Test utilities for nexus42 CLI tests.
//!
//! Provides helpers for test isolation, including temporary HOME directory
//! management to prevent race conditions under parallel test execution.

/// RAII guard that sets `HOME` to a temp directory for the duration of the test.
///
/// On macOS, `dirs::home_dir()` does NOT respect `$HOME` in all configurations,
/// so this helper uses `std::env::set_var("HOME", ...)` which works for our
/// internal `user_home_dir()` / `nexus_home_dir()` functions.
///
/// # Panics
///
/// Panics if a temporary directory cannot be created.
///
/// # Example
///
/// ```ignore
/// let _home = isolated_home();
/// // HOME is now set to a temp dir; will be restored on drop
/// ```
pub fn isolated_home() -> IsolatedHome {
    let tmp = tempfile::TempDir::new().expect("tempdir for test");
    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", tmp.path());
    IsolatedHome {
        _tmp: tmp,
        original_home,
    }
}

/// RAII guard that restores `HOME` when dropped.
pub struct IsolatedHome {
    _tmp: tempfile::TempDir,
    original_home: Option<String>,
}

impl Drop for IsolatedHome {
    fn drop(&mut self) {
        if let Some(home) = &self.original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn isolated_home_sets_home_to_temp_dir() {
        let guard = isolated_home();
        let home = std::env::var("HOME").expect("HOME should be set");
        let home_path = PathBuf::from(&home);
        assert!(
            home_path.exists(),
            "HOME should point to an existing directory"
        );
        assert!(
            home_path != dirs::home_dir().unwrap_or_default() || home_path.starts_with("/tmp")
                || home_path.starts_with("/var/folders"),
            "HOME should be a temp directory, not the real home"
        );
        // Temp dir should still exist while guard is alive
        assert!(home_path.is_dir());
        drop(guard);
    }

    #[test]
    fn isolated_home_restores_original_home_on_drop() {
        let original = std::env::var("HOME").ok();
        {
            let _guard = isolated_home();
            // HOME is changed inside this block
            let during = std::env::var("HOME").expect("HOME should be set");
            assert_ne!(during, original.clone().unwrap_or_default());
        }
        // After drop, HOME should be restored
        let after = std::env::var("HOME").ok();
        assert_eq!(after, original, "HOME should be restored to its original value");
    }
}

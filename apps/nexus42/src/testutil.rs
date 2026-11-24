//! Test utilities for nexus42 CLI tests.
//!
//! Provides helpers for test isolation, including temporary HOME directory
//! management to prevent race conditions under parallel test execution.

use std::sync::Mutex;

/// Global mutex to serialize HOME environment variable access.
///
/// Since `HOME` is a process-wide variable, concurrent tests that each set it
/// to different temp directories will race. This mutex ensures only one
/// `IsolatedHome` guard is active at a time, preventing flaky failures under
/// `--test-threads>1`.
static HOME_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard that sets `HOME` (and `USERPROFILE` on Windows) to a temp directory
/// for the duration of the test.
///
/// On macOS, `dirs::home_dir()` does NOT respect `$HOME` in all configurations,
/// so this helper uses `std::env::set_var("HOME", ...)` which works for our
/// internal `user_home_dir()` / `nexus_home_dir()` functions. `USERPROFILE` is
/// moved in lockstep because Windows `dirs::home_dir()` resolves through it.
///
/// Uses a global mutex to serialize access — only one `IsolatedHome` is active
/// at a time, preventing race conditions under `--test-threads>1`.
///
/// # Panics
///
/// Panics if a temporary directory cannot be created or the `HOME_LOCK` mutex is poisoned.
///
/// # Example
///
/// ```ignore
/// let _home = isolated_home();
/// // HOME is now set to a temp dir; will be restored on drop
/// ```
pub fn isolated_home() -> IsolatedHome {
    let lock = HOME_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    IsolatedHome::locked(lock)
}

impl IsolatedHome {
    /// Build a guard that owns the process-wide HOME lock.
    fn locked(lock: std::sync::MutexGuard<'static, ()>) -> Self {
        Self::install(Some(lock))
    }

    /// Build a guard while the CALLER already holds [`HOME_LOCK`].
    ///
    /// Exists so a test can keep the lock across "restore on drop" and its
    /// assertion: `HOME` is process-wide, so a post-drop assertion is only
    /// sound while no sibling test can take the lock. Callers must hold the
    /// lock for the guard's whole lifetime.
    fn with_caller_holding_lock() -> Self {
        Self::install(None)
    }

    fn install(lock: Option<std::sync::MutexGuard<'static, ()>>) -> Self {
        let tmp = tempfile::TempDir::new().expect("tempdir for test");
        let original_home = std::env::var("HOME").ok();
        let original_userprofile = std::env::var("USERPROFILE").ok();
        std::env::set_var("HOME", tmp.path());
        // Windows `dirs::home_dir()` resolves through `USERPROFILE`, so moving
        // only `HOME` would let tests touch the real `~/.nexus42` there.
        std::env::set_var("USERPROFILE", tmp.path());
        Self {
            _tmp: tmp,
            original_home,
            original_userprofile,
            _lock: lock,
        }
    }
}

/// RAII guard that restores `HOME` when dropped.
pub struct IsolatedHome {
    _tmp: tempfile::TempDir,
    original_home: Option<String>,
    original_userprofile: Option<String>,
    /// Held until drop to serialize HOME manipulation across test threads.
    /// `None` when the caller retains ownership of the lock itself.
    _lock: Option<std::sync::MutexGuard<'static, ()>>,
}

impl IsolatedHome {
    /// Temporary home directory currently installed as `HOME` / `USERPROFILE`.
    ///
    /// Use it to assert that a resolved path stays inside the isolated home.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        self._tmp.path()
    }
}

impl Drop for IsolatedHome {
    fn drop(&mut self) {
        if let Some(home) = &self.original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(profile) = &self.original_userprofile {
            std::env::set_var("USERPROFILE", profile);
        } else {
            std::env::remove_var("USERPROFILE");
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
            home_path != dirs::home_dir().unwrap_or_default()
                || home_path.starts_with("/tmp")
                || home_path.starts_with("/var/folders"),
            "HOME should be a temp directory, not the real home"
        );
        assert!(home_path.is_dir());
        drop(guard);
    }

    #[test]
    fn isolated_home_restores_original_home_on_drop() {
        let lock = HOME_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let original = std::env::var("HOME").ok();
        let original_profile = std::env::var("USERPROFILE").ok();
        {
            let _guard = IsolatedHome::with_caller_holding_lock();
            let during = std::env::var("HOME").expect("HOME should be set");
            let during_profile = std::env::var("USERPROFILE").expect("USERPROFILE should be set");
            assert_ne!(during, original.clone().unwrap_or_default());
            assert_ne!(during_profile, original_profile.clone().unwrap_or_default());
            assert_eq!(during, during_profile, "both home variables move together");
        }
        assert_eq!(
            std::env::var("HOME").ok(),
            original,
            "HOME should be restored to its original value"
        );
        assert_eq!(
            std::env::var("USERPROFILE").ok(),
            original_profile,
            "USERPROFILE should be restored to its original value"
        );
        drop(lock);
    }
}

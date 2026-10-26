//! I-000065: the daemon-home `config.toml` advisory lock facade.
//!
//! The protocol itself lives in [`nexus_home_layout::config_lock`] — the lowest
//! crate every writer can depend on, which is also where the fifth writer
//! (`ensure_system_layout`) lives. This module only maps lock failures onto the
//! core error carrier so every core writer keeps its original call shape.

use std::path::Path;

use crate::error::{CoreError, CoreResult};

/// Run one `config.toml` read-modify-write under the daemon-home advisory
/// lock.
///
/// Every writer of this file takes it — the service
/// (`apps/nexus-service/src/agent-profile.ts` `writeAgentProfile`), the
/// desktop main (`apps/desktop-electron/src/desktop-config.ts`
/// `withConfigLock`), the CLI (`apps/nexus42/src/config.rs`
/// `CliConfig::mutate`, including its legacy-JSON migration) and the startup
/// skeleton (`nexus_home_layout::ensure_system_layout`) use the same
/// `<daemon home>/config.toml.lock` protocol, so the lock excludes writers in
/// other processes, not just in this one.
///
/// # Errors
/// Returns the mapped internal carrier when the lock cannot be taken, then
/// whatever `run` returns.
pub fn with_config_lock<T>(
    nexus_home: &Path,
    run: impl FnOnce() -> CoreResult<T>,
) -> CoreResult<T> {
    let _lock = nexus_home_layout::config_lock::acquire(nexus_home).map_err(|error| {
        CoreError::Internal {
            category: format!("config_lock: {error}"),
        }
    })?;
    run()
}

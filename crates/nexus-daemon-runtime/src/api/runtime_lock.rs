//! Runtime lock helper for mutating HTTP handlers.
//!
//! Provides a single canonical [`RuntimeLockGuard`] so the per-Work
//! single-writer contract is enforced consistently across handlers.

use crate::api::errors::NexusApiError;
use nexus_local_db::SqlitePool;

/// RAII guard that acquires a runtime lock on creation.
///
/// **Important:** this guard does **not** release the lock on `Drop` — `Drop`
/// only logs a warning because async release is not possible in a synchronous
/// `Drop`. Callers must explicitly call [`RuntimeLockGuard::release`] on every
/// exit path. See the crate `AGENTS.md` "Runtime Lock Acquire / Release Order"
/// rule for the mandatory pattern.
///
/// Spec: `novel-writing/multi-work-lifecycle.md` §4.2 — CLI holder format
/// `cli:<caller>:<uuid>`. For HTTP callers, `caller` is `http` since the
/// actual PID isn't available over the API.
pub struct RuntimeLockGuard {
    pool: SqlitePool,
    creator_id: String,
    work_id: String,
    holder: String,
    /// Whether the lock was successfully acquired and must be explicitly
    /// released by the caller (Drop only logs a warning).
    armed: bool,
}

impl RuntimeLockGuard {
    /// Acquire a runtime lock for a mutating HTTP handler.
    ///
    /// # Errors
    ///
    /// Returns `NexusApiError::Locked` if the Work is already locked by another
    /// process (and the lock is not stale).
    pub async fn acquire(
        pool: &SqlitePool,
        creator_id: &str,
        work_id: &str,
    ) -> Result<Self, NexusApiError> {
        let holder = nexus_local_db::cli_holder("http");
        let ttl = nexus_local_db::ttl_from_env();
        match nexus_local_db::acquire_runtime_lock(pool, creator_id, work_id, &holder, ttl, true)
            .await
        {
            Ok(nexus_local_db::AcquireResult::Acquired { .. }) => Ok(Self {
                pool: pool.clone(),
                creator_id: creator_id.to_string(),
                work_id: work_id.to_string(),
                holder,
                armed: true,
            }),
            Ok(nexus_local_db::AcquireResult::Locked {
                holder: existing, ..
            }) => Err(NexusApiError::Locked {
                resource: "work".to_string(),
                reason: format!(
                    "work {work_id} is locked by '{existing}'; \
                     wait for release or check 'creator works status'"
                ),
            }),
            Err(e) => Err(NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("runtime_lock acquire failed: {e}"),
            }),
        }
    }

    /// Release the lock early (before drop). Useful when the handler
    /// needs to explicitly control the release point.
    pub async fn release(mut self) {
        self.disarm().await;
    }

    async fn disarm(&mut self) {
        if self.armed {
            self.armed = false;
            if let Err(e) = nexus_local_db::release_runtime_lock(
                &self.pool,
                &self.creator_id,
                &self.work_id,
                &self.holder,
            )
            .await
            {
                tracing::warn!(
                    work_id = %self.work_id,
                    holder = %self.holder,
                    error = %e,
                    "runtime_lock: failed to release on drop"
                );
            }
        }
    }
}

impl Drop for RuntimeLockGuard {
    fn drop(&mut self) {
        if self.armed {
            // Best-effort synchronous release is not possible with async.
            // Log a warning — the guard should be explicitly released via
            // `release()` before dropping. If not, the TTL will clean up
            // the stale lock.
            tracing::warn!(
                work_id = %self.work_id,
                holder = %self.holder,
                "runtime_lock: guard dropped without explicit release; \
                 TTL-based recovery will clean up"
            );
        }
    }
}

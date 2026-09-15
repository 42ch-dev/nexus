//! Per-Work auto-chronology state read/projection over the guarded core.
//!
//! Extracts the read side of the CLI `creator works chronology show`
//! projection (ref-or-id Work resolution + `works.auto_chronology` flag) so
//! P6-T1 can migrate the CLI leaf onto the core without a second Work-lookup
//! authority. `set` and `advance` stay on their existing surfaces
//! (`nexus-local-db` write / `nexus_orchestration::auto_chronology`).

use crate::{CoreError, CoreResult, CoreService, Principal};

/// Owned auto-chronology state projection for a Work.
///
/// Mirrors the `chronology show` payload minus the CLI-local last-advance
/// filesystem label (that scan is bound to CLI workspace-root discovery).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreWorkChronology {
    pub work_id: String,
    pub auto_chronology: bool,
}

impl CoreService {
    /// Resolve a Work by `work_ref` slug or `work_id` and project its
    /// auto-chronology state (behavior matches the CLI `chronology show`
    /// read path: creator + workspace scoped, ref precedence per
    /// `resolve_work_id_by_ref_or_id`).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::NotFound`] when no Work matches in the
    /// active workspace, and [`CoreError::Internal`] (legacy
    /// `DATABASE_ERROR` carrier at the adapter) for storage faults.
    pub async fn work_chronology(
        &self,
        principal: &Principal,
        work_ref_or_id: &str,
    ) -> CoreResult<CoreWorkChronology> {
        self.verify_principal(principal)?;
        let work_id = nexus_local_db::works::resolve_work_id_by_ref_or_id(
            &self.inner.pool,
            principal.creator_id(),
            principal.workspace_slug(),
            work_ref_or_id,
        )
        .await
        .map_err(crate::error::local_db_err)?
        .ok_or_else(|| CoreError::NotFound {
            resource: format!("work {work_ref_or_id}"),
        })?;
        let auto_chronology =
            nexus_local_db::works::get_auto_chronology(&self.inner.pool, &work_id)
                .await
                .map_err(crate::error::local_db_err)?;
        Ok(CoreWorkChronology { work_id, auto_chronology })
    }
}

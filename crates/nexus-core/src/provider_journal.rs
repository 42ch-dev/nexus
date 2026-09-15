//! Durable JS-provider operation journal, owned by `CoreService` (LIFE-3).
//!
//! The journal mirrors native provider-callback facts so a previously active
//! operation stays queryable after a restart (`interrupted`, never 404 and
//! never a fabricated terminal). It is not a second business truth: no
//! `core_changes` outbox event is emitted and no revision is maintained.
//! These owned methods are the narrow seam that lets P4-T2 retire the
//! transitional [`CoreService::pool()`] escape hatch — no pool or transaction
//! handle leaves the service.

use nexus_contracts::{
    CoreProviderJournalWrite, CoreProviderJournalWriteStatus, CoreProviderOperation,
    CoreProviderOperationStatus,
};
use nexus_local_db::js_provider_journal::{self, JournaledOperation};

use crate::error::{local_db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;
use crate::CoreAccess;

fn journal_internal(what: &str, e: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("journal row {what}: {e}"),
    }
}

/// Wire spelling of a journal write status (schema enum order).
const fn status_wire(status: &CoreProviderJournalWriteStatus) -> &'static str {
    match status {
        CoreProviderJournalWriteStatus::Running => "running",
        CoreProviderJournalWriteStatus::Finished => "finished",
        CoreProviderJournalWriteStatus::Failed => "failed",
        CoreProviderJournalWriteStatus::Interrupted => "interrupted",
        CoreProviderJournalWriteStatus::Cancelled => "cancelled",
    }
}

/// Project a stored journal row onto the owned wire DTO.
///
/// Rows are only ever written through [`CoreService::journal_provider_operation`],
/// so a row that fails projection means the stored journal was tampered with;
/// the error is `internal`, never a fabricated success.
fn project_operation(row: JournaledOperation) -> CoreResult<CoreProviderOperation> {
    let sequence = u64::try_from(row.sequence)
        .map_err(|_| journal_internal("sequence", row.sequence))?;
    Ok(CoreProviderOperation {
        operation_id: row
            .operation_id
            .parse()
            .map_err(|e| journal_internal("operation_id", e))?,
        session_id: row
            .session_id
            .parse()
            .map_err(|e| journal_internal("session_id", e))?,
        provider_id: row
            .provider_id
            .parse()
            .map_err(|e| journal_internal("provider_id", e))?,
        status: CoreProviderOperationStatus::try_from(row.status.as_str())
            .map_err(|e| journal_internal("status", e))?,
        sequence,
    })
}

impl CoreService {
    fn require_write_access(&self, what: &str) -> CoreResult<()> {
        if self.inner.access == CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden {
                resource: format!("{what}: read-only core access"),
            });
        }
        Ok(())
    }

    /// Read one journaled JS-provider operation by id.
    ///
    /// Returns `Ok(None)` for an unknown id — a restart never fabricates a
    /// terminal status for an operation it has no durable record of.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, and the mapped storage error otherwise.
    pub async fn provider_operation(
        &self,
        principal: &Principal,
        operation_id: String,
    ) -> CoreResult<Option<CoreProviderOperation>> {
        self.verify_principal(principal)?;
        js_provider_journal::get_operation(&self.inner.pool, &operation_id)
            .await
            .map_err(local_db_err)?
            .map(project_operation)
            .transpose()
    }

    /// Write-through one provider operation journal record.
    ///
    /// The sequence is assigned by the store; an already-terminal row is
    /// never downgraded back to `running` by a later write.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::Forbidden`] under read-only
    /// access, and the mapped storage error otherwise.
    pub async fn journal_provider_operation(
        &self,
        principal: &Principal,
        request: CoreProviderJournalWrite,
    ) -> CoreResult<()> {
        self.verify_principal(principal)?;
        self.require_write_access("provider_journal_write")?;
        js_provider_journal::upsert_operation(
            &self.inner.pool,
            &request.operation_id,
            &request.session_id,
            &request.provider_id,
            status_wire(&request.status),
        )
        .await
        .map_err(local_db_err)
    }

    /// Settle orphaned journal entries: every non-terminal operation a
    /// predecessor process left behind becomes `interrupted`, and the journal
    /// is pruned to its bounded retention window. Returns the number of
    /// settled rows.
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the service is closing,
    /// [`CoreError::AuthRequired`] when the on-disk selection no longer
    /// matches the context this service was opened against,
    /// [`CoreError::Forbidden`] under read-only access, and the mapped
    /// storage error otherwise.
    pub async fn settle_provider_orphans(&self) -> CoreResult<u64> {
        self.ensure_open()?;
        self.verify_selected_context()?;
        self.require_write_access("provider_journal_settle")?;
        js_provider_journal::settle_orphaned_as_interrupted(&self.inner.pool)
            .await
            .map_err(local_db_err)
    }
}

impl CoreService {
    /// Internal effect-boundary seam (P4-T2): the environment's admitting
    /// provider port journals at the provider effect boundary, where no
    /// per-request principal exists. The write is guarded by the open/closing
    /// state and write access only — never by a principal re-verification,
    /// so the LIFE-3 durable mirror cannot be silenced by a later on-disk
    /// selection change mid-session.
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the service is closing and the
    /// mapped storage error otherwise.
    /// NOTE: `pub` visibility is the P4-T2 environment-boundary seam; do not
    /// acquire new callers.
    pub async fn journal_provider_write_internal(
        &self,
        operation_id: &str,
        session_id: &str,
        provider_id: &str,
        status: &str,
    ) -> CoreResult<()> {
        self.ensure_open()?;
        self.require_write_access("provider_journal_write")?;
        js_provider_journal::upsert_operation(
            &self.inner.pool,
            operation_id,
            session_id,
            provider_id,
            status,
        )
        .await
        .map_err(local_db_err)
    }

    /// Internal read seam mirroring [`Self::journal_provider_write_internal`]:
    /// raw stored row fields for the `hostQuery` restart fallback, without a
    /// per-request principal. Returns the stored `(operation_id, session_id,
    /// status)` verbatim; no terminal is ever fabricated.
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the service is closing and the
    /// mapped storage error otherwise.
    /// NOTE: `pub` visibility is the P4-T2 environment-boundary seam; do not
    /// acquire new callers.
    pub async fn provider_operation_row_internal(
        &self,
        operation_id: &str,
    ) -> CoreResult<Option<(String, String, String)>> {
        self.ensure_open()?;
        Ok(
            js_provider_journal::get_operation(&self.inner.pool, operation_id)
                .await
                .map_err(local_db_err)?
                .map(|row| (row.operation_id, row.session_id, row.status)),
        )
    }

    /// Internal delete seam for a cleanly shut-down session's journal rows.
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the service is closing and the
    /// mapped storage error otherwise.
    /// NOTE: `pub` visibility is the P4-T2 environment-boundary seam; do not
    /// acquire new callers.
    pub async fn forget_provider_session_internal(
        &self,
        session_id: &str,
    ) -> CoreResult<()> {
        self.ensure_open()?;
        self.require_write_access("provider_journal_forget")?;
        js_provider_journal::forget_session(&self.inner.pool, session_id)
            .await
            .map_err(local_db_err)
    }
}

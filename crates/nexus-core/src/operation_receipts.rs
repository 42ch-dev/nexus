//! Core-owned read handle over the durable operation receipts store
//! (v1.207 P3 / RN-OGA-5).
//!
//! Spec: `.mstar/iterations/v1.207/specs/connect-replay-and-operation-receipts.md`
//! §B.2 (the `operation_receipts` read path) and §B.3 step 2 ("ask the receipt
//! store first"). The storage half lives in `nexus_local_db::operation_receipts`
//! (the store + the frozen Recover handshake); this module is the narrow
//! `CoreService` seam that keeps the pool from escaping the service and hands
//! callers the owned wire DTO `CoreOperationReceipt`.
//!
//! Only the read side is owned here: recovery entry points ask the receipt
//! store BEFORE deciding to re-apply, and a terminal receipt answers the
//! replay instead of a second effect run.

use nexus_contracts::{
    CoreOperationReceipt, CoreOperationReceiptConsumer, CoreOperationReceiptOperationId,
    CoreOperationReceiptRequestFingerprint, CoreOperationReceiptStatus,
    CoreOperationReceiptSubjectId,
};
use nexus_local_db::OperationReceipt;

use crate::error::{local_db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;

fn receipt_internal(what: &str, e: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("operation receipt {what}: {e}"),
    }
}

/// The frozen `uncertain` refusal code (spec §C).
///
/// Gated on the cohort that renders it today: the compute Run receipt
/// recovery entry ([`crate::execution::compute::recover_stuck_compute_runs`]).
/// The Connect write surface is the other intended consumer; the gate moves
/// with that wiring.
#[cfg(feature = "compute")]
pub const UNCERTAIN_CODE: &str = "uncertain";

/// The typed §B.3 item-4 refusal: a non-idempotent write with no terminal
/// receipt is **never** retried blindly.
///
/// `subject` names the operation as the consumer knows it (a compute `run_id`,
/// a Connect `<peer_session_id>/<op>`); `reason` says which ambiguity was
/// observed. The code is the frozen [`UNCERTAIN_CODE`], rendered 409 by the
/// adapters.
#[cfg(feature = "compute")]
#[must_use]
pub fn uncertain_refusal(subject: &str, reason: &str) -> CoreError {
    CoreError::Coded {
        code: UNCERTAIN_CODE.to_string(),
        message: format!(
            "operation {subject} cannot be retried: {reason}; with no terminal receipt the \
             effect may already have been applied, so re-applying it could duplicate the result"
        ),
    }
}

/// Parse one stored RFC 3339 timestamp column into the wire `DateTime<Utc>`.
fn parse_timestamp(what: &str, value: &str) -> CoreResult<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
        .map_err(|e| receipt_internal(what, e))
}

/// Project a stored receipt row onto the owned wire DTO.
///
/// Rows are only ever written through the receipt store (which enforces the
/// status/payload CHECK), so a row that fails projection means the stored
/// receipt was tampered with; the error is `internal`, never a fabricated
/// success and never a fabricated terminal.
///
/// The compute Run recovery pass answers a terminal replay **from the
/// receipt** (§B.3 step 3), so it needs the same owned projection this
/// module's read handle returns. (`pub` in this private module = crate-wide;
/// the module itself is not part of the public surface.)
pub fn project_receipt(row: OperationReceipt) -> CoreResult<CoreOperationReceipt> {
    let sequence =
        u64::try_from(row.sequence).map_err(|_| receipt_internal("sequence", row.sequence))?;
    Ok(CoreOperationReceipt {
        operation_id: row
            .operation_id
            .parse::<CoreOperationReceiptOperationId>()
            .map_err(|e| receipt_internal("operation_id", e))?,
        consumer: CoreOperationReceiptConsumer::try_from(row.consumer.as_str())
            .map_err(|e| receipt_internal("consumer", e))?,
        subject_id: row
            .subject_id
            .parse::<CoreOperationReceiptSubjectId>()
            .map_err(|e| receipt_internal("subject_id", e))?,
        status: CoreOperationReceiptStatus::try_from(row.status.as_str())
            .map_err(|e| receipt_internal("status", e))?,
        request_fingerprint: row
            .request_fingerprint
            .parse::<CoreOperationReceiptRequestFingerprint>()
            .map_err(|e| receipt_internal("request_fingerprint", e))?,
        result_json: row.result_json,
        error_json: row.error_json,
        created_at: parse_timestamp("created_at", &row.created_at)?,
        updated_at: parse_timestamp("updated_at", &row.updated_at)?,
        terminal_at: row
            .terminal_at
            .as_deref()
            .map(|value| parse_timestamp("terminal_at", value))
            .transpose()?,
        sequence,
    })
}

impl CoreService {
    /// Read one durable operation receipt by id (spec §B.2 read path, §B.3
    /// step 2).
    ///
    /// Returns `Ok(None)` for an unknown id — recovery never fabricates a
    /// receipt for an operation the store has no durable record of, and an
    /// absent receipt is exactly the "safe to apply once" signal.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, and the mapped storage error otherwise.
    pub async fn get_operation_receipt(
        &self,
        principal: &Principal,
        operation_id: String,
    ) -> CoreResult<Option<CoreOperationReceipt>> {
        self.verify_principal(principal)?;
        nexus_local_db::operation_receipts::get_operation_receipt(&self.inner.pool, &operation_id)
            .await
            .map_err(local_db_err)?
            .map(project_receipt)
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_local_db::operation_receipts::{
        OperationReceipt, STATUS_CANCELLED, STATUS_FINISHED, STATUS_RUNNING,
    };

    fn row(status: &str, result_json: Option<&str>, terminal_at: Option<&str>) -> OperationReceipt {
        OperationReceipt {
            operation_id: format!("op_{}", "a".repeat(32)),
            consumer: "compute_run".to_string(),
            subject_id: "run-1".to_string(),
            status: status.to_string(),
            request_fingerprint: "fp-1".to_string(),
            result_json: result_json.map(ToString::to_string),
            error_json: None,
            created_at: "2026-10-07T10:00:00+00:00".to_string(),
            updated_at: "2026-10-07T10:00:05+00:00".to_string(),
            terminal_at: terminal_at.map(ToString::to_string),
            sequence: 3,
        }
    }

    /// The store's RFC 3339 text columns project onto the wire DTO, including
    /// the nullable terminal timestamp and the payload-free running shape.
    #[test]
    fn stored_rows_project_onto_the_wire_dto() {
        let running = project_receipt(row(STATUS_RUNNING, None, None)).unwrap();
        assert_eq!(running.sequence, 3);
        assert_eq!(running.status, CoreOperationReceiptStatus::Running);
        assert_eq!(running.consumer, CoreOperationReceiptConsumer::ComputeRun);
        assert_eq!(running.result_json, None);
        assert_eq!(running.terminal_at, None);
        assert_eq!(running.created_at.to_rfc3339(), "2026-10-07T10:00:00+00:00");

        let finished = project_receipt(row(
            STATUS_FINISHED,
            Some("{\"ok\":true}"),
            Some("2026-10-07T10:00:05+00:00"),
        ))
        .unwrap();
        assert_eq!(finished.status, CoreOperationReceiptStatus::Finished);
        assert_eq!(finished.result_json.as_deref(), Some("{\"ok\":true}"));
        assert!(finished.terminal_at.is_some());

        let cancelled = project_receipt(row(
            STATUS_CANCELLED,
            None,
            Some("2026-10-07T10:00:05+00:00"),
        ))
        .unwrap();
        assert_eq!(cancelled.status, CoreOperationReceiptStatus::Cancelled);
    }

    /// A tampered row is an internal refusal, never a fabricated receipt.
    #[test]
    fn malformed_stored_rows_are_internal_refusals() {
        let mut bad_status = row(
            STATUS_FINISHED,
            Some("{}"),
            Some("2026-10-07T10:00:05+00:00"),
        );
        bad_status.status = "not_a_status".to_string();
        assert!(matches!(
            project_receipt(bad_status),
            Err(CoreError::Internal { .. })
        ));

        let mut bad_timestamp = row(STATUS_FINISHED, Some("{}"), None);
        bad_timestamp.created_at = "2026-10-07 10:00:00".to_string();
        assert!(matches!(
            project_receipt(bad_timestamp),
            Err(CoreError::Internal { .. })
        ));

        let mut bad_id = row(STATUS_FINISHED, Some("{}"), None);
        bad_id.operation_id = "not-an-op-id".to_string();
        assert!(matches!(
            project_receipt(bad_id),
            Err(CoreError::Internal { .. })
        ));
    }

    /// The store's first-writer-wins refusal maps onto the frozen
    /// `operation_id_conflict` code (spec §B.1/§C) at the core taxonomy seam.
    #[test]
    fn operation_id_conflict_maps_to_the_frozen_code() {
        let mapped = local_db_err(nexus_local_db::LocalDbError::OperationIdConflict {
            operation_id: format!("op_{}", "b".repeat(32)),
        });
        match mapped {
            CoreError::Coded { code, message } => {
                assert_eq!(code, nexus_local_db::OPERATION_ID_CONFLICT_CODE);
                assert!(
                    message.contains("different request fingerprint"),
                    "got: {message}"
                );
            }
            other => panic!("expected a coded refusal, got {other:?}"),
        }
    }
}

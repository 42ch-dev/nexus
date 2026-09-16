//! Local outbox status/resolve owned by `CoreService` (v1.190 P2-T0).
//!
//! The local send-queue surface extracted from the CLI sync helpers: status
//! and resolve of stuck entries, served through the existing
//! `nexus-cloud-sync` Outbox on its private guarded pool
//! (`OutboxPool`, already writer-protocol admitted). No second outbox SQL
//! implementation and no cloud HTTP behavior lives here — platform
//! authentication, push and pull stay the existing explicit CLI cloud
//! clients.
//!
//! Resolution is local and never re-sends by itself: `retry` re-queues a
//! stuck (`conflicted`/`failed`) entry for delivery by the next replay/push
//! cycle; `discard` drops it from the delivery queue (permanently failed
//! with no retry time, durable row retained). Manual review remains a
//! caller-side no-op: a CLI that resolves nothing simply does not call
//! [`CoreService::resolve_outbox`].

use nexus_cloud_sync::outbox::Outbox;
use nexus_cloud_sync::pool::{OutboxPool, DEFAULT_POOL_SIZE};
use nexus_cloud_sync::SyncError;
use nexus_contracts::{
    CoreOutboxResolveRequest, CoreOutboxStatus, CoreOutboxStatusEntriesItem,
    CoreOutboxStatusEntriesItemBundleId, CoreOutboxStatusEntriesItemCreatedAt,
    CoreOutboxStatusEntriesItemDeliveryState, CoreOutboxStatusEntriesItemIdempotencyKey,
    CoreOutboxStatusEntriesItemOutboxEntryId,
};

use crate::error::{local_db_err, CoreError, CoreResult};
use crate::home::newtype_value;
use crate::principal::Principal;
use crate::service::CoreService;

/// Status page bound (the generated surface is a bounded page, not an
/// unbounded dump).
const OUTBOX_STATUS_PAGE: i64 = 100;

impl CoreService {
    /// Bounded page (newest first) of the durable send-queue entries of this
    /// workspace's state DB, each with delivery state, retry accounting and
    /// last error.
    ///
    /// Errors propagate — a failing read is an error, never a zero count.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, and the mapped outbox error otherwise.
    pub async fn outbox_status(&self, principal: &Principal) -> CoreResult<CoreOutboxStatus> {
        self.verify_principal(principal)?;
        let outbox = self.open_outbox().await?;
        let entries = outbox
            .list_page(OUTBOX_STATUS_PAGE)
            .await
            .map_err(sync_err)?;
        let entries = entries
            .into_iter()
            .map(map_entry)
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(CoreOutboxStatus { entries })
    }

    /// Resolve one stuck local outbox entry: `retry` re-queues it for
    /// delivery, `discard` drops it locally. Only stuck (`conflicted`/
    /// `failed`) entries are resolvable; resolution is local and never
    /// re-sends by itself.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::Forbidden`] under read-only
    /// access, [`CoreError::NotFound`] when the entry does not exist,
    /// [`CoreError::InvalidInput`] when the entry is not stuck, and the
    /// mapped outbox error otherwise.
    pub async fn resolve_outbox(
        &self,
        principal: &Principal,
        request: CoreOutboxResolveRequest,
    ) -> CoreResult<()> {
        self.verify_principal(principal)?;
        if self.inner.access == crate::CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden {
                resource: "outbox_resolve: read-only core access".to_string(),
            });
        }
        let outbox = self.open_outbox().await?;
        let entry = outbox
            .get(request.outbox_entry_id.as_str())
            .await
            .map_err(sync_err)?;
        match entry.delivery_state {
            nexus_contracts::DeliveryState::Conflicted | nexus_contracts::DeliveryState::Failed => {
            }
            other => {
                return Err(CoreError::InvalidInput {
                    field: "outbox_entry_id".to_string(),
                    reason: format!(
                        "only stuck (conflicted/failed) entries are resolvable, entry is {other:?}"
                    ),
                });
            }
        }
        match request.action {
            nexus_contracts::CoreOutboxResolveRequestAction::Retry => {
                outbox
                    .requeue_stuck(request.outbox_entry_id.as_str())
                    .await
                    .map_err(sync_err)?;
            }
            nexus_contracts::CoreOutboxResolveRequestAction::Discard => {
                outbox
                    .discard_stuck(
                        request.outbox_entry_id.as_str(),
                        "core resolve: discarded from the local send queue",
                    )
                    .await
                    .map_err(sync_err)?;
            }
        }
        Ok(())
    }

    /// Open the existing cloud-sync Outbox on its private guarded pool for
    /// this workspace's state DB.
    async fn open_outbox(&self) -> CoreResult<Outbox> {
        let pool = OutboxPool::new(&self.inner.db_path, DEFAULT_POOL_SIZE)
            .await
            .map_err(local_db_err)?;
        Outbox::with_pool(pool).await.map_err(sync_err)
    }
}

/// Map a sync/outbox error onto the core taxonomy. Missing entries are
/// not-found; stuck-state and input violations are caller errors; everything
/// else is an honest internal failure with its category string.
fn sync_err(e: SyncError) -> CoreError {
    match e {
        SyncError::OutboxEntryNotFound { id } => CoreError::NotFound {
            resource: format!("outbox entry {id}"),
        },
        SyncError::InvalidInput(reason) => CoreError::InvalidInput {
            field: "outbox".to_string(),
            reason,
        },
        SyncError::OutboxInvalidState { expected, actual } => CoreError::InvalidInput {
            field: "outbox".to_string(),
            reason: format!("invalid state transition: expected {expected}, got {actual}"),
        },
        SyncError::OutboxMaxRetriesExceeded { id, retries } => CoreError::Internal {
            category: format!("outbox max retries exceeded: {id} (retried {retries} times)"),
        },
        other => CoreError::Internal {
            category: format!("outbox: {other}"),
        },
    }
}

/// Project one durable outbox row onto the generated status entry.
fn map_entry(
    entry: nexus_contracts::local::domain::OutboxEntry,
) -> CoreResult<CoreOutboxStatusEntriesItem> {
    let delivery_state = match entry.delivery_state {
        nexus_contracts::DeliveryState::Staged => CoreOutboxStatusEntriesItemDeliveryState::Staged,
        nexus_contracts::DeliveryState::Ready => CoreOutboxStatusEntriesItemDeliveryState::Ready,
        nexus_contracts::DeliveryState::Sent => CoreOutboxStatusEntriesItemDeliveryState::Sent,
        nexus_contracts::DeliveryState::Acked => CoreOutboxStatusEntriesItemDeliveryState::Acked,
        nexus_contracts::DeliveryState::Conflicted => {
            CoreOutboxStatusEntriesItemDeliveryState::Conflicted
        }
        nexus_contracts::DeliveryState::Failed => CoreOutboxStatusEntriesItemDeliveryState::Failed,
    };
    Ok(CoreOutboxStatusEntriesItem {
        schema_version: i64::from(entry.schema_version),
        outbox_entry_id: newtype_value::<CoreOutboxStatusEntriesItemOutboxEntryId>(
            &entry.outbox_entry_id,
        )?,
        bundle_id: newtype_value::<CoreOutboxStatusEntriesItemBundleId>(&entry.bundle_id)?,
        idempotency_key: newtype_value::<CoreOutboxStatusEntriesItemIdempotencyKey>(
            &entry.idempotency_key,
        )?,
        delivery_state,
        retry_count: entry.retry_count,
        last_error: entry.last_error,
        next_retry_at: entry.next_retry_at,
        created_at: newtype_value::<CoreOutboxStatusEntriesItemCreatedAt>(&entry.created_at)?,
        updated_at: entry.updated_at,
    })
}

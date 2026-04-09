//! Apply a platform [`SyncPullResponse`] to the local [`Outbox`].

use nexus_contracts::generated::{Bundle, SyncPullResponse};

use crate::errors::{SyncError, SyncResult};
use crate::outbox::Outbox;

/// Summary of applying a pull response to the outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullApplySummary {
    pub world_revision: u64,
    pub confirmed_delta_sequence: u64,
    pub staged_entry_ids: Vec<String>,
    pub skipped_duplicate_bundles: usize,
}

/// Deserialize each bundle in `response` and stage it when `bundle_id` is not already present.
pub async fn apply_pull_response_to_outbox(
    outbox: &Outbox,
    response: &SyncPullResponse,
) -> SyncResult<PullApplySummary> {
    let mut staged_entry_ids = Vec::new();
    let mut skipped_duplicate_bundles = 0usize;

    for raw in &response.bundles {
        let bundle: Bundle = serde_json::from_value(raw.clone()).map_err(|e| {
            SyncError::BundleValidation(format!("pull bundle JSON does not match Bundle: {e}"))
        })?;
        match outbox.stage_if_absent(&bundle).await? {
            Some(id) => staged_entry_ids.push(id),
            None => skipped_duplicate_bundles += 1,
        }
    }

    Ok(PullApplySummary {
        world_revision: response.world_revision,
        confirmed_delta_sequence: response.confirmed_delta_sequence,
        staged_entry_ids,
        skipped_duplicate_bundles,
    })
}

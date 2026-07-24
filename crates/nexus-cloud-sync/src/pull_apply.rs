//! Apply a platform [`SyncPullResponse`] to the local [`Outbox`].

use nexus_contracts::generated::SyncPullResponse;

use crate::errors::SyncResult;
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
///
/// # Errors
/// Returns the specific error type if the operation fails.
///
/// # Panics
/// Panics if a pull-response bundle envelope fails to round-trip through `Bundle`
/// (drift-gate-proven equivalent; should never happen for well-formed platform responses).
pub async fn apply_pull_response_to_outbox(
    outbox: &Outbox,
    response: &SyncPullResponse,
) -> SyncResult<PullApplySummary> {
    let mut staged_entry_ids = Vec::new();
    let mut skipped_duplicate_bundles = 0usize;

    for envelope in &response.bundles {
        // `SyncPullResponse.bundles` uses typify's inlined `NexusDeltaBundleEnvelope`;
        // the outbox stores the canonical `Bundle`. Wire-equivalent (drift gate).
        let bundle: nexus_contracts::Bundle =
            serde_json::from_value(serde_json::to_value(envelope).unwrap_or_default())
                .expect("pull envelope round-trips to Bundle");
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

//! Refreshable scan delta-write — sync a world's `kb_key_blocks` against a
//! freshly-extracted candidate set.
//!
//! V1.50 T-B P2 (plan `2026-06-18-v1.50-kb-refreshable-scan` §5 T3; compass
//! §0.1 decision 7). The `creator kb rescan` CLI re-runs the review-time
//! heuristic over the current chapter text and calls into this module to keep
//! confirmed `KeyBlock` rows in sync with their source chapter.
//!
//! # §5.5 invariant (entity-scope-model)
//!
//! A `KeyBlock` enters the World only via the promotion state machine
//! (`pending → confirmed`, terminal). This module therefore **never inserts
//! or deletes** `kb_key_blocks` — those operations stay with
//! `creator world kb adopt|edit|delete`. [`diff_and_apply`] only refreshes the
//! `body` of active `KeyBlock`s whose `canonical_name` matches a freshly
//! extracted candidate, so "KB rows reflect current chapter text" (compass
//! flow line 173) without bypassing the author adopt gate.
//!
//! [`compute_kb_diff`] is the pure half used by `--dry-run`; [`diff_and_apply`]
//! is compute + apply.

use crate::key_block::{KeyBlock, KeyBlockBody};
use crate::store::{KbStore, KbStoreError};
use serde::Serialize;

/// Whether a `KeyBlock` status counts as "active" for body refresh.
///
/// Mirrors the `kb_key_blocks` partial unique index
/// `WHERE status NOT IN ('deleted', 'merged', 'deprecated')`.
fn is_active_status(status: &str) -> bool {
    !matches!(status, "deleted" | "merged" | "deprecated")
}

/// A `KeyBlock` body refresh applied by [`diff_and_apply`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KbSyncUpdate {
    /// The `KeyBlock` whose body was refreshed.
    pub key_block_id: String,
    /// The matched `canonical_name`.
    pub canonical_name: String,
}

/// Computed diff for a refreshable scan against a world's `kb_key_blocks`.
///
/// `inserted` and `removed` are **advisory**: the rescan does not create or
/// remove `KeyBlock`s (§5.5 reserves those for adopt/edit/delete). They are
/// reported so the author knows what changed; `updated` is the only category
/// actually persisted by [`diff_and_apply`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct KbSyncDiff {
    /// Canonical names present in the new extraction but with no matching
    /// active `KeyBlock`. Promote via `creator world kb adopt`.
    pub inserted: Vec<String>,
    /// Active `KeyBlock`s whose body was refreshed from the new extraction.
    pub updated: Vec<KbSyncUpdate>,
    /// Active `KeyBlock`s whose `canonical_name` vanished from the new
    /// extraction. Review via `creator world kb edit|delete`.
    pub removed: Vec<String>,
}

impl KbSyncDiff {
    /// Returns `true` when the diff contains no changes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.inserted.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }
}

/// Compute (no I/O) the diff between a world's existing `KeyBlock`s and a
/// freshly-extracted candidate set.
///
/// Matching is by `canonical_name`, case-insensitive. Used by `--dry-run` to
/// preview changes without writing, and by [`diff_and_apply`] as its compute
/// step.
///
/// # Arguments
///
/// * `old_rows` — the world's current `KeyBlock`s (e.g. from `list_by_world`).
/// * `new_extracted` — `(canonical_name, body)` tuples from the heuristic.
#[must_use]
pub fn compute_kb_diff(
    old_rows: &[KeyBlock],
    new_extracted: &[(String, KeyBlockBody)],
) -> KbSyncDiff {
    // Index active old rows by lowercased canonical_name for O(1) lookup.
    use std::collections::HashMap;
    let mut active_by_name: HashMap<String, &KeyBlock> = HashMap::new();
    for kb in old_rows.iter().filter(|kb| is_active_status(&kb.status)) {
        active_by_name
            .entry(kb.canonical_name.to_ascii_lowercase())
            .or_insert(kb);
    }

    let mut diff = KbSyncDiff::default();
    let mut matched_old: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (name, new_body) in new_extracted {
        let name_lower = name.to_ascii_lowercase();
        let Some(old_kb) = active_by_name.get(&name_lower) else {
            // No active KeyBlock for this candidate — advisory insert.
            diff.inserted.push(name.clone());
            continue;
        };
        matched_old.insert(old_kb.key_block_id.clone());
        // Body refresh only when the extracted body differs from the row's.
        if old_kb.body.as_ref() != Some(new_body) {
            diff.updated.push(KbSyncUpdate {
                key_block_id: old_kb.key_block_id.clone(),
                canonical_name: old_kb.canonical_name.clone(),
            });
        }
    }

    // Active old rows not matched by any new candidate — advisory remove.
    for kb in old_rows.iter().filter(|kb| is_active_status(&kb.status)) {
        if !matched_old.contains(&kb.key_block_id) {
            diff.removed.push(kb.canonical_name.clone());
        }
    }

    diff
}

/// Compute + apply a refreshable-scan diff: refresh the `body` of active
/// `KeyBlock`s whose `canonical_name` matches a freshly-extracted candidate.
///
/// Insert/delete of `KeyBlock`s is **not** performed (entity-scope-model §5.5
/// reserves those for adopt/edit/delete); `inserted` and `removed` in the
/// returned [`KbSyncDiff`] are advisory. Each body refresh is an atomic
/// per-row `update_key_block` (which re-runs the store's configured
/// `ValidationMode`). A row whose body is unchanged is skipped.
///
/// # Errors
///
/// Returns [`KbStoreError`] if any `update_key_block` fails (the diff is
/// applied best-effort up to that row; re-running the rescan retries the
/// remaining rows idempotently).
///
/// # Arguments
///
/// * `store` — the KB store (caller selects `ValidationMode`).
/// * `world_id` — the world being rescanned (forwarded to `update_key_block`
///   via each cloned `KeyBlock`).
/// * `old_rows` — the world's current `KeyBlock`s.
/// * `new_extracted` — `(canonical_name, body)` tuples from the heuristic.
pub async fn diff_and_apply<S>(
    store: &S,
    world_id: &str,
    old_rows: &[KeyBlock],
    new_extracted: &[(String, KeyBlockBody)],
) -> Result<KbSyncDiff, KbStoreError>
where
    S: KbStore + Sync,
{
    let mut diff = compute_kb_diff(old_rows, new_extracted);

    // Apply only body refreshes. Re-resolve each updated row from old_rows to
    // build the full KeyBlock (compute_kb_diff only carries ids/names).
    let mut applied: Vec<KbSyncUpdate> = Vec::with_capacity(diff.updated.len());
    for update in diff.updated.drain(..) {
        let Some(old_kb) = old_rows
            .iter()
            .find(|kb| kb.key_block_id == update.key_block_id)
        else {
            // Row vanished between compute and apply — skip defensively.
            continue;
        };
        let new_body = new_extracted
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&old_kb.canonical_name))
            .map(|(_, b)| b.clone());

        let Some(new_body) = new_body else {
            continue;
        };

        let mut refreshed = old_kb.clone();
        refreshed.world_id = world_id.to_string();
        refreshed.body = Some(new_body);
        refreshed.updated_at = Some(chrono::Utc::now().to_rfc3339());

        store.update_key_block(refreshed).await?;
        applied.push(update);
    }
    diff.updated = applied;
    Ok(diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryKbStore;
    use nexus_contracts::BlockType;

    fn body_with(summary: &str) -> KeyBlockBody {
        KeyBlockBody {
            summary: Some(summary.to_string()),
            attributes: Some(serde_json::json!({"novel_category": "character"})),
            tags: Some(vec!["novel".to_string()]),
        }
    }

    fn confirmed_block(world_id: &str, name: &str, body: KeyBlockBody) -> KeyBlock {
        let mut kb = KeyBlock::new(world_id, BlockType::Character, name);
        kb.status = "confirmed".to_string();
        kb.body = Some(body);
        kb
    }

    #[test]
    fn compute_diff_flags_inserted_and_removed_advisory() {
        let world = "wld_1";
        let old = vec![confirmed_block(world, "Kept", body_with("old"))];
        let new = vec![
            ("Kept".to_string(), body_with("old")),
            ("Newcomer".to_string(), body_with("fresh")),
        ];
        let diff = compute_kb_diff(&old, &new);
        // "Kept" unchanged → not in updated.
        assert!(diff.updated.is_empty());
        // "Newcomer" has no KeyBlock → advisory insert.
        assert_eq!(diff.inserted, vec!["Newcomer".to_string()]);
        // No active old row vanished → no removes.
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn compute_diff_flags_body_update() {
        let world = "wld_1";
        let old = vec![confirmed_block(world, "Lin Xia", body_with("v1"))];
        let new = vec![("Lin Xia".to_string(), body_with("v2-edited"))];
        let diff = compute_kb_diff(&old, &new);
        assert_eq!(diff.updated.len(), 1);
        assert_eq!(diff.updated[0].canonical_name, "Lin Xia");
        assert!(diff.inserted.is_empty());
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn compute_diff_matches_case_insensitively() {
        let world = "wld_1";
        let old = vec![confirmed_block(world, "Lin Xia", body_with("v1"))];
        let new = vec![("lin xia".to_string(), body_with("v2"))];
        let diff = compute_kb_diff(&old, &new);
        assert_eq!(
            diff.updated.len(),
            1,
            "case-insensitive match should update"
        );
    }

    #[test]
    fn compute_diff_ignores_deleted_rows() {
        let world = "wld_1";
        let mut kb = confirmed_block(world, "Ghost", body_with("old"));
        kb.status = "deleted".to_string();
        let old = vec![kb];
        let new = vec![("Ghost".to_string(), body_with("old"))];
        let diff = compute_kb_diff(&old, &new);
        // Deleted row is not active → "Ghost" looks newly extracted.
        assert_eq!(diff.inserted, vec!["Ghost".to_string()]);
        assert!(diff.updated.is_empty());
    }

    #[test]
    fn compute_diff_removed_advisory_when_name_vanishes() {
        let world = "wld_1";
        let old = vec![confirmed_block(world, "Gone", body_with("old"))];
        let new: Vec<(String, KeyBlockBody)> = vec![];
        let diff = compute_kb_diff(&old, &new);
        assert_eq!(diff.removed, vec!["Gone".to_string()]);
    }

    #[tokio::test]
    async fn diff_and_apply_refreshes_body_via_store() {
        let world = "wld_1";
        let store = InMemoryKbStore::with_validation_mode(crate::validation::ValidationMode::Novel);
        // Seed a confirmed KeyBlock the store owns.
        let seeded = confirmed_block(world, "Lin Xia", body_with("v1"));
        store.insert_key_block(seeded.clone()).await.unwrap();
        let old_rows = store.list_by_world(world).await.unwrap();

        let new = vec![("Lin Xia".to_string(), body_with("v2-edited"))];
        let diff = diff_and_apply(&store, world, &old_rows, &new)
            .await
            .unwrap();
        assert_eq!(diff.updated.len(), 1);

        // The stored body was refreshed.
        let after = store.list_by_world(world).await.unwrap();
        assert_eq!(
            after[0].body.as_ref().unwrap().summary.as_deref(),
            Some("v2-edited")
        );
    }

    #[tokio::test]
    async fn diff_and_apply_no_op_when_unchanged() {
        let world = "wld_1";
        let store = InMemoryKbStore::with_validation_mode(crate::validation::ValidationMode::Novel);
        store
            .insert_key_block(confirmed_block(world, "Lin Xia", body_with("same")))
            .await
            .unwrap();
        let old_rows = store.list_by_world(world).await.unwrap();

        let new = vec![("Lin Xia".to_string(), body_with("same"))];
        let diff = diff_and_apply(&store, world, &old_rows, &new)
            .await
            .unwrap();
        assert!(diff.is_empty(), "no body change → empty diff");
    }
}

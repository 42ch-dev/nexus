//! C-matrix families C and D (N3): capture/idempotency/receipts and candidate
//! lifecycle, on both real providers.
//!
//! Every scenario body is provider-agnostic and runs against the real
//! `nexus_core` local provider and the independent durable fixture provider.
//! The `_local` / `_remote` suffix follows matrix §3; C-029 is the one
//! remote-only row.
//!
//! C-015 is N4-only (N4-R + N4-T) and deliberately has no scenario here: the
//! unavailable long-running families are proven only as refusals by C-003 and
//! C-004. No ignored or placeholder test stands in for it.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use support::{
    bootstrap, candidate_ids, capture_body, context_scope, dismiss_body, error_code, error_details,
    grant_body, is_committed, receipt_id, receipt_windows, record_ids, result,
    retry_class, review_body, shared_scope, Actor, CharacterFixture, Kind, Matrix, CONTEXT,
    OTHER_CONTEXT, SUBJECT, TEXT,
};

async fn pending(actor: &Actor<'_>) -> Value {
    actor
        .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
        .await
}

fn ids(body: &Value) -> Vec<String> {
    body["candidates"]
        .as_array()
        .unwrap_or_else(|| panic!("a candidate page names its rows: {body}"))
        .iter()
        .map(|candidate| {
            candidate["candidate_id"]
                .as_str()
                .expect("candidate id")
                .to_owned()
        })
        .collect()
}

async fn recalled(actor: &Actor<'_>, selection: Value) -> Value {
    actor
        .recall(json!({
            "subject_id": SUBJECT,
            "selection": selection,
            "budget": { "max_items": 50 },
        }))
        .await
}

// ---------------------------------------------------------------------------
// C. Capture, idempotency and receipts
// ---------------------------------------------------------------------------

/// C-020: a capture receipt names its candidate and never the captured text;
/// the candidate is visible via `candidates.list` and excluded from recall.
async fn capture_receipt_is_content_free(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let committed = admin
        .capture(capture_body("k-c020", TEXT, &shared_scope()))
        .await;
    let candidate = candidate_ids(&committed);
    assert_eq!(candidate.len(), 1);
    let serialized = committed.to_string();
    assert!(
        !serialized.contains("northern gate"),
        "the receipt never carries the captured text: {committed}"
    );
    assert!(!serialized.contains("Mira"), "{committed}");

    // Candidate visibility: pending, with the immutable capture placement.
    let listing = pending(&admin).await;
    let row = listing["candidates"]
        .as_array()
        .expect("candidate rows")
        .iter()
        .find(|row| row["candidate_id"] == json!(candidate[0]))
        .unwrap_or_else(|| panic!("the captured candidate is listed: {listing}"));
    assert_eq!(row["status"], json!("pending"));
    assert_eq!(row["capture_scope"], shared_scope());

    // Exclusion from recall (MEM-R004): nothing is retained yet.
    let recalled = recalled(&admin, json!({ "mode": "subject_all" })).await;
    assert_eq!(recalled["total_authorized"], json!(0), "{recalled}");
    assert_eq!(recalled["records"], json!([]));

    let read = admin.get_receipt(receipt_id(&committed)).await;
    assert_eq!(read["receipt"]["status"], json!("committed"));
    assert_eq!(read["receipt"]["result"]["candidate_ids"][0], json!(candidate[0]));
    assert!(
        !read.to_string().contains("northern gate"),
        "the receipt read is just as content-free: {read}"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_020_capture_receipt_is_content_free_local() {
    capture_receipt_is_content_free(Matrix::local().await).await;
}

#[tokio::test]
async fn c_020_capture_receipt_is_content_free_remote() {
    capture_receipt_is_content_free(Matrix::remote().await).await;
}

/// C-021: an identical same-key capture retry replays the stored receipt and
/// creates no second candidate.
async fn identical_capture_retry_replays(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let body = capture_body("k-c021", TEXT, &shared_scope());
    let first = admin.capture(body.clone()).await;
    let first_id = receipt_id(&first).to_owned();
    let replay = admin.capture(body).await;
    assert_eq!(receipt_id(&replay), first_id);
    assert_eq!(candidate_ids(&replay), candidate_ids(&first));
    assert_eq!(
        ids(&pending(&admin).await).len(),
        1,
        "the replay inserted no second candidate"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_021_identical_capture_retry_replays_local() {
    identical_capture_retry_replays(Matrix::local().await).await;
}

#[tokio::test]
async fn c_021_identical_capture_retry_replays_remote() {
    identical_capture_retry_replays(Matrix::remote().await).await;
}

/// C-022: a changed payload under the same key is a terminal conflict and
/// leaves the original effect untouched.
async fn changed_payload_idempotency_conflict(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let first = admin
        .capture(capture_body("k-c022", TEXT, &shared_scope()))
        .await;
    let conflict = admin
        .capture(capture_body(
            "k-c022",
            "A different promise under the same idempotency key.",
            &shared_scope(),
        ))
        .await;
    assert_eq!(error_code(&conflict), "IDEMPOTENCY_CONFLICT", "{conflict}");
    assert_eq!(retry_class(&conflict), "terminal");
    let listing = pending(&admin).await;
    assert_eq!(ids(&listing), candidate_ids(&first));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_022_changed_payload_idempotency_conflict_local() {
    changed_payload_idempotency_conflict(Matrix::local().await).await;
}

#[tokio::test]
async fn c_022_changed_payload_idempotency_conflict_remote() {
    changed_payload_idempotency_conflict(Matrix::remote().await).await;
}

/// C-023: the response is lost after the commit; the caller resolves the
/// uncertainty with the same key and gets exactly one effect.
///
/// Local uses `inject_commit_fault(true)` (after COMMIT: effect committed,
/// response lost). The fixture uses its `AfterCommitBeforeResponse` point.
async fn response_loss_replays_one_effect(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    matrix.arm_lost_response();
    let body = capture_body("k-c023", TEXT, &shared_scope());
    let lost = matrix.admin().capture(body.clone()).await;
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN", "{lost}");
    assert_eq!(retry_class(&lost), "uncertain");

    let resolved = matrix.admin().capture(body).await;
    assert!(is_committed(&resolved), "same-key replay resolves it: {resolved}");
    let listing = pending(&matrix.admin()).await;
    assert_eq!(
        ids(&listing).len(),
        1,
        "the lost response committed exactly one effect: {listing}"
    );
    assert_eq!(candidate_ids(&resolved), ids(&listing));

    let read = matrix.admin().get_receipt(receipt_id(&resolved)).await;
    assert_eq!(read["receipt"]["status"], json!("committed"));
    let replay = matrix
        .admin()
        .capture(capture_body("k-c023", TEXT, &shared_scope()))
        .await;
    assert_eq!(receipt_id(&replay), receipt_id(&resolved));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_023_response_loss_replays_one_effect_local() {
    response_loss_replays_one_effect_pipeline(Matrix::local().await).await;
}

#[tokio::test]
async fn c_023_response_loss_replays_one_effect_remote() {
    response_loss_replays_one_effect_pipeline(Matrix::remote().await).await;
}

async fn response_loss_replays_one_effect_pipeline(matrix: Matrix) {
    response_loss_replays_one_effect(matrix).await;
}

/// C-024: an uncertain outcome resolves to the ORIGINAL committed receipt.
///
/// Phase A is the narrower, honestly labelled case: a pre-COMMIT failure rolls
/// the staged write back, so no effect survives the restart. Phase B is the
/// required history: the original attempt COMMITTED and only its response was
/// lost (the provider's after-commit seam), the effect and its terminal receipt
/// survive the restart, same-key replay resolves that original receipt, and a
/// later replay returns it again with one unchanged effect. No process kill is
/// claimed and no receipt-id oracle or third recovery channel is used.
async fn midcommit_crash_resolves_uncertain_receipt(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;

    // (A) Pre-COMMIT rollback: uncertain answer, no effect after the restart.
    matrix.arm_commit_boundary_failure();
    let staged = matrix
        .admin()
        .capture(capture_body("k-c024-precommit", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&staged), "OUTCOME_UNCERTAIN", "{staged}");
    assert_eq!(retry_class(&staged), "uncertain");
    let matrix = matrix.restart().await;
    let admin = matrix.admin();
    assert!(
        ids(&pending(&admin).await).is_empty(),
        "the rolled-back attempt committed no candidate"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "the rolled-back attempt left no pending receipt"
    );
    drop(admin);

    // (B) Original committed uncertainty: the commit happened, the response did
    // not, so the original receipt and effect are durable before the restart.
    matrix.arm_lost_response();
    let original = capture_body("k-c024-original", TEXT, &shared_scope());
    let lost = matrix.admin().capture(original.clone()).await;
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN", "{lost}");
    assert_eq!(retry_class(&lost), "uncertain");

    let matrix = matrix.restart().await;
    let admin = matrix.admin();
    let committed_effect = ids(&pending(&admin).await);
    assert_eq!(
        committed_effect.len(),
        1,
        "the original committed effect survived the restart: {committed_effect:?}"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "the original receipt is terminal (committed), not pending"
    );

    // Resolution: same-key replay returns the ORIGINAL receipt and effect.
    let resolved = admin.capture(original.clone()).await;
    assert!(is_committed(&resolved), "the same key resolves it: {resolved}");
    let resolved_id = receipt_id(&resolved).to_owned();
    assert_eq!(
        candidate_ids(&resolved),
        committed_effect,
        "the replay returns the original effect's candidate, not a fresh one"
    );
    let read = admin.get_receipt(&resolved_id).await;
    assert_eq!(
        read["receipt"]["status"], json!("committed"),
        "receipts.get resolves the uncertain key to its terminal receipt: {read}"
    );
    assert_eq!(
        read["receipt"]["result"]["candidate_ids"],
        json!(committed_effect),
        "the terminal receipt names the original effect"
    );

    let replay = admin.capture(original).await;
    assert_eq!(
        receipt_id(&replay),
        resolved_id,
        "the subsequent replay returns the same original receipt"
    );
    assert_eq!(
        ids(&pending(&admin).await),
        committed_effect,
        "exactly one effect exists and it is unchanged"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_024_midcommit_crash_resolves_uncertain_receipt_local() {
    midcommit_crash_resolves_uncertain_receipt(Matrix::local().await).await;
}

#[tokio::test]
async fn c_024_midcommit_crash_resolves_uncertain_receipt_remote() {
    midcommit_crash_resolves_uncertain_receipt(Matrix::remote().await).await;
}

/// C-025: a capture by a principal without capture coverage persists nothing.
async fn foreign_capture_persists_nothing(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let peer = matrix.peer();
    let denied = peer
        .capture(capture_body("k-c025", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");
    let admin = matrix.admin();
    assert!(ids(&pending(&admin).await).is_empty());
    assert_eq!(
        recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(0)
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "the refused foreign capture created no receipt"
    );

    // The same request from the same principal is admitted once coverage exists.
    let peer_id = matrix.peer_principal().principal_id.clone();
    admin
        .grant(grant_body("capture", &peer_id, "k-c025-grant"))
        .await;
    let admitted = peer
        .capture(capture_body("k-c025-admitted", TEXT, &shared_scope()))
        .await;
    assert!(is_committed(&admitted), "{admitted}");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_025_foreign_capture_persists_nothing_local() {
    foreign_capture_persists_nothing(Matrix::local().await).await;
}

#[tokio::test]
async fn c_025_foreign_capture_persists_nothing_remote() {
    foreign_capture_persists_nothing(Matrix::remote().await).await;
}

/// C-026: a capture placed into a fenced context has no effect.
async fn capture_to_fenced_target_has_no_effect(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    admin
        .fence_subject(
            SUBJECT,
            json!({ "type": "context", "context_id": CONTEXT }),
            "fence",
            1,
            "k-c026-fence",
        )
        .await;
    let denied = admin
        .capture(capture_body("k-c026", TEXT, &context_scope(CONTEXT)))
        .await;
    assert_eq!(error_code(&denied), "SUBJECT_FENCED", "{denied}");
    assert_eq!(
        error_details(&denied)["scope"],
        json!("context"),
        "the fenced-context write names its scope (§7.7): {denied}"
    );
    assert_eq!(retry_class(&denied), "terminal");

    // Protocol-visible absence of every effect the refused write could have had.
    assert!(ids(&pending(&admin).await).is_empty());
    assert_eq!(
        recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(0)
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0)
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_026_capture_to_fenced_target_has_no_effect_local() {
    capture_to_fenced_target_has_no_effect(Matrix::local().await).await;
}

#[tokio::test]
async fn c_026_capture_to_fenced_target_has_no_effect_remote() {
    capture_to_fenced_target_has_no_effect(Matrix::remote().await).await;
}

/// C-027: capture → review → delete; the OLD capture and review receipts still
/// resolve, but disclose ids/revisions/counts only — never deleted content.
async fn receipts_never_leak_deleted_content(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let captured = admin
        .capture(capture_body("k-c027-capture", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();
    let reviewed = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c027-review"))
        .await;
    assert!(is_committed(&reviewed), "{reviewed}");
    let record_id = record_ids(&reviewed)[0].clone();
    let deleted = admin
        .delete_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "reason": "superseded by C-027",
            "idempotency_key": "k-c027-delete",
        }))
        .await;
    assert_eq!(result(&deleted)["tombstoned"], json!(true));

    // Replays of the pre-deletion history still resolve to their receipts.
    let capture_replay = admin
        .capture(capture_body("k-c027-capture", TEXT, &shared_scope()))
        .await;
    assert_eq!(receipt_id(&capture_replay), receipt_id(&captured));
    let review_replay = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c027-review"))
        .await;
    assert_eq!(receipt_id(&review_replay), receipt_id(&reviewed));

    for receipt in [&captured, &reviewed, &deleted] {
        let served = admin.get_receipt(receipt_id(receipt)).await;
        assert!(
            !served.to_string().contains("northern gate"),
            "a receipt never restates deleted content: {served}"
        );
        assert!(
            !served.to_string().contains("Mira"),
            "a receipt never restates deleted content: {served}"
        );
        assert_eq!(served["receipt"]["status"], json!("committed"));
    }
    let shell = admin.get_candidate(&candidate_id).await;
    assert!(
        !shell.to_string().contains("northern gate"),
        "the consumed shell carries no content: {shell}"
    );

    // The record itself is a content-free tombstone now.
    let tombstone = admin.get_record(&record_id).await;
    assert_eq!(tombstone["record"]["content"]["text"], json!(""));
    assert!(!tombstone.to_string().contains("northern gate"));
    assert_eq!(
        recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(0)
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_027_receipts_never_leak_deleted_content_local() {
    receipts_never_leak_deleted_content(Matrix::local().await).await;
}

#[tokio::test]
async fn c_027_receipts_never_leak_deleted_content_remote() {
    receipts_never_leak_deleted_content(Matrix::remote().await).await;
}

/// C-028: the advertised receipt-body and dedup-retention windows govern what a
/// receipt and its key can still do, including the mandatory last known status.
async fn receipt_and_dedup_retention_windows(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let windows = receipt_windows(&matrix.admin().describe().await);
    let body = capture_body("k-c028", TEXT, &shared_scope());
    let first = matrix.admin().capture(body.clone()).await;
    let first_id = receipt_id(&first).to_owned();

    let live = matrix.admin().get_receipt(&first_id).await;
    assert_eq!(live["receipt"]["status"], json!("committed"));

    // Past the body window: the body is gone, the key stays bound.
    matrix.expire_receipts(false, windows).await;
    let expired = matrix.admin().get_receipt(&first_id).await;
    assert_eq!(error_code(&expired), "RECEIPT_EXPIRED", "{expired}");
    assert_eq!(
        error_details(&expired)["last_known_status"],
        json!("committed"),
        "RECEIPT_EXPIRED reports the tombstone's last known status: {expired}"
    );
    assert_eq!(retry_class(&expired), "terminal");
    let replay = matrix.admin().capture(body.clone()).await;
    assert_eq!(
        error_code(&replay),
        "RECEIPT_EXPIRED",
        "a key inside its dedup window never re-executes: {replay}"
    );
    assert_eq!(
        error_details(&replay)["last_known_status"],
        json!("committed")
    );
    assert_eq!(ids(&pending(&matrix.admin()).await).len(), 1);

    // Past the dedup window: the receipt is absent and the key is free again.
    matrix.expire_receipts(true, windows).await;
    let absent = matrix.admin().get_receipt(&first_id).await;
    assert_eq!(error_code(&absent), "NOT_FOUND", "{absent}");
    let fresh = matrix.admin().capture(body).await;
    assert!(is_committed(&fresh), "the freed key admits a new effect: {fresh}");
    assert_ne!(receipt_id(&fresh), first_id);
    assert_eq!(
        ids(&pending(&matrix.admin()).await).len(),
        2,
        "the new effect produced a second candidate"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_028_receipt_and_dedup_retention_windows_local() {
    receipt_and_dedup_retention_windows(Matrix::local().await).await;
}

#[tokio::test]
async fn c_028_receipt_and_dedup_retention_windows_remote() {
    receipt_and_dedup_retention_windows(Matrix::remote().await).await;
}

/// C-029 (remote only): reconnecting the signed session keeps the admitted
/// principal, its grants and its receipt/dedup namespace.
async fn reconnect_preserves_principal_and_receipt(matrix: Matrix) {
    bootstrap(&matrix, &["capture"]).await;
    let body = capture_body("k-c029", TEXT, &shared_scope());
    let first = matrix.peer().capture(body.clone()).await;
    let first_id = receipt_id(&first).to_owned();
    let principal_before = matrix.peer_principal().principal_id.clone();
    let space_before = matrix.space_id().to_owned();

    let mut matrix = matrix;
    matrix.reconnect_peer().await;
    assert_eq!(
        matrix.peer_principal().principal_id,
        principal_before,
        "the reconnect maps the same authenticated peer to the same principal"
    );
    assert_eq!(matrix.space_id(), space_before);

    let replay = matrix.peer().capture(body).await;
    assert_eq!(
        receipt_id(&replay),
        first_id,
        "the reconnected peer replays its own receipt instead of re-executing"
    );
    assert_eq!(candidate_ids(&replay), candidate_ids(&first));
    let read = matrix.peer().get_receipt(&first_id).await;
    assert_eq!(read["receipt"]["receipt_id"], json!(first_id));
    assert_eq!(ids(&pending(&matrix.admin()).await).len(), 1);
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_029_reconnect_preserves_principal_and_receipt_remote() {
    reconnect_preserves_principal_and_receipt(Matrix::remote().await).await;
}

// ---------------------------------------------------------------------------
// D. Candidate lifecycle
// ---------------------------------------------------------------------------

/// C-030: candidate pages are bounded, complete and equal to the authorized set;
/// a real inaccessible candidate and a missing id are indistinguishable.
async fn candidate_pages_are_bounded_and_scoped(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let mut seeded = Vec::new();
    for (key, scope) in [
        ("k-c030-a", context_scope(CONTEXT)),
        ("k-c030-b", context_scope(OTHER_CONTEXT)),
        ("k-c030-c", shared_scope()),
    ] {
        let captured = admin.capture(capture_body(key, TEXT, &scope)).await;
        assert!(is_committed(&captured), "{captured}");
        seeded.extend(candidate_ids(&captured));
    }

    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = admin
            .list_candidates(json!({
                "subject_id": SUBJECT,
                "limit": 1,
                "cursor": cursor,
            }))
            .await;
        let rows = ids(&page);
        assert!(rows.len() <= 1, "the page is bounded by the limit: {page}");
        if page["next_cursor"].is_string() {
            assert!(!rows.is_empty(), "a page before the end is never empty: {page}");
        }
        seen.extend(rows);
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    let mut expected = seeded.clone();
    expected.sort();
    let mut observed = seen.clone();
    observed.sort();
    observed.dedup();
    assert_eq!(observed, expected, "the page union equals the authorized seeded set");
    assert_eq!(seen.len(), seeded.len(), "no row is paged twice: {seen:?}");

    // A reviewer restricted to one context sees only the candidate it covers.
    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["review", "records_write"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c030-review",
        }))
        .await;
    assert!(is_committed(&granted), "{granted}");
    let scoped = ids(&pending(&matrix.peer()).await);
    assert_eq!(
        scoped,
        vec![seeded[0].clone()],
        "the scoped page is exactly the seeded authorized candidate in its context"
    );

    // A real candidate outside that context and an id this store never held are
    // the same NOT_FOUND-class refusal (C-102 anti-oracle).
    let inaccessible = matrix.peer().get_candidate(&seeded[1]).await;
    let missing = matrix.peer().get_candidate("cnd_never_existed").await;
    assert_eq!(error_code(&inaccessible), "NOT_FOUND", "{inaccessible}");
    assert_eq!(error_code(&missing), "NOT_FOUND", "{missing}");
    assert_eq!(
        error_code(&inaccessible),
        error_code(&missing),
        "a real inaccessible candidate is indistinguishable from a missing id"
    );
    assert_eq!(retry_class(&inaccessible), "terminal");
    assert_eq!(retry_class(&missing), "terminal");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_030_candidate_pages_are_bounded_and_scoped_local() {
    candidate_pages_are_bounded_and_scoped(Matrix::local().await).await;
}

#[tokio::test]
async fn c_030_candidate_pages_are_bounded_and_scoped_remote() {
    candidate_pages_are_bounded_and_scoped(Matrix::remote().await).await;
}

/// C-031: review consumption and record creation are one atomic outcome, the
/// consumed shell retains its content digest, and a replay returns that same
/// outcome.
async fn review_consumes_and_creates_atomically(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let captured = admin
        .capture(capture_body("k-c031", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();
    let captured_shell = admin.get_candidate(&candidate_id).await;
    assert_eq!(
        captured_shell["candidate"]["proposed"]["content"]["text"],
        json!(TEXT),
        "a pending candidate still carries its proposed body: {captured_shell}"
    );
    // Authoritative digest: §6.7 pins the JCS SHA-256 of the candidate content,
    // which the public pending projection exposes. Compute it before the body is
    // scrubbed; the consumed shell must retain exactly this value.
    let expected_digest = match spoke_operations::jcs_sha256_hex(
        &captured_shell["candidate"]["proposed"]["content"],
    ) {
        spoke_operations::SpokeResult::Ok(digest) => digest,
        spoke_operations::SpokeResult::Reject(reject) => {
            panic!("the public pending content canonicalizes: {}", reject.message)
        }
    };

    let reviewed = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c031-review"))
        .await;
    assert_eq!(result(&reviewed)["outcome"], json!("promoted"));
    let records = record_ids(&reviewed);
    assert_eq!(records.len(), 1);

    let shell = admin.get_candidate(&candidate_id).await;
    let candidate = &shell["candidate"];
    assert_eq!(candidate["status"], json!("consumed"));
    assert!(
        candidate["proposed"].get("content").is_none(),
        "consumption removes the proposed body: {candidate}"
    );
    assert_eq!(
        candidate["content_digest"],
        json!(expected_digest),
        "the consumed shell retains the authoritative content digest: {candidate}"
    );
    assert_eq!(
        candidate["consumed_by"]["receipt_id"],
        json!(receipt_id(&reviewed)),
        "the same receipt carries the consumption"
    );
    let record = admin.get_record(&records[0]).await;
    assert_eq!(record["record"]["content"]["text"], json!(TEXT));
    assert_eq!(record["record"]["scope"], shared_scope());

    let replay = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c031-review"))
        .await;
    assert_eq!(receipt_id(&replay), receipt_id(&reviewed));
    assert_eq!(record_ids(&replay), records);

    let again = admin
        .review_candidate(review_body(&candidate_id, 2, "k-c031-again"))
        .await;
    assert_eq!(error_code(&again), "CANDIDATE_CONSUMED", "{again}");
    assert_eq!(
        recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(1),
        "exactly one record exists"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_031_review_consumes_and_creates_atomically_local() {
    review_consumes_and_creates_atomically(Matrix::local().await).await;
}

#[tokio::test]
async fn c_031_review_consumes_and_creates_atomically_remote() {
    review_consumes_and_creates_atomically(Matrix::remote().await).await;
}

/// C-032 (I-000149): dismissing consumes the candidate without a record, and
/// neither a replay nor a restart can resurrect it.
async fn dismiss_replay_never_resurrects(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let captured = admin
        .capture(capture_body("k-c032", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();

    let dismissed = admin
        .dismiss_candidate(dismiss_body(&candidate_id, 1, "k-c032-dismiss"))
        .await;
    assert_eq!(result(&dismissed)["outcome"], json!("dismissed"));
    let replay = admin
        .dismiss_candidate(dismiss_body(&candidate_id, 1, "k-c032-dismiss"))
        .await;
    assert_eq!(receipt_id(&replay), receipt_id(&dismissed));
    let second = admin
        .dismiss_candidate(dismiss_body(&candidate_id, 2, "k-c032-again"))
        .await;
    assert_eq!(error_code(&second), "CANDIDATE_CONSUMED", "{second}");
    drop(admin);

    let matrix = matrix.restart().await;
    let admin = matrix.admin();
    let shell = admin.get_candidate(&candidate_id).await;
    assert_eq!(
        shell["candidate"]["status"], json!("consumed"),
        "the restart never resurrects the candidate: {shell}"
    );
    assert!(ids(&pending(&admin).await).is_empty());
    assert_eq!(
        recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(0),
        "a dismissal produced no record"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_032_dismiss_replay_never_resurrects_local() {
    dismiss_replay_never_resurrects(Matrix::local().await).await;
}

#[tokio::test]
async fn c_032_dismiss_replay_never_resurrects_remote() {
    dismiss_replay_never_resurrects(Matrix::remote().await).await;
}

/// C-033: two ACTUAL signed clients review one candidate. Both are observed at
/// the provider's own admission boundary (the seam acknowledges the arrival,
/// not elapsed client time) while the writer is held; exactly one commits, the
/// competing request gets exactly `CANDIDATE_CONSUMED`, and one effect remains.
async fn concurrent_review_has_exactly_one_effect(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let peer_id = matrix.peer_principal().principal_id.clone();
    // One grant carrying both required capabilities over the candidate's shared
    // capture scope (the single-grant rule).
    let granted = matrix
        .admin()
        .grant(json!({
            "principal_id": peer_id,
            "capabilities": ["review", "records_write"],
            "idempotency_key": "k-c033-peer",
        }))
        .await;
    assert!(is_committed(&granted), "{granted}");

    // Fixture target-arm lifecycle on real stale fences. The control handle is
    // kept ALIVE across each fence whenever the case is an observation
    // cancellation: with a live release sender, a stale arm would park the fence
    // until the arm bound, so a prompt refusal is causal. A dropped handle's
    // closed sender would proceed anyway and is therefore never used as proof.
    if matrix.kind() == Kind::Remote {
        // Unpolled observation cancelled, handle retained.
        let unpolled = matrix.park_next_target("memory.subjects.fence");
        let mut observation = unpolled;
        {
            let future = observation.wait_reached();
            drop(future);
        }
        let refused = tokio::time::timeout(
            Duration::from_secs(3),
            matrix.admin().fence_subject(
                SUBJECT,
                json!({ "type": "subject" }),
                "fence",
                99,
                "k-c033-unpolled",
            ),
        )
        .await
        .expect("a cancelled observation leaves no stale arm while its handle lives");
        assert_eq!(error_code(&refused), "REVISION_CONFLICT", "{refused}");
        drop(observation);

        // Replacement: dropping the old handle must not clear the newer arm, and
        // the replacement is the one the fence really pauses on.
        let admin = matrix.admin();
        let old = matrix.park_next_target("memory.subjects.fence");
        let mut replacement = matrix.park_next_target("memory.subjects.fence");
        drop(old);
        let fence = admin.fence_subject(
            SUBJECT,
            json!({ "type": "subject" }),
            "fence",
            99,
            "k-c033-replaced",
        );
        tokio::pin!(fence);
        tokio::select! {
            completed = &mut fence => panic!("the replaced fence completed before reaching the arm: {completed:?}"),
            reached = replacement.wait_reached() => reached.expect("the replacement arm owns the next fence"),
        }
        replacement.release().await;
        let refused = tokio::time::timeout(Duration::from_secs(10), &mut fence)
            .await
            .expect("the replaced fence completes");
        assert_eq!(error_code(&refused), "REVISION_CONFLICT", "{refused}");

        // Controller LOSS after take: the fence took the arm, the controller is
        // really dropped, and the fence reaches its ordinary refusal boundedly.
        let admin = matrix.admin();
        let mut loss = matrix.park_next_target("memory.subjects.fence");
        let fence = admin.fence_subject(
            SUBJECT,
            json!({ "type": "subject" }),
            "fence",
            99,
            "k-c033-loss",
        );
        tokio::pin!(fence);
        tokio::select! {
            completed = &mut fence => panic!("the loss fence completed before reaching the arm: {completed:?}"),
            reached = loss.wait_reached() => reached.expect("the fence took the arm"),
        }
        drop(loss);
        let refused = tokio::time::timeout(Duration::from_secs(15), &mut fence)
            .await
            .expect("the fence completes boundedly after controller loss");
        assert_eq!(error_code(&refused), "REVISION_CONFLICT", "{refused}");

        // Controller RETAINED past the arm's own bound (expiry): the fence still
        // refuses boundedly, and the late release returns instead of hanging.
        let admin = matrix.admin();
        let mut expiry =
            matrix.park_next_target_bounded("memory.subjects.fence", Duration::from_secs(2));
        let fence = admin.fence_subject(
            SUBJECT,
            json!({ "type": "subject" }),
            "fence",
            99,
            "k-c033-expiry",
        );
        tokio::pin!(fence);
        tokio::select! {
            completed = &mut fence => panic!("the expiry fence completed before reaching the arm: {completed:?}"),
            reached = expiry.wait_reached() => reached.expect("the fence took the arm"),
        }
        let refused = tokio::time::timeout(Duration::from_secs(10), &mut fence)
            .await
            .expect("the fence completes after the arm bound expires");
        assert_eq!(error_code(&refused), "REVISION_CONFLICT", "{refused}");
        tokio::time::timeout(Duration::from_secs(3), expiry.release())
            .await
            .expect("a late release after arm expiry is bounded");
    }

    let captured = matrix
        .admin()
        .capture(capture_body("k-c033", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();

    // Both arrivals are established before the held request is released.
    // - Local: the held review owns the candidate's disposition section. The
    //   peer review's disposition acquisition reports that it is really about to
    //   block on that section — a provider-side acknowledgment of ITS arrival,
    //   not elapsed time and not an OS lock-wait claim.
    // - Fixture: the held review is acknowledged at its exact op-targeted handler
    //   admission; the peer review runs to a durable committed receipt before the
    //   release, which is its real arrival and effect.
    let mut arrival = match matrix.kind() {
        Kind::Local => matrix.park_next_admission(),
        Kind::Remote => matrix.park_next_target("memory.candidates.review"),
    };
    let admin_review = tokio::spawn(support::review_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        matrix.space_id().to_owned(),
        review_body(&candidate_id, 1, "k-c033-admin"),
    ));
    arrival
        .wait_reached()
        .await
        .expect("the admin review reached the provider admission boundary");

    let mut peer_task = None;
    let mut peer_completed = None;
    match matrix.kind() {
        Kind::Local => {
            // The held review owns the candidate's disposition section; the peer
            // review's acquisition reports that it is really about to block on
            // that section — a provider-side acknowledgment of ITS arrival.
            let waiter = Arc::new(tokio::sync::Notify::new());
            matrix.report_disposition_waits(Arc::clone(&waiter));
            peer_task = Some(tokio::spawn(support::review_with(
                matrix.peer_client(),
                matrix.peer_principal().clone(),
                matrix.space_id().to_owned(),
                review_body(&candidate_id, 1, "k-c033-peer"),
            )));
            tokio::time::timeout(support::BOUND, waiter.notified())
                .await
                .expect("the peer review reached the disposition wait");
        }
        Kind::Remote => {
            peer_completed = Some(
                support::review_with(
                    matrix.peer_client(),
                    matrix.peer_principal().clone(),
                    matrix.space_id().to_owned(),
                    review_body(&candidate_id, 1, "k-c033-peer"),
                )
                .await,
            );
        }
    }

    arrival.release().await;
    let admin_outcome = admin_review.await.expect("the admin review completes");
    let peer_outcome = match peer_completed {
        Some(outcome) => outcome,
        None => peer_task
            .expect("the peer review was issued")
            .await
            .expect("the peer review completes"),
    };

    let winners = [&admin_outcome, &peer_outcome]
        .into_iter()
        .filter(|body| is_committed(body))
        .count();
    assert_eq!(
        winners, 1,
        "exactly one of the two admitted clients commits: {admin_outcome} / {peer_outcome}"
    );
    let (winner, loser) = if is_committed(&admin_outcome) {
        (&admin_outcome, &peer_outcome)
    } else {
        (&peer_outcome, &admin_outcome)
    };
    assert_eq!(
        error_code(loser),
        "CANDIDATE_CONSUMED",
        "the competing review gets the retained historical loser code: {loser}"
    );
    assert_eq!(retry_class(loser), "terminal");
    assert_eq!(record_ids(winner).len(), 1);

    let observed = recalled(&matrix.admin(), json!({ "mode": "subject_all" })).await;
    assert_eq!(
        observed["total_authorized"],
        json!(1),
        "the pair produced exactly one record: {observed}"
    );
    assert_eq!(
        matrix.admin().get_candidate(&candidate_id).await["candidate"]["status"],
        json!("consumed")
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_033_concurrent_review_has_exactly_one_effect_local() {
    concurrent_review_has_exactly_one_effect(Matrix::local().await).await;
}

#[tokio::test]
async fn c_033_concurrent_review_has_exactly_one_effect_remote() {
    concurrent_review_has_exactly_one_effect(Matrix::remote().await).await;
}

/// C-034: a review inherits the candidate's capture scope and needs BOTH the
/// review and records_write coverage over it IN ONE GRANT; the later visibility
/// change is a separate, explicitly authorized `records.promote`.
async fn review_inherits_scope_and_requires_dual_promote_grant(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let captured = admin
        .capture(capture_body("k-c034", TEXT, &context_scope(CONTEXT)))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();
    let peer_id = matrix.peer_principal().principal_id.clone();

    // 1. `review` alone is not enough (the strong cause of the refusal is the
    //    missing records_write coverage).
    let review_only = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["review"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c034-review",
        }))
        .await;
    assert!(is_committed(&review_only), "{review_only}");
    let refused = matrix
        .peer()
        .review_candidate(review_body(&candidate_id, 1, "k-c034-review-only"))
        .await;
    assert!(!is_committed(&refused), "review alone cannot promote: {refused}");
    assert_eq!(retry_class(&refused), "terminal");
    assert!(record_ids_or_empty(&refused).is_empty());
    assert_eq!(
        matrix.admin().get_candidate(&candidate_id).await["candidate"]["status"],
        json!("pending"),
        "the refused review consumed nothing"
    );

    // 2. Same-scope SEPARATE review + records_write grants never union into an
    //    admission (protocol §6.5 single-grant rule); zero effect.
    let second_grant = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["records_write"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c034-write",
        }))
        .await;
    assert!(is_committed(&second_grant), "{second_grant}");
    let separate = matrix
        .peer()
        .review_candidate(review_body(&candidate_id, 1, "k-c034-separate"))
        .await;
    assert!(!is_committed(&separate), "separate grants do not union: {separate}");
    assert_eq!(retry_class(&separate), "terminal");
    assert_eq!(error_code(&separate), "NOT_FOUND", "{separate}");
    assert!(record_ids_or_empty(&separate).is_empty());
    assert_eq!(
        matrix.admin().get_candidate(&candidate_id).await["candidate"]["status"],
        json!("pending"),
        "the refused separate-grant review consumed nothing"
    );

    // 3. ONE grant carrying both capabilities over the same scope is admitted
    //    and the created record inherits the context placement verbatim.
    let combined = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["review", "records_write"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c034-dual",
        }))
        .await;
    assert!(is_committed(&combined), "{combined}");
    let promoted = matrix
        .peer()
        .review_candidate(review_body(&candidate_id, 1, "k-c034-promote"))
        .await;
    assert_eq!(result(&promoted)["outcome"], json!("promoted"), "{promoted}");
    let record_id = record_ids(&promoted)[0].clone();
    let record = admin.get_record(&record_id).await;
    assert_eq!(
        record["record"]["scope"],
        context_scope(CONTEXT),
        "the record inherits the candidate's capture scope: {record}"
    );

    // 4. Shared visibility needs the explicit dual-authorized promote. The peer
    //    may read the context-placed record but cannot promote it (no `promote`
    //    coverage), and the state never widens.
    let reader = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["records_read"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c034-read",
        }))
        .await;
    assert!(is_committed(&reader), "{reader}");
    assert_eq!(
        matrix.peer().get_record(&record_id).await["record"]["record_id"],
        json!(record_id),
        "the context-limited reader sees the binding-placed record"
    );
    let unauthorized = matrix
        .peer()
        .promote_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "target_scope": shared_scope(),
            "idempotency_key": "k-c034-promote-denied",
        }))
        .await;
    assert!(
        matches!(error_code(&unauthorized), "NOT_AUTHORIZED" | "NOT_FOUND"),
        "the unauthorized promote is a classified terminal refusal: {unauthorized}"
    );
    assert_eq!(retry_class(&unauthorized), "terminal");
    let unchanged = admin.get_record(&record_id).await;
    assert_eq!(
        unchanged["record"]["scope"],
        context_scope(CONTEXT),
        "the refused promotion changed no placement: {unchanged}"
    );
    assert_eq!(unchanged["record"]["revision"], json!(1));

    let widened = admin
        .promote_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "target_scope": shared_scope(),
            "idempotency_key": "k-c034-promote",
        }))
        .await;
    assert_eq!(result(&widened)["scope"], shared_scope(), "{widened}");
    assert_eq!(result(&widened)["revision"], json!(2));
    assert_eq!(
        error_code(&matrix.peer().get_record(&record_id).await),
        "NOT_FOUND",
        "the context-limited reader loses the record once it becomes shared"
    );
    assert_eq!(
        recalled(&admin, json!({ "mode": "shared_only" })).await["total_authorized"],
        json!(1)
    );
    matrix.shutdown().await;
}

fn record_ids_or_empty(body: &Value) -> Vec<String> {
    if is_committed(body) {
        return record_ids(body);
    }
    Vec::new()
}

#[tokio::test]
async fn c_034_review_inherits_scope_and_requires_dual_promote_grant_local() {
    review_inherits_scope_and_requires_dual_promote_grant(Matrix::local().await).await;
}

#[tokio::test]
async fn c_034_review_inherits_scope_and_requires_dual_promote_grant_remote() {
    review_inherits_scope_and_requires_dual_promote_grant(Matrix::remote().await).await;
}

/// C-035: the historical `merge` review action is not part of the protocol and
/// is refused at the typed boundary with zero effects.
async fn merge_review_action_is_explicitly_unsupported(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let captured = matrix
        .admin()
        .capture(capture_body("k-c035", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();

    let mut raw = matrix.raw_admin().await;
    let response = raw
        .invoke(
            "memory.candidates.review",
            &json!({
                "space_id": matrix.space_id(),
                "candidate_id": candidate_id,
                "expected_revision": 1,
                "decision": { "action": "merge" },
                "idempotency_key": "k-c035-merge",
            }),
        )
        .await;
    let error = response.get("error").expect("typed-decode refusal envelope");
    assert_eq!(error["code"], json!("INVALID_INPUT"), "{response}");
    assert_eq!(error["details"]["retry_class"], json!("terminal"));
    raw.close().await;

    let shell = matrix.admin().get_candidate(&candidate_id).await;
    assert_eq!(
        shell["candidate"]["status"], json!("pending"),
        "the refused merge action consumed nothing: {shell}"
    );
    assert_eq!(shell["candidate"]["revision"], json!(1));
    assert_eq!(
        recalled(&matrix.admin(), json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(0)
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_035_merge_review_action_is_explicitly_unsupported_local() {
    merge_review_action_is_explicitly_unsupported(Matrix::local().await).await;
}

#[tokio::test]
async fn c_035_merge_review_action_is_explicitly_unsupported_remote() {
    merge_review_action_is_explicitly_unsupported(Matrix::remote().await).await;
}

/// C-036: replaying a committed CAPTURE after a fence/revision change returns
/// the stored receipt without re-running the stale fence/CAS checks; a fresh
/// capture still observes the fence. The review replay is additional evidence.
async fn replay_does_not_recheck_current_fence_or_cas(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let body = capture_body("k-c036-capture", TEXT, &shared_scope());
    let captured = admin.capture(body.clone()).await;
    assert!(is_committed(&captured), "{captured}");
    let candidate_id = candidate_ids(&captured)[0].clone();
    let reviewed = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c036-review"))
        .await;
    assert!(is_committed(&reviewed), "{reviewed}");

    // The subject is fenced after both decisions were committed.
    admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "fence", 1, "k-c036-fence")
        .await;

    // The specified operation: a CAPTURE replay is the stored receipt.
    let replay = admin.capture(body).await;
    assert_eq!(
        receipt_id(&replay),
        receipt_id(&captured),
        "the capture replay returns the stored receipt, not a fresh decision: {replay}"
    );
    assert_eq!(candidate_ids(&replay), candidate_ids(&captured));

    // Additional evidence: the review replay behaves the same way.
    let review_replay = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c036-review"))
        .await;
    assert_eq!(receipt_id(&review_replay), receipt_id(&reviewed));

    // A fresh operation still observes the fence.
    let fresh = admin
        .capture(capture_body("k-c036-fresh", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&fresh), "SUBJECT_FENCED", "{fresh}");
    assert_eq!(
        recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(1),
        "the replays created no second record"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_036_replay_does_not_recheck_current_fence_or_cas_local() {
    replay_does_not_recheck_current_fence_or_cas(Matrix::local().await).await;
}

#[tokio::test]
async fn c_036_replay_does_not_recheck_current_fence_or_cas_remote() {
    replay_does_not_recheck_current_fence_or_cas(Matrix::remote().await).await;
}

/// C-116: a binding-local review of a real Character subject cannot widen
/// visibility; shared placement needs the explicit dual-authorized promote.
async fn binding_local_review_cannot_widen_visibility(
    matrix: Matrix,
    character: CharacterFixture,
) {
    let subject = character.subject_id.as_str();
    let binding = character.binding_id.as_str();
    let admin = matrix.admin();
    let ensured = admin
        .ensure_subject(subject)
        .await;
    assert_eq!(result(&ensured)["created"], json!(true));

    let captured = admin
        .capture(json!({
            "subject_id": subject,
            "content": { "text": TEXT },
            "capture_scope": { "visibility": "context", "context_id": binding },
            "source_context_ids": [binding],
            "kind_hint": "fact",
            "idempotency_key": "k-c116",
        }))
        .await;
    assert!(is_committed(&captured), "{captured}");
    let candidate_id = candidate_ids(&captured)[0].clone();
    let promoted = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c116-review"))
        .await;
    assert_eq!(result(&promoted)["outcome"], json!("promoted"), "{promoted}");
    let record_id = record_ids(&promoted)[0].clone();
    let record = admin.get_record(&record_id).await;
    assert_eq!(
        record["record"]["scope"],
        json!({ "visibility": "context", "context_id": binding }),
        "the promoted record keeps the binding capture scope: {record}"
    );
    assert_eq!(record["record"]["subject_id"], json!(subject));

    // Shared-only visibility never sees the binding-placed record.
    let shared_only = admin
        .recall(json!({
            "subject_id": subject,
            "selection": { "mode": "shared_only" },
            "budget": { "max_items": 10 },
        }))
        .await;
    assert_eq!(shared_only["total_authorized"], json!(0), "{shared_only}");
    let own_binding = admin
        .recall(json!({
            "subject_id": subject,
            "selection": {
                "mode": "selected_contexts",
                "context_ids": [binding],
                "include_shared": false,
            },
            "budget": { "max_items": 10 },
        }))
        .await;
    assert_eq!(own_binding["total_authorized"], json!(1), "{own_binding}");

    // An explicit promote needs promote + records_write on both placements; an
    // unauthorized attempt changes nothing.
    let peer_id = matrix.peer_principal().principal_id.clone();
    let reader = admin
        .grant(json!({
            "subject_id": subject,
            "principal_id": peer_id,
            "capabilities": ["records_read"],
            "context_ids": [binding],
            "shared_access": false,
            "idempotency_key": "k-c116-read",
        }))
        .await;
    assert!(is_committed(&reader), "{reader}");
    assert_eq!(
        matrix.peer().get_record(&record_id).await["record"]["record_id"],
        json!(record_id)
    );
    let denied = matrix
        .peer()
        .promote_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "target_scope": shared_scope(),
            "idempotency_key": "k-c116-promote-denied",
        }))
        .await;
    assert!(
        matches!(error_code(&denied), "NOT_AUTHORIZED" | "NOT_FOUND"),
        "the unauthorized promote is a classified terminal refusal: {denied}"
    );
    assert_eq!(retry_class(&denied), "terminal");
    assert_eq!(
        admin.get_record(&record_id).await["record"]["scope"],
        json!({ "visibility": "context", "context_id": binding }),
        "the refused promotion did not widen the record"
    );

    // The explicit authorized promote is the only widening path.
    let widened = admin
        .promote_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "target_scope": shared_scope(),
            "idempotency_key": "k-c116-promote",
        }))
        .await;
    assert_eq!(result(&widened)["scope"], shared_scope(), "{widened}");
    assert_eq!(
        error_code(&matrix.peer().get_record(&record_id).await),
        "NOT_FOUND",
        "the binding-limited reader loses the record after the explicit promote"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_116_binding_local_review_cannot_widen_visibility_local() {
    let (matrix, character) = Matrix::local_character().await;
    binding_local_review_cannot_widen_visibility(matrix, character).await;
}

#[tokio::test]
async fn c_116_binding_local_review_cannot_widen_visibility_remote() {
    let (matrix, character) = Matrix::remote_character().await;
    binding_local_review_cannot_widen_visibility(matrix, character).await;
}

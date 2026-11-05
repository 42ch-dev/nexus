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

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde_json::{json, Value};

use support::{
    bootstrap, candidate_ids, capture_body, context_scope, dismiss_body, error_code, error_details,
    grant_body, is_committed, receipt_id, receipt_windows, record_ids, result,
    retry_class, review_body, shared_scope, Actor, CharacterFixture, Kind, Matrix, CONTEXT,
    OTHER_CONTEXT, SUBJECT, TEXT,
};

/// A bounded observation that a request has reached the contested boundary and
/// has not completed yet.
async fn still_pending<F: Future>(future: &mut Pin<Box<F>>, what: &str) {
    match tokio::time::timeout(Duration::from_millis(400), future).await {
        Err(_) => {}
        Ok(_) => panic!("{what} completed before the contested boundary was released"),
    }
}

async fn pending(actor: &Actor<'_>) -> Value {
    actor
        .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
        .await
}

async fn pending_for(actor: &Actor<'_>, subject: &str) -> Value {
    actor
        .list_candidates(json!({ "subject_id": subject, "status": "pending" }))
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

async fn hold_writer(matrix: &Matrix) -> (support::CaptureHold, tokio::task::JoinHandle<Value>) {
    let mut hold = matrix.park_next_capture();
    let capture = tokio::spawn(support::capture_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-hold-writer", TEXT, &shared_scope()),
    ));
    hold.wait_reached()
        .await
        .expect("the holding capture reached the writer boundary");
    (hold, capture)
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

/// C-024: a commit-boundary crash answers `OUTCOME_UNCERTAIN`; the uncertain
/// receipt is resolved to a terminal receipt by same-key replay, `receipts.get`
/// confirms the terminal state, and a later replay returns the same receipt.
///
/// The fixture additionally keeps the staged-but-uncommitted cancellation as its
/// narrower rollback evidence. The local provider exposes no staged-write park
/// (its capture park sits before any insert), so its commit-boundary seam is the
/// applicable one there; no process kill is claimed anywhere.
async fn midcommit_crash_resolves_uncertain_receipt(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let space_id = matrix.space_id().to_owned();

    // Narrower evidence (fixture): cancellation with the effect staged and not
    // committed leaves nothing durable after the restart.
    if matrix.kind() == Kind::Remote {
        let staged = capture_body("k-c024-staged", TEXT, &shared_scope());
        let mut hold = matrix.park_next_capture();
        let capture = tokio::spawn(support::capture_with(
            matrix.admin_client(),
            matrix.admin_principal().clone(),
            space_id.clone(),
            staged,
        ));
        hold.wait_reached()
            .await
            .expect("the capture reached the staged, uncommitted point");
        capture.abort();
        let _ = capture.await;
    }

    // The declared uncertainty boundary: the commit does not complete and the
    // answer is still uncertain.
    matrix.arm_commit_boundary_failure();
    let body = capture_body("k-c024", TEXT, &shared_scope());
    let lost = matrix.admin().capture(body.clone()).await;
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN", "{lost}");
    assert_eq!(retry_class(&lost), "uncertain");

    // The crash leaves no committed effect, also across the restart.
    let matrix = matrix.restart().await;
    let admin = matrix.admin();
    assert!(
        ids(&pending(&admin).await).is_empty(),
        "the crashed attempt committed no candidate"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "the crashed attempt left no pending receipt"
    );

    // Resolution: same-key replay returns the committed terminal receipt.
    let resolved = admin.capture(body.clone()).await;
    assert!(is_committed(&resolved), "the same key resolves it: {resolved}");
    let resolved_id = receipt_id(&resolved).to_owned();
    let read = admin.get_receipt(&resolved_id).await;
    assert_eq!(
        read["receipt"]["status"], json!("committed"),
        "receipts.get resolves the uncertain key to its terminal receipt: {read}"
    );
    assert_eq!(
        read["receipt"]["result"]["candidate_ids"][0],
        json!(candidate_ids(&resolved)[0])
    );

    let replay = admin.capture(body).await;
    assert_eq!(
        receipt_id(&replay),
        resolved_id,
        "the subsequent replay returns the same terminal receipt"
    );
    assert_eq!(
        ids(&pending(&admin).await).len(),
        1,
        "exactly one effect exists after resolution"
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
    assert_eq!(scoped.len(), 1, "only the covered candidate is listed: {scoped:?}");

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
    let retained_digest = candidate["content_digest"]
        .as_str()
        .expect("the consumed shell retains its content digest");
    assert_eq!(retained_digest.len(), 64, "§6.1 64 lowercase hex chars: {candidate}");
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

/// C-033: two ACTUAL signed clients review one candidate while the writer lock
/// is held. Exactly one commits; the competing request is observed in flight at
/// the contested boundary and gets exactly `CANDIDATE_CONSUMED`.
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

    let captured = matrix
        .admin()
        .capture(capture_body("k-c033", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();

    // Hold the writer lock so both real clients are issued while the contested
    // boundary is occupied, and observe both still pending before releasing.
    let (mut hold, holder) = hold_writer(&matrix).await;
    let mut admin_review = Box::pin(support::review_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        matrix.space_id().to_owned(),
        review_body(&candidate_id, 1, "k-c033-admin"),
    ));
    let mut peer_review = Box::pin(support::review_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        review_body(&candidate_id, 1, "k-c033-peer"),
    ));
    still_pending(&mut admin_review, "the admin review").await;
    still_pending(&mut peer_review, "the peer review").await;

    hold.release().await;
    let _ = holder.await.expect("the holding capture completes");
    let admin_outcome = admin_review.await;
    let peer_outcome = peer_review.await;

    let winners = [&admin_outcome, &peer_outcome]
        .into_iter()
        .filter(|body| is_committed(body))
        .count();
    assert_eq!(
        winners, 1,
        "exactly one of the two real clients commits: {admin_outcome} / {peer_outcome}"
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

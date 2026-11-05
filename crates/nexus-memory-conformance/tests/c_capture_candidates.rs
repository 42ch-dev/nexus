//! C-matrix families C and D (N3): capture/idempotency/receipts and candidate
//! lifecycle, on both real providers.
//!
//! Every scenario body is provider-agnostic and runs twice: once against the
//! real `nexus_core` local provider and once against the independent durable
//! fixture provider. The `_local` / `_remote` suffix follows matrix §3;
//! C-029 is the one remote-only row.
//!
//! C-015 is N4-only (N4-R + N4-T) and deliberately has no scenario here: the
//! unavailable long-running families are proven only as refusals by C-003 and
//! C-004. No ignored or placeholder test stands in for it.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use serde_json::{json, Value};

use support::{
    bootstrap, candidate_ids, capture_body, context_grant_body, context_scope, dismiss_body,
    error_code, grant_body, is_committed, receipt_windows, record_body, record_ids, result,
    retry_class, review_body, review_with, shared_scope, Actor, Kind, Matrix, CONTEXT,
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

/// C-020: a capture receipt names its candidate and never the captured text.
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

    let read = admin.get_receipt(support::receipt_id(&committed)).await;
    let served = &read["receipt"];
    assert_eq!(served["status"], json!("committed"));
    assert_eq!(served["result"]["candidate_ids"][0], json!(candidate[0]));
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
    let first_id = support::receipt_id(&first).to_owned();
    let replay = admin.capture(body).await;
    assert_eq!(support::receipt_id(&replay), first_id);
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
/// Both are the provider's documented after-commit loss boundary.
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
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_023_response_loss_replays_one_effect_local() {
    response_loss_replays_one_effect(Matrix::local().await).await;
}

#[tokio::test]
async fn c_023_response_loss_replays_one_effect_remote() {
    response_loss_replays_one_effect(Matrix::remote().await).await;
}

/// C-024: the commit boundary stops the write. Nothing is committed, and the
/// uncertainty is resolved by the same key later.
///
/// The two legs use the boundary each provider actually exposes:
/// - Local: `inject_commit_fault(false)` — the committing transaction fails
///   before `COMMIT` (no effect) and the answer is still `OUTCOME_UNCERTAIN`.
///   The local provider's capture park sits before any insert, so it is an
///   admission/cancellation point and is NOT used as a mid-commit proof here.
/// - Fixture: the held `BeforeCommit` point, which is taken with the effect,
///   receipt, dedup row and target staged but not committed; the client future
///   is cancelled there, and the session teardown rolls the open transaction
///   back.
async fn midcommit_crash_resolves_uncertain_receipt(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let body = capture_body("k-c024", TEXT, &shared_scope());

    match matrix.kind() {
        Kind::Local => {
            matrix.arm_commit_boundary_failure();
            let lost = matrix.admin().capture(body.clone()).await;
            assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN", "{lost}");
            assert_eq!(retry_class(&lost), "uncertain");
        }
        Kind::Remote => {
            let mut hold = matrix.park_next_capture();
            let capture = tokio::spawn(support::capture_with(
                matrix.admin_client(),
                matrix.admin_principal().clone(),
                matrix.space_id().to_owned(),
                body.clone(),
            ));
            hold.wait_reached()
                .await
                .expect("the capture reached the staged, uncommitted point");
            // The caller is gone while the transaction is still open.
            capture.abort();
            let _ = capture.await;
        }
    }

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

    let resolved = admin.capture(body).await;
    assert!(is_committed(&resolved), "the same key resolves it: {resolved}");
    assert_eq!(
        ids(&pending(&admin).await).len(),
        1,
        "the retry committed exactly one effect"
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
    assert_eq!(recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"], json!(0));
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
    assert_eq!(retry_class(&denied), "terminal");
    assert!(ids(&pending(&admin).await).is_empty());
    assert_eq!(
        admin.get_candidate("cnd_missing").await["error"]["code"],
        json!("NOT_FOUND"),
        "no candidate exists to read"
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

/// C-027: receipts never leak the content of a record that was later deleted.
async fn receipts_never_leak_deleted_content(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let created = admin
        .create_record(record_body("k-c027-create", "nexus.world_building", TEXT, &shared_scope()))
        .await;
    let record_id = result(&created)["record_id"].as_str().expect("record id").to_owned();
    let deleted = admin
        .delete_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "reason": "superseded by C-027",
            "idempotency_key": "k-c027-delete",
        }))
        .await;
    assert_eq!(result(&deleted)["tombstoned"], json!(true));

    for receipt in [&created, &deleted] {
        let served = admin.get_receipt(support::receipt_id(receipt)).await;
        assert!(
            !served.to_string().contains("northern gate"),
            "a receipt never restates deleted content: {served}"
        );
        assert_eq!(served["receipt"]["status"], json!("committed"));
    }

    // The record itself is a content-free tombstone now.
    let tombstone = admin.get_record(&record_id).await;
    assert_eq!(tombstone["record"]["content"]["text"], json!(""));
    assert!(
        !tombstone.to_string().contains("northern gate"),
        "the tombstone carries no content: {tombstone}"
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
/// receipt and its key can still do.
async fn receipt_and_dedup_retention_windows(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let windows = receipt_windows(&matrix.admin().describe().await);
    let body = capture_body("k-c028", TEXT, &shared_scope());
    let first = matrix.admin().capture(body.clone()).await;
    let first_id = support::receipt_id(&first).to_owned();

    // Inside the window the receipt is served as stored.
    let live = matrix.admin().get_receipt(&first_id).await;
    assert_eq!(live["receipt"]["status"], json!("committed"));

    // Past the body window: the body is gone, the key stays bound.
    matrix.expire_receipts(false, windows).await;
    let expired = matrix.admin().get_receipt(&first_id).await;
    assert_eq!(error_code(&expired), "RECEIPT_EXPIRED", "{expired}");
    assert_eq!(retry_class(&expired), "terminal");
    let replay = matrix.admin().capture(body.clone()).await;
    assert_eq!(
        error_code(&replay),
        "RECEIPT_EXPIRED",
        "a key inside its dedup window never re-executes: {replay}"
    );
    assert_eq!(ids(&pending(&matrix.admin()).await).len(), 1);

    // Past the dedup window: the receipt is absent and the key is free again.
    matrix.expire_receipts(true, windows).await;
    let absent = matrix.admin().get_receipt(&first_id).await;
    assert_eq!(error_code(&absent), "NOT_FOUND", "{absent}");
    let fresh = matrix.admin().capture(body).await;
    assert!(is_committed(&fresh), "the freed key admits a new effect: {fresh}");
    assert_ne!(support::receipt_id(&fresh), first_id);
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
    let first_id = support::receipt_id(&first).to_owned();
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
        support::receipt_id(&replay),
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

/// C-030: candidate pages are bounded by the requested limit and carry only
/// candidates the caller covers.
async fn candidate_pages_are_bounded_and_scoped(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    for (key, scope) in [
        ("k-c030-a", context_scope(CONTEXT)),
        ("k-c030-b", context_scope(OTHER_CONTEXT)),
        ("k-c030-c", shared_scope()),
    ] {
        assert!(is_committed(&admin.capture(capture_body(key, TEXT, &scope)).await));
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
    assert_eq!(seen.len(), 3, "every candidate is paged exactly once: {seen:?}");

    // A reviewer restricted to one context sees only the candidate it covers.
    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = admin
        .grant(context_grant_body(
            "review",
            &peer_id,
            SUBJECT,
            &[CONTEXT],
            "k-c030-review",
        ))
        .await;
    assert!(is_committed(&granted), "{granted}");
    let scoped = ids(&pending(&matrix.peer()).await);
    assert_eq!(scoped.len(), 1, "only the covered candidate is listed: {scoped:?}");
    let hidden = matrix.peer().get_candidate("cnd_not_covered").await;
    assert_eq!(error_code(&hidden), "NOT_FOUND", "{hidden}");
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

/// C-031: review consumption and record creation are one atomic outcome, and a
/// replay returns that same outcome.
async fn review_consumes_and_creates_atomically(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let captured = admin
        .capture(capture_body("k-c031", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();

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
        candidate["consumed_by"]["receipt_id"],
        json!(support::receipt_id(&reviewed)),
        "the same receipt carries the consumption"
    );
    let record = admin.get_record(&records[0]).await;
    assert_eq!(record["record"]["content"]["text"], json!(TEXT));
    assert_eq!(record["record"]["scope"], shared_scope());

    let replay = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c031-review"))
        .await;
    assert_eq!(support::receipt_id(&replay), support::receipt_id(&reviewed));
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
    assert_eq!(support::receipt_id(&replay), support::receipt_id(&dismissed));
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

/// C-033: two concurrent reviews of one candidate have exactly one effect.
async fn concurrent_review_has_exactly_one_effect(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let peer_id = matrix.peer_principal().principal_id.clone();
    for (index, capability) in ["review", "records_write"].into_iter().enumerate() {
        let granted = matrix
            .admin()
            .grant(grant_body(capability, &peer_id, &format!("k-c033-{index}")))
            .await;
        assert!(is_committed(&granted), "{granted}");
    }
    let captured = matrix
        .admin()
        .capture(capture_body("k-c033", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();

    let admin_review = tokio::spawn(review_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        matrix.space_id().to_owned(),
        review_body(&candidate_id, 1, "k-c033-admin"),
    ));
    let peer_review = tokio::spawn(review_with(
        matrix.admin_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        review_body(&candidate_id, 1, "k-c033-peer"),
    ));
    let first = admin_review.await.expect("the first review completes");
    let second = peer_review.await.expect("the second review completes");

    let winners = [&first, &second].into_iter().filter(|body| is_committed(body)).count();
    assert_eq!(winners, 1, "exactly one review commits: {first} / {second}");
    let loser = if is_committed(&first) { &second } else { &first };
    let loser_code = error_code(loser);
    assert!(
        loser_code == "CANDIDATE_CONSUMED" || loser_code == "REVISION_CONFLICT",
        "the losing review is the terminal consumption/CAS refusal: {loser}"
    );
    let winner = if is_committed(&first) { &first } else { &second };
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
/// review and records_write coverage over it.
async fn review_inherits_scope_and_requires_dual_promote_grant(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let captured = admin
        .capture(capture_body("k-c034", TEXT, &context_scope(CONTEXT)))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();
    let peer_id = matrix.peer_principal().principal_id.clone();

    let review_only = admin
        .grant(context_grant_body(
            "review",
            &peer_id,
            SUBJECT,
            &[CONTEXT],
            "k-c034-review",
        ))
        .await;
    assert!(is_committed(&review_only), "{review_only}");
    let denied = matrix
        .peer()
        .review_candidate(review_body(&candidate_id, 1, "k-c034-denied"))
        .await;
    // The refusal is protocol-visible either way; providers differ in whether
    // the missing `records_write` coverage is reported as NOT_AUTHORIZED or
    // hidden as an absent candidate (both terminal, both zero-effect).
    assert!(!is_committed(&denied), "review alone cannot promote: {denied}");
    assert!(
        matches!(error_code(&denied), "NOT_AUTHORIZED" | "NOT_FOUND"),
        "the refusal stays in the classified refusal family: {denied}"
    );
    assert_eq!(retry_class(&denied), "terminal");
    assert_eq!(
        admin.get_candidate(&candidate_id).await["candidate"]["status"],
        json!("pending"),
        "the refused review consumed nothing"
    );
    assert_eq!(
        recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(0)
    );

    // §7.15 needs both capabilities over the same capture scope. The grant is
    // issued as ONE grant carrying both, which is the portable form: the local
    // provider also accepts two separate grants, the independent fixture
    // requires a single covering grant, and this form is admitted by both.
    let write = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["review", "records_write"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c034-dual",
        }))
        .await;
    assert!(is_committed(&write), "{write}");
    let promoted = matrix
        .peer()
        .review_candidate(review_body(&candidate_id, 1, "k-c034-promote"))
        .await;
    assert_eq!(result(&promoted)["outcome"], json!("promoted"), "{promoted}");
    let record = admin.get_record(&record_ids(&promoted)[0]).await;
    assert_eq!(
        record["record"]["scope"],
        context_scope(CONTEXT),
        "the record inherits the candidate's capture scope: {record}"
    );
    matrix.shutdown().await;
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

/// C-036: a same-key replay answers the stored receipt without re-evaluating
/// the current fence or CAS state; a fresh decision still observes both.
async fn replay_does_not_recheck_current_fence_or_cas(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let captured = admin
        .capture(capture_body("k-c036", TEXT, &shared_scope()))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();
    let request = review_body(&candidate_id, 1, "k-c036-review");
    let committed = admin.review_candidate(request.clone()).await;
    assert!(is_committed(&committed), "{committed}");

    // The subject is fenced after the decision was committed.
    admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "fence", 1, "k-c036-fence")
        .await;

    let replay = admin.review_candidate(request).await;
    assert_eq!(
        support::receipt_id(&replay),
        support::receipt_id(&committed),
        "the replay is the stored outcome, not a fresh decision: {replay}"
    );
    assert_eq!(record_ids(&replay), record_ids(&committed));

    let fresh = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c036-fresh"))
        .await;
    assert!(
        !is_committed(&fresh),
        "a fresh decision on the fenced subject is refused: {fresh}"
    );
    assert_eq!(
        recalled(&admin, json!({ "mode": "subject_all" })).await["total_authorized"],
        json!(1),
        "the replay created no second record"
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

/// C-116: a review of a binding-local candidate can never widen its visibility.
async fn binding_local_review_cannot_widen_visibility(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let captured = admin
        .capture(capture_body("k-c116", TEXT, &context_scope(CONTEXT)))
        .await;
    let candidate_id = candidate_ids(&captured)[0].clone();

    let promoted = admin
        .review_candidate(review_body(&candidate_id, 1, "k-c116-review"))
        .await;
    assert!(is_committed(&promoted), "{promoted}");
    let record = admin.get_record(&record_ids(&promoted)[0]).await;
    assert_eq!(
        record["record"]["scope"],
        context_scope(CONTEXT),
        "the promoted record stays in its binding-local placement: {record}"
    );

    // A shared-only read never sees it, while its own context still does.
    let shared_only = recalled(&admin, json!({ "mode": "shared_only" })).await;
    assert_eq!(shared_only["total_authorized"], json!(0), "{shared_only}");
    let own_context = recalled(
        &admin,
        json!({ "mode": "selected_contexts", "context_ids": [CONTEXT], "include_shared": false }),
    )
    .await;
    assert_eq!(own_context["total_authorized"], json!(1), "{own_context}");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_116_binding_local_review_cannot_widen_visibility_local() {
    binding_local_review_cannot_widen_visibility(Matrix::local().await).await;
}

#[tokio::test]
async fn c_116_binding_local_review_cannot_widen_visibility_remote() {
    binding_local_review_cannot_widen_visibility(Matrix::remote().await).await;
}

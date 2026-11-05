//! C-matrix family K (N3): supported error envelopes and retry classes,
//! unknown-op versus missing-capability, existence-oracle protection, effect
//! honesty around COMMIT, receipt authorization rechecks, and the truthful
//! refusal of the unavailable reflection family.
//!
//! The contract under test is the protocol's own: MEM-R016 (every error carries
//! an explicit retry class), MEM-R019 (no fallback), MEM-R003 (authorization
//! derives from authenticated context), MEM-R018/R021/R022 (receipt disclosure
//! rechecks current authorization; receipts stay content-free) and §14.2/§14.3
//! (`uncertain` is resolved only through same-key replay or `receipts.get`).
//!
//! Each scenario body takes a [`Matrix`] so the `_local` leg runs the real
//! `nexus_core` `LocalMemoryProvider` and the `_remote` leg runs the
//! independent durable fixture provider behind the published `spoke-connect`
//! memory service.
//!
//! C-100's `retryable` limb uses each provider's own operator seam: the fixture
//! exposes a bounded pre-effect admission hold whose loss is a no-effect
//! `INTERNAL_ERROR`; the local provider's pre-effect `INTERNAL_ERROR` is the
//! journal-predecessor refusal, which has no wire-reachable trigger, so the
//! `_local` leg exercises it through the real provider instance directly (the
//! same public `MemoryPort`). Its `terminal` and `uncertain` limbs run over the
//! signed wire on both providers. Capability-specific classes of the reflection
//! and transfer families remain N4-R/N4-T and are not claimed here.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use nexus_core::memory_provider::LocalMemoryProvider;
use spoke_operations::MemoryPort;

use support::{
    bootstrap, capture_body, error_code, error_details, is_committed, principal, receipt_id,
    result, retry_class, shared_scope, Kind, Matrix, SUBJECT, TEXT,
};

fn dto<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid request")
}

fn candidates(body: &Value) -> Vec<String> {
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

async fn pending_candidates(actor: &support::Actor<'_>) -> Vec<String> {
    candidates(
        &actor
            .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
            .await,
    )
}

// ---------------------------------------------------------------------------
// K. Error envelope and retry classes
// ---------------------------------------------------------------------------

/// C-100 (N3 base): a supported terminal refusal, an uncertain commit and a
/// no-effect retryable failure each carry `details.retry_class` that matches
/// their actual meaning. The uncertain outcome is resolved only through
/// same-key replay or `receipts.get` and never by a hidden third channel.
///
/// The reflection/transfer capability-specific classes remain N4-R/N4-T.
async fn supported_error_envelopes_and_retry_classes(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();

    // 1. Terminal: a principal with no capture grant is refused, and retrying
    //    unchanged cannot help.
    let denied = matrix
        .peer()
        .capture(capture_body("k-c100-denied", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");

    // Grant capture so the commit paths below have a real admitted effect.
    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture"],
            "idempotency_key": "k-c100-grant",
        }))
        .await;
    assert!(is_committed(&granted), "{granted}");

    // 2. Uncertain: the effect commits and the response is lost. Resolution is
    //    the same-key replay, which returns the original receipt; polling
    //    `receipts.get` serves the same committed receipt.
    matrix.arm_lost_response();
    let lost = matrix
        .peer()
        .capture(capture_body("k-c100-lost", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN", "{lost}");
    assert_eq!(retry_class(&lost), "uncertain");
    assert!(
        lost["error"]["details"].get("retry_class").is_some(),
        "the class is in details, not fabricated from the message: {lost}"
    );

    let replayed = matrix
        .peer()
        .capture(capture_body("k-c100-lost", TEXT, &shared_scope()))
        .await;
    assert!(is_committed(&replayed), "same-key replay resolves: {replayed}");
    let effect_ids = candidates(&admin.list_candidates(
        json!({ "subject_id": SUBJECT, "status": "pending" }),
    )
    .await);
    assert_eq!(effect_ids.len(), 1, "the lost response applied exactly once");
    assert_eq!(support::candidate_ids(&replayed), effect_ids);

    let stored = admin.get_receipt(receipt_id(&replayed)).await;
    assert_eq!(stored["receipt"]["status"], json!("committed"), "{stored}");
    assert!(
        stored["receipt"]["result"].get("content").is_none(),
        "the receipt is content-free: {stored}"
    );

    // 3. Retryable: a real no-effect failure before the effect commits.
    //    Fixture: the bounded admission hold is lost before any staging, which
    //    the provider answers as a retryable INTERNAL_ERROR. Local: the same
    //    class is raised by the journal-predecessor refusal, exercised below on
    //    the real provider instance.
    if matrix.kind() == Kind::Remote {
        let mut hold = matrix.park_next_admission();
        let capture = tokio::spawn(support::capture_with(
            matrix.peer_client(),
            matrix.peer_principal().clone(),
            matrix.space_id().to_owned(),
            capture_body("k-c100-retry", TEXT, &shared_scope()),
        ));
        hold.wait_reached()
            .await
            .expect("the capture reached the bounded admission point");
        // The control is lost without a release: the provider's own bounded
        // wait ends with the no-effect refusal.
        drop(hold);
        let failed = capture.await.expect("the capture completes");
        assert_eq!(error_code(&failed), "INTERNAL_ERROR", "{failed}");
        assert_eq!(retry_class(&failed), "retryable");
        assert_eq!(
            candidates(&admin.list_candidates(
                json!({ "subject_id": SUBJECT, "status": "pending" }),
            )
            .await)
            .len(),
            1,
            "the no-effect failure persisted nothing"
        );
    } else {
        local_retryable_predecessor().await;
    }

    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0)
    );
    matrix.shutdown().await;
}

/// C-100 (`_local` limb): the local provider's pre-effect `INTERNAL_ERROR` is
/// the journal-predecessor refusal. It is raised before the later write's own
/// effect commits, is typed `retryable`, names the unsettled predecessor and
/// publishes nothing.
///
/// The unsafe predecessor is produced through the provider's own commit-boundary
/// seam (the effect commits, the response is lost) and an unresolvable
/// projection target, exactly the state the protocol defines as retryable.
async fn local_retryable_predecessor() {
    let home = tempfile::TempDir::new().expect("temporary local home");
    let admin_id = "ctr_c100_admin";
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), admin_id, "default")
        .await
        .expect("open the real local provider");
    let admin = principal(admin_id);
    let space = provider.space_id().to_owned();

    let ensured = support::wire(
        &provider
            .ensure_subject(&admin, dto(json!({ "space_id": space, "subject_id": SUBJECT })))
            .await,
    );
    assert!(is_committed(&ensured), "{ensured}");

    let created = support::wire(
        &provider
            .create_record(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": SUBJECT,
                    "kind": "nexus.world_building",
                    "content": { "text": TEXT },
                    "scope": { "visibility": "shared" },
                    "idempotency_key": "k-c100-create",
                })),
            )
            .await,
    );
    let record_id = result(&created)["record_id"]
        .as_str()
        .expect("record id")
        .to_owned();

    // The update commits but its response is lost: the journal intent stays
    // unsettled, which is the no-effect refusal a later write on the same target
    // must report instead of publishing over it.
    provider.inject_commit_fault(true);
    let lost_update = support::wire(
        &provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": space,
                    "record_id": record_id,
                    "expected_revision": 1,
                    "content": { "text": "content the caller never saw" },
                    "idempotency_key": "k-c100-update",
                })),
            )
            .await,
    );
    assert_eq!(error_code(&lost_update), "OUTCOME_UNCERTAIN", "{lost_update}");
    assert_eq!(retry_class(&lost_update), "uncertain");

    // Make that intent unresolvable: the projection target holds bytes the
    // intent never produced.
    let target = provider
        .file_root()
        .join(format!("records/{record_id}.md"));
    std::fs::create_dir_all(target.parent().expect("record parent")).expect("record dir");
    let foreign = "foreign bytes this intent never produced";
    std::fs::write(&target, foreign).expect("foreign target bytes");

    let refused = support::wire(
        &provider
            .delete_record(
                &admin,
                dto(json!({
                    "space_id": space,
                    "record_id": record_id,
                    "expected_revision": 2,
                    "idempotency_key": "k-c100-delete",
                })),
            )
            .await,
    );
    assert_eq!(error_code(&refused), "INTERNAL_ERROR", "{refused}");
    assert_eq!(retry_class(&refused), "retryable");
    assert!(
        error_details(&refused)["receipt_id"].as_str().is_some(),
        "the refusal names its unsettled predecessor: {refused}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("target readable"),
        foreign,
        "the refused deletion published nothing"
    );
}

/// C-101: an unknown wire op is `op_unsupported`; a known op on a session that
/// never negotiated `spoke-memory` is `capability_missing` at the dispatch
/// boundary. A valid op on the same boundary succeeds, and neither refusal
/// creates state (no local fallback, no forged reject).
async fn unknown_op_and_missing_capability_are_distinct(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();

    let mut with_memory = matrix.raw_admin().await;
    let unknown = with_memory.invoke("memory.not_an_op", &json!({})).await;
    assert_eq!(
        unknown["error"]["code"],
        json!("op_unsupported"),
        "an unknown op is not silently served: {unknown}"
    );
    // The unknown-op branch carries no provider details: it never ran a
    // handler that could classify one.
    assert!(
        unknown["error"]["details"].is_null(),
        "the dispatch deny invents no class: {unknown}"
    );

    // The positive control on the same session and boundary: a real memory op
    // succeeds.
    let described = with_memory.invoke("memory.describe", &json!({})).await;
    assert!(
        described.get("error").is_none(),
        "the same session serves a real op: {described}"
    );
    assert_eq!(
        described["payload"]["capabilities"],
        json!(["spoke-memory"]),
        "{described}"
    );
    with_memory.close().await;

    let mut without_memory = matrix.raw_admin_without_memory_capability().await;
    let missing = without_memory
        .invoke(
            "memory.capture",
            &json!({ "space_id": matrix.space_id(), "idempotency_key": "k-c101" }),
        )
        .await;
    assert_eq!(
        missing["error"]["code"],
        json!("capability_missing"),
        "a known op without the negotiated family is a capability refusal: {missing}"
    );
    assert_eq!(missing["error"]["details"]["retry_class"], json!("terminal"));
    assert_ne!(
        missing["error"]["code"], unknown["error"]["code"],
        "unknown op and missing capability are distinct"
    );
    without_memory.close().await;

    assert!(
        pending_candidates(&admin).await.is_empty(),
        "neither refusal created a candidate"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "neither refusal created a receipt"
    );
    matrix.shutdown().await;
}

/// C-102: a foreign id and a missing id answer the same permitted refusal; no
/// content or ownership leaks, and authorization is rechecked before existence.
async fn foreign_and_missing_ids_have_no_existence_oracle(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let ensured = admin.ensure_subject(support::OTHER_SUBJECT).await;
    assert_eq!(result(&ensured)["created"], json!(true), "{ensured}");

    let foreign = admin
        .create_record(json!({
            "subject_id": support::OTHER_SUBJECT,
            "kind": "nexus.world_building",
            "content": { "text": "a secret from another subject" },
            "scope": shared_scope(),
            "idempotency_key": "k-c102-foreign",
        }))
        .await;
    let foreign_id = result(&foreign)["record_id"]
        .as_str()
        .expect("record id")
        .to_owned();
    let covered = admin
        .create_record(json!({
            "subject_id": SUBJECT,
            "kind": "nexus.world_building",
            "content": { "text": TEXT },
            "scope": shared_scope(),
            "idempotency_key": "k-c102-covered",
        }))
        .await;
    let covered_id = result(&covered)["record_id"]
        .as_str()
        .expect("record id")
        .to_owned();

    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["records_read"],
            "idempotency_key": "k-c102-grant",
        }))
        .await;
    let grant_id = result(&granted)["grant_id"]
        .as_str()
        .expect("grant id")
        .to_owned();
    let peer = matrix.peer();

    // Positive control: the covered record is readable before the recheck.
    assert_eq!(
        peer.get_record(&covered_id).await["record"]["record_id"],
        json!(covered_id)
    );

    let foreign_probe = peer.get_record(&foreign_id).await;
    let missing_probe = peer.get_record("rec_c102_missing").await;
    assert_eq!(error_code(&foreign_probe), "NOT_FOUND", "{foreign_probe}");
    assert_eq!(error_code(&missing_probe), "NOT_FOUND", "{missing_probe}");
    assert_eq!(
        error_details(&foreign_probe),
        error_details(&missing_probe),
        "foreign and missing ids share one refusal shape"
    );
    let leaked = format!("{foreign_probe}{missing_probe}");
    assert!(
        !leaked.contains("a secret from another subject"),
        "no foreign content leaks: {leaked}"
    );
    assert!(
        !leaked.contains(support::OTHER_SUBJECT),
        "no foreign ownership leaks: {leaked}"
    );
    assert_eq!(retry_class(&foreign_probe), "terminal");

    // Subject level: a subject the caller holds no grant on and a subject that
    // does not exist answer one identical refusal, so neither existence nor
    // ownership can be probed.
    let foreign_subject = peer.get_subject(support::OTHER_SUBJECT).await;
    let missing_subject = peer.get_subject("sbj_c102_missing").await;
    assert_eq!(
        error_code(&foreign_subject),
        error_code(&missing_subject),
        "a foreign subject and a missing one share one refusal: {foreign_subject} {missing_subject}"
    );
    assert!(
        matches!(error_code(&foreign_subject), "NOT_FOUND" | "NOT_AUTHORIZED"),
        "{foreign_subject}"
    );
    assert!(
        error_details(&foreign_subject) == error_details(&missing_subject),
        "the subject refusal carries no differing detail: {foreign_subject} {missing_subject}"
    );
    let subject_leak = format!("{foreign_subject}{missing_subject}");
    assert!(
        !subject_leak.contains("a secret from another subject"),
        "no foreign content leaks through the subject surface"
    );

    // Authorization is rechecked before existence: revoking the grant turns the
    // covered read into the same absence, never a success or a different shape.
    let revoked = admin.revoke_grant(&grant_id, 1, "k-c102-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");
    let after_revoke = peer.get_record(&covered_id).await;
    assert_eq!(error_code(&after_revoke), "NOT_FOUND", "{after_revoke}");
    assert_eq!(retry_class(&after_revoke), "terminal");
    assert_eq!(
        error_details(&after_revoke),
        error_details(&missing_probe),
        "a revoked reader gets the same absence as a missing id"
    );
    matrix.shutdown().await;
}

/// C-103: a failure before COMMIT and a loss after COMMIT are reported honestly.
/// The pre-commit case is never claimed as a clean success, and the post-commit
/// case never as a clean retryable: it is `uncertain`, the durable effect exists
/// exactly once, and the content-free receipt resolves the outcome.
async fn internal_error_reports_effect_honesty(matrix: Matrix) {
    bootstrap(&matrix, &["capture"]).await;
    let admin = matrix.admin();

    // 1. Failure before COMMIT: nothing durable, and the provider does not
    //    claim a completed effect.
    matrix.arm_commit_boundary_failure();
    let before = matrix
        .peer()
        .capture(capture_body("k-c103-before", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&before), "OUTCOME_UNCERTAIN", "{before}");
    assert_eq!(retry_class(&before), "uncertain");
    assert!(
        pending_candidates(&admin).await.is_empty(),
        "a failure before COMMIT committed no candidate"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "a failure before COMMIT left no receipt"
    );

    // 2. Loss after COMMIT: the effect is durable, the caller learns an unknown
    //    outcome, and the same-key replay resolves it without a second effect.
    matrix.arm_lost_response();
    let after = matrix
        .peer()
        .capture(capture_body("k-c103-after", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&after), "OUTCOME_UNCERTAIN", "{after}");
    assert_eq!(retry_class(&after), "uncertain");
    let durable = pending_candidates(&admin).await;
    assert_eq!(durable.len(), 1, "the post-COMMIT effect is durable exactly once");

    let replayed = matrix
        .peer()
        .capture(capture_body("k-c103-after", TEXT, &shared_scope()))
        .await;
    assert!(is_committed(&replayed), "{replayed}");
    assert_eq!(support::candidate_ids(&replayed), durable);
    assert_eq!(
        pending_candidates(&admin).await.len(),
        1,
        "replay did not double-apply"
    );
    let stored = admin.get_receipt(receipt_id(&replayed)).await;
    assert_eq!(stored["receipt"]["status"], json!("committed"), "{stored}");
    assert!(
        stored["receipt"]["result"].get("content").is_none(),
        "the durable receipt is content-free: {stored}"
    );
    matrix.shutdown().await;
}

/// C-104: a receipt read rechecks the caller's current authorization. The
/// original requester reads its receipt, then loses coverage; the same read is
/// refused with no content disclosure and no forbidden effect.
async fn receipt_read_rechecks_revoked_authorization(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture"],
            "idempotency_key": "k-c104-grant",
        }))
        .await;
    let grant_id = result(&granted)["grant_id"]
        .as_str()
        .expect("grant id")
        .to_owned();

    let captured = matrix
        .peer()
        .capture(capture_body("k-c104", TEXT, &shared_scope()))
        .await;
    let receipt = receipt_id(&captured).to_owned();
    let effects_before = pending_candidates(&admin).await.len();

    // The original requester reads the same receipt while authorized.
    let first = matrix.peer().get_receipt(&receipt).await;
    assert_eq!(first["receipt"]["receipt_id"], json!(receipt), "{first}");

    let revoked = admin.revoke_grant(&grant_id, 1, "k-c104-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");

    // The revoked reader is refused; no receipt body is disclosed.
    let second = matrix.peer().get_receipt(&receipt).await;
    assert!(
        matches!(
            error_code(&second),
            "NOT_AUTHORIZED" | "GRANT_REVOKED"
        ),
        "a revoked requester cannot read the receipt: {second}"
    );
    assert_eq!(retry_class(&second), "terminal");
    assert!(
        second["error"].get("receipt").is_none(),
        "no receipt content is disclosed: {second}"
    );

    // The refusal is about current authority, not retention: an authorized
    // principal still reads the same receipt.
    assert_eq!(
        admin.get_receipt(&receipt).await["receipt"]["receipt_id"],
        json!(receipt)
    );
    assert_eq!(
        pending_candidates(&admin).await.len(),
        effects_before,
        "the refused read changed no candidate"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0)
    );
    matrix.shutdown().await;
}

/// C-073 (N3 refusal): the unavailable reflection family's state/list reads are
/// refused truthfully with zero effect and no generation/receipt mutation.
/// Successful state/list/sufficiency behavior is N4-R and is not claimed.
async fn reflection_reads_refuse_without_capability(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let baseline = admin.list_spaces().await;

    let mut raw = matrix.raw_admin().await;
    for op in ["memory.reflections.state", "memory.reflections.list"] {
        let response = raw
            .invoke(
                op,
                &json!({
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                }),
            )
            .await;
        assert_eq!(
            response["error"]["code"],
            json!("capability_missing"),
            "{op} is not served by an N3 provider: {response}"
        );
        assert_eq!(response["error"]["details"]["retry_class"], json!("terminal"));
        assert!(
            response.get("payload").is_none(),
            "{op} never fabricates a reflection payload: {response}"
        );
    }
    raw.close().await;

    let after = admin.list_spaces().await;
    assert_eq!(
        after["spaces"][0]["pending_receipts"],
        json!(0),
        "the refused reads created no generation receipt"
    );
    assert_eq!(
        after["spaces"][0]["revision"], baseline["spaces"][0]["revision"],
        "the refused reads did not mutate the space"
    );
    assert_eq!(
        after, baseline,
        "the refused reads left the space projection unchanged"
    );
    assert!(
        pending_candidates(&admin).await.is_empty(),
        "the refused reads created no candidate"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_100_supported_error_envelopes_and_retry_classes_local() {
    supported_error_envelopes_and_retry_classes(Matrix::local().await).await;
}

#[tokio::test]
async fn c_100_supported_error_envelopes_and_retry_classes_remote() {
    supported_error_envelopes_and_retry_classes(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_101_unknown_op_and_missing_capability_are_distinct_local() {
    unknown_op_and_missing_capability_are_distinct(Matrix::local().await).await;
}

#[tokio::test]
async fn c_101_unknown_op_and_missing_capability_are_distinct_remote() {
    unknown_op_and_missing_capability_are_distinct(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_102_foreign_and_missing_ids_have_no_existence_oracle_local() {
    foreign_and_missing_ids_have_no_existence_oracle(Matrix::local().await).await;
}

#[tokio::test]
async fn c_102_foreign_and_missing_ids_have_no_existence_oracle_remote() {
    foreign_and_missing_ids_have_no_existence_oracle(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_103_internal_error_reports_effect_honesty_local() {
    internal_error_reports_effect_honesty(Matrix::local().await).await;
}

#[tokio::test]
async fn c_103_internal_error_reports_effect_honesty_remote() {
    internal_error_reports_effect_honesty(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_104_receipt_read_rechecks_revoked_authorization_local() {
    receipt_read_rechecks_revoked_authorization(Matrix::local().await).await;
}

#[tokio::test]
async fn c_104_receipt_read_rechecks_revoked_authorization_remote() {
    receipt_read_rechecks_revoked_authorization(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_073_reflection_reads_refuse_without_capability_local() {
    reflection_reads_refuse_without_capability(Matrix::local().await).await;
}

#[tokio::test]
async fn c_073_reflection_reads_refuse_without_capability_remote() {
    reflection_reads_refuse_without_capability(Matrix::remote().await).await;
}

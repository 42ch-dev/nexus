//! C-matrix families A and B (N3): capability admission/discovery and
//! space/subject lifecycle fencing, on both real providers.
//!
//! Every scenario body is provider-agnostic and runs against the real
//! `nexus_core` local provider and the independent durable fixture provider.
//! The `_local` / `_remote` suffix follows matrix §3.
//!
//! Split-row accounting (§5):
//! - C-011 is `N3 + N4`: this file proves the base-operation freeze allowlist
//!   only. The N4-T export/import allowlist success remainder stays open and is
//!   never asserted here.
//! - C-015 is N4-only and has no scenario in this file at all.
//!
//! Wire-only observations (an op with no dispatch row, an unnegotiated family, a
//! session that never negotiated the memory capability) go through a real
//! signed raw session, because the typed client cannot express those shapes.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde_json::{json, Value};

use support::{
    bootstrap, capture_body, context_scope, error_code, error_details, grant_body, is_committed,
    receipt_id, result, retry_class, shared_scope, Actor, Kind, Matrix, CONTEXT, OTHER_CONTEXT,
    OTHER_SUBJECT, SUBJECT, TEXT,
};

/// A bounded observation that a request has reached the contested boundary and
/// has not completed yet. It is not a sleep used to order the race: the hold is
/// released only after this fails to complete.
async fn still_pending<F: Future>(future: &mut Pin<Box<F>>, what: &str) {
    match tokio::time::timeout(Duration::from_millis(400), future).await {
        Err(_) => {}
        Ok(_) => panic!("{what} completed before the contested boundary was released"),
    }
}

async fn candidates(actor: &Actor<'_>) -> Value {
    actor
        .list_candidates(json!({ "subject_id": SUBJECT }))
        .await
}

fn candidate_ids(body: &Value) -> Vec<String> {
    body["candidates"]
        .as_array()
        .unwrap_or_else(|| panic!("a candidate page names its rows: {body}"))
        .iter()
        .map(|candidate| candidate["candidate_id"].as_str().expect("candidate id").to_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// A. Capability admission and discovery
// ---------------------------------------------------------------------------

/// C-001: the provider describes exactly the surface it serves.
async fn describe_contract(matrix: Matrix) {
    let body = matrix.admin().describe().await;
    assert_eq!(body["memory_protocol_version"], json!(1));
    assert_eq!(
        body["capabilities"],
        json!(["spoke-memory"]),
        "only the served family is advertised: {body}"
    );
    assert!(
        body["engine_policy_version"]
            .as_str()
            .is_some_and(|version| !version.is_empty()),
        "the provider stamps its own policy version: {body}"
    );
    for limit in [
        "max_page_size",
        "max_content_bytes",
        "max_candidates_per_capture",
        "export_chunk_bytes",
        "receipt_retention_seconds",
        "dedup_retention_seconds",
        "idempotency_retention_seconds",
    ] {
        assert!(
            body["limits"][limit].as_u64().is_some_and(|value| value >= 1),
            "describe advertises a positive {limit}: {body}"
        );
    }
    assert!(
        body["limits"]["dedup_retention_seconds"]
            .as_u64()
            .unwrap_or(0)
            >= body["limits"]["receipt_retention_seconds"]
                .as_u64()
                .unwrap_or(u64::MAX),
        "the dedup window covers the receipt body window: {body}"
    );
    assert!(
        body["required_profile_kinds"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().all(Value::is_string)),
        "the required authored kinds are an honest list: {body}"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_001_describe_contract_local() {
    describe_contract(Matrix::local().await).await;
}

#[tokio::test]
async fn c_001_describe_contract_remote() {
    describe_contract(Matrix::remote().await).await;
}

/// C-002: known operations fail closed and an op with no dispatch row is
/// refused; a session that never negotiated the capability is denied before any
/// handler runs; neither refusal reaches an effect.
async fn known_and_unknown_ops_fail_closed(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer = matrix.peer();

    // A known op whose grant is absent refuses; it never answers a success.
    let denied = peer.capture(capture_body("k-c002-denied", TEXT, &shared_scope())).await;
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");

    // A known op with a stale revision refuses too.
    let stale = admin.freeze_space("k-c002-stale", 9).await;
    assert_eq!(error_code(&stale), "REVISION_CONFLICT", "{stale}");

    // A signed session whose hello never advertised `spoke-memory`: the Connect
    // dispatch gate denies known memory ops with `capability_missing` before any
    // handler or payload decode runs (protocol §13).
    let mut capabilityless = matrix.raw_admin_without_memory_capability().await;
    for op in ["memory.describe", "memory.capture", "memory.recall"] {
        let response = capabilityless
            .invoke(op, &json!({ "space_id": matrix.space_id() }))
            .await;
        let error = response.get("error").expect("dispatch-deny envelope");
        assert_eq!(
            error["code"], json!("capability_missing"),
            "{op} on a capability-less session: {response}"
        );
        assert_eq!(error["details"]["retry_class"], json!("terminal"), "{op}");
    }
    capabilityless.close().await;

    // An op name with no dispatch row is not silently served.
    let mut raw = matrix.raw_admin().await;
    let unknown = raw.invoke("memory.not_an_op", &json!({})).await;
    assert_eq!(
        unknown["error"]["code"],
        json!("op_unsupported"),
        "an unknown memory op is refused: {unknown}"
    );
    raw.close().await;

    // None of the refusals created work.
    assert!(
        candidate_ids(&candidates(&admin).await).is_empty(),
        "a refused capture or denied session leaves no candidate"
    );
    assert_eq!(
        admin
            .recall(json!({
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
                "budget": { "max_items": 10 },
            }))
            .await["total_authorized"],
        json!(0)
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["status"],
        json!("active"),
        "the stale freeze refusal changed nothing"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "no refusal created a receipt"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_002_known_and_unknown_ops_fail_closed_local() {
    known_and_unknown_ops_fail_closed(Matrix::local().await).await;
}

#[tokio::test]
async fn c_002_known_and_unknown_ops_fail_closed_remote() {
    known_and_unknown_ops_fail_closed(Matrix::remote().await).await;
}

/// C-003: reflection requires its sub-capability across state/list/generate; the
/// unnegotiated family is answered `capability_missing` at the dispatch gate.
async fn reflection_requires_subcapability(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let mut raw = matrix.raw_admin().await;
    for op in [
        "memory.reflections.state",
        "memory.reflections.list",
        "memory.reflections.generate",
    ] {
        let response = raw
            .invoke(
                op,
                &json!({
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                }),
            )
            .await;
        let error = response.get("error").expect("capability-deny envelope");
        assert_eq!(
            error["code"],
            json!("capability_missing"),
            "{op} refused by the dispatch gate: {response}"
        );
        assert_eq!(error["details"]["retry_class"], json!("terminal"), "{op}");
    }
    raw.close().await;
    let admin = matrix.admin();
    assert_eq!(
        admin
            .recall(json!({
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
                "budget": { "max_items": 10 },
            }))
            .await["total_authorized"],
        json!(0),
        "an unavailable reflection generation wrote nothing"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_003_reflection_requires_subcapability_local() {
    reflection_requires_subcapability(Matrix::local().await).await;
}

#[tokio::test]
async fn c_003_reflection_requires_subcapability_remote() {
    reflection_requires_subcapability(Matrix::remote().await).await;
}

/// C-004: transfer requires its sub-capability across every export/import
/// operation it names.
async fn transfer_requires_subcapability(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let mut raw = matrix.raw_admin().await;
    for op in [
        "memory.exports.create",
        "memory.exports.get",
        "memory.imports.stage",
        "memory.imports.activate",
        "memory.imports.discard",
    ] {
        let response = raw.invoke(op, &json!({})).await;
        let error = response.get("error").expect("capability-deny envelope");
        assert_eq!(
            error["code"],
            json!("capability_missing"),
            "{op} refused by the dispatch gate: {response}"
        );
        assert_eq!(error["details"]["retry_class"], json!("terminal"), "{op}");
    }
    raw.close().await;
    let admin = matrix.admin();
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "the refused transfer family created no receipt"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_004_transfer_requires_subcapability_local() {
    transfer_requires_subcapability(Matrix::local().await).await;
}

#[tokio::test]
async fn c_004_transfer_requires_subcapability_remote() {
    transfer_requires_subcapability(Matrix::remote().await).await;
}

/// C-005: `memory.describe` carries capability facts only, never a space
/// catalogue; the catalogue itself pages under the advertised bound.
async fn describe_has_no_space_catalogue(matrix: Matrix) {
    let body = matrix.admin().describe().await;
    assert!(
        body.get("spaces").is_none(),
        "describe is not a space catalogue: {body}"
    );
    assert_eq!(body["capabilities"], json!(["spoke-memory"]));

    // The catalogue is a bounded page: a page never exceeds the requested limit
    // and reports its end instead of growing without bound.
    let bounded = matrix.admin().list_spaces_page(1).await;
    let rows = bounded["spaces"].as_array().expect("space rows");
    assert!(
        rows.len() <= 1,
        "the page honours the advertised limit: {bounded}"
    );
    assert!(
        bounded["next_cursor"].is_null(),
        "the exhausted catalogue reports no next cursor: {bounded}"
    );
    assert_eq!(rows[0]["space_id"], json!(matrix.space_id()));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_005_describe_has_no_space_catalogue_local() {
    describe_has_no_space_catalogue(Matrix::local().await).await;
}

#[tokio::test]
async fn c_005_describe_has_no_space_catalogue_remote() {
    describe_has_no_space_catalogue(Matrix::remote().await).await;
}

// ---------------------------------------------------------------------------
// B. Space and subject lifecycle fencing
// ---------------------------------------------------------------------------

/// C-010: the space identity and its authority generation are stable across a
/// write and across a restart.
async fn space_identity_and_generation_are_stable(matrix: Matrix) {
    let identity = {
        let admin = matrix.admin();
        let before = admin.list_spaces().await;
        let space = &before["spaces"][0];
        let identity = json!({
            "space_id": space["space_id"].clone(),
            "authority_host_id": space["authority_host_id"].clone(),
            "authority_generation": space["authority_generation"].clone(),
            "revision": space["revision"].clone(),
        });
        assert_eq!(identity["space_id"], json!(matrix.space_id()));
        admin.ensure_subject(SUBJECT).await;
        identity
    };

    let after_write = matrix.admin().list_spaces().await;
    assert_eq!(
        after_write["spaces"][0]["authority_generation"], identity["authority_generation"],
        "a content write never advances the authority generation: {after_write}"
    );
    assert_eq!(after_write["spaces"][0]["space_id"], identity["space_id"]);

    let matrix = matrix.restart().await;
    let after_restart = matrix.admin().list_spaces().await;
    assert_eq!(
        after_restart["spaces"][0]["space_id"], identity["space_id"],
        "the restarted provider serves the same space"
    );
    assert_eq!(
        after_restart["spaces"][0]["authority_host_id"], identity["authority_host_id"],
        "the authority host survives the restart"
    );
    assert_eq!(
        after_restart["spaces"][0]["authority_generation"], identity["authority_generation"],
        "the authority generation survives the restart"
    );
    assert_eq!(
        after_restart["spaces"][0]["revision"], identity["revision"],
        "no speculative space revision is written by a write or restart: {after_restart}"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_010_space_identity_and_generation_are_stable_local() {
    space_identity_and_generation_are_stable(Matrix::local().await).await;
}

#[tokio::test]
async fn c_010_space_identity_and_generation_are_stable_remote() {
    space_identity_and_generation_are_stable(Matrix::remote().await).await;
}

/// C-011 (N3 + N4): the base-operation freeze allowlist. The N4-T
/// export/import allowlist success remainder is deliberately not asserted.
async fn freeze_allowlist_base_operations(matrix: Matrix) {
    bootstrap(&matrix, &["capture"]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();

    let issued = admin
        .grant(grant_body("recall", &peer_id, "k-c011-pre"))
        .await;
    let grant_id = result(&issued)["grant_id"].as_str().expect("grant id").to_owned();

    let frozen = admin.freeze_space("k-c011-freeze", 1).await;
    assert_eq!(result(&frozen)["status"], "frozen");
    assert_eq!(result(&frozen)["revision"], json!(2));

    // Outside the §8.2 allowlist: fresh writes refuse and commit nothing.
    let capture = admin
        .capture(capture_body("k-c011-capture", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&capture), "SPACE_FROZEN", "{capture}");
    let created = admin
        .create_record(
            json!({
                "subject_id": SUBJECT,
                "kind": "nexus.world_building",
                "content": { "text": TEXT },
                "scope": shared_scope(),
                "idempotency_key": "k-c011-record",
            }),
        )
        .await;
    assert_eq!(error_code(&created), "SPACE_FROZEN", "{created}");
    let granted = admin
        .grant(grant_body("capture", &peer_id, "k-c011-grant"))
        .await;
    assert_eq!(error_code(&granted), "SPACE_FROZEN", "{granted}");
    let ensured = admin.ensure_subject(OTHER_SUBJECT).await;
    assert_eq!(error_code(&ensured), "SPACE_FROZEN", "{ensured}");
    let profile = admin
        .update_profile(json!({
            "subject_id": SUBJECT,
            "profile_kind": "personality",
            "expected_revision": null,
            "document": { "format": "markdown", "body": "# Mira" },
            "idempotency_key": "k-c011-profile",
        }))
        .await;
    assert_eq!(error_code(&profile), "SPACE_FROZEN", "{profile}");
    assert!(
        candidate_ids(&candidates(&admin).await).is_empty(),
        "no frozen-space write left a candidate"
    );

    // Inside the allowlist: reads, revoke and unfreeze stay admissible.
    let spaces = admin.list_spaces().await;
    assert_eq!(spaces["spaces"][0]["status"], "frozen");
    let subject = admin.get_subject(SUBJECT).await;
    assert_eq!(subject["subject"]["subject_id"], json!(SUBJECT));
    let revoked = admin.revoke_grant(grant_id.as_str(), 1, "k-c011-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true));
    let receipt_view = admin.get_receipt(receipt_id(&frozen)).await;
    assert_eq!(receipt_view["receipt"]["receipt_id"], receipt_id(&frozen));

    let unfrozen = admin.unfreeze_space("k-c011-unfreeze", 2).await;
    assert_eq!(result(&unfrozen)["status"], "active");
    assert_eq!(result(&unfrozen)["revision"], json!(3));

    // After unfreeze exactly the fresh write commits — no ghost write appears
    // later from the frozen period.
    let after = admin
        .capture(capture_body("k-c011-after", TEXT, &shared_scope()))
        .await;
    assert!(is_committed(&after), "unfreezing restores the write: {after}");
    assert_eq!(
        candidate_ids(&candidates(&admin).await).len(),
        1,
        "exactly the post-unfreeze write left a candidate"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0)
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_011_freeze_allowlist_base_operations_local() {
    freeze_allowlist_base_operations(Matrix::local().await).await;
}

#[tokio::test]
async fn c_011_freeze_allowlist_base_operations_remote() {
    freeze_allowlist_base_operations(Matrix::remote().await).await;
}

/// C-012: a write racing the freeze transition has exactly one admission
/// outcome, established by the provider's own causal boundary.
///
/// - Local: the parked capture already holds the writer lock, so the freeze
///   request is observed still pending at the contested boundary; releasing the
///   capture commits it, and the freeze commits after it.
/// - Fixture: the capture is paused before its writer transaction, so the freeze
///   is observed committed *before* the capture is released; the released
///   capture must then be refused.
async fn write_racing_freeze_has_one_admission_outcome(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = matrix
        .admin()
        .grant(grant_body("admin", &peer_id, "k-c012-admin"))
        .await;
    assert!(is_committed(&granted), "{granted}");

    let space_id = matrix.space_id().to_owned();
    let mut hold = match matrix.kind() {
        Kind::Local => matrix.park_next_capture(),
        Kind::Remote => matrix.park_next_admission(),
    };
    let capture = tokio::spawn(support::capture_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        space_id.clone(),
        capture_body("k-c012-race", TEXT, &shared_scope()),
    ));
    hold.wait_reached()
        .await
        .expect("the capture reached its provider point");

    let mut freeze = Box::pin(support::freeze_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        space_id,
        "k-c012-freeze",
        1,
    ));

    let capture_outcome = match matrix.kind() {
        Kind::Local => {
            // The capture holds the writer lock; the freeze request reaches the
            // contested boundary and cannot pass it before the release.
            still_pending(&mut freeze, "the freeze request").await;
            hold.release().await;
            let outcome = capture.await.expect("the racing capture completes");
            assert!(
                is_committed(&outcome),
                "the transaction-held capture commits: {outcome}"
            );
            let frozen = freeze.await;
            assert_eq!(result(&frozen)["status"], json!("frozen"));
            outcome
        }
        Kind::Remote => {
            // The admission pause is before the writer transaction: the freeze
            // commits first, and that is observed before the capture is resumed.
            let frozen = freeze.await;
            assert_eq!(
                result(&frozen)["status"],
                json!("frozen"),
                "the freeze commits while the capture is paused at admission"
            );
            hold.release().await;
            let outcome = capture.await.expect("the paused capture completes");
            assert_eq!(
                error_code(&outcome),
                "SPACE_FROZEN",
                "the released admission observes the committed freeze: {outcome}"
            );
            outcome
        }
    };

    let admin = matrix.admin();
    let rows = candidate_ids(&candidates(&admin).await);
    if is_committed(&capture_outcome) {
        assert_eq!(rows.len(), 1, "the committed capture produced one candidate: {rows:?}");
    } else {
        assert!(rows.is_empty(), "the refused capture committed no candidate: {rows:?}");
    }
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["status"],
        json!("frozen")
    );
    let after = admin
        .capture(capture_body("k-c012-after", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&after), "SPACE_FROZEN", "{after}");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_012_write_racing_freeze_has_one_admission_outcome_local() {
    write_racing_freeze_has_one_admission_outcome(Matrix::local().await).await;
}

#[tokio::test]
async fn c_012_write_racing_freeze_has_one_admission_outcome_remote() {
    write_racing_freeze_has_one_admission_outcome(Matrix::remote().await).await;
}

/// C-013: a subject fence is enforced for a different authenticated client.
async fn subject_fence_applies_across_clients(matrix: Matrix) {
    bootstrap(&matrix, &["capture"]).await;
    let admin = matrix.admin();
    let peer = matrix.peer();

    let fenced = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "fence", 1, "k-c013-fence")
        .await;
    assert_eq!(result(&fenced)["status"], "fenced");
    assert_eq!(result(&fenced)["revision"], json!(2));

    let denied = peer
        .capture(capture_body("k-c013-peer", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&denied), "SUBJECT_FENCED", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");

    let ensured = peer.ensure_subject(SUBJECT).await;
    assert_eq!(error_code(&ensured), "SUBJECT_FENCED", "{ensured}");
    assert_eq!(admin.get_subject(SUBJECT).await["subject"]["status"], json!("fenced"));
    assert!(
        candidate_ids(&candidates(&admin).await).is_empty(),
        "the fenced write left no candidate"
    );

    // The fence is subject-scoped: a sibling subject still admits the write.
    let sibling = peer.ensure_subject(OTHER_SUBJECT).await;
    assert_eq!(result(&sibling)["created"], json!(true));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_013_subject_fence_applies_across_clients_local() {
    subject_fence_applies_across_clients(Matrix::local().await).await;
}

#[tokio::test]
async fn c_013_subject_fence_applies_across_clients_remote() {
    subject_fence_applies_across_clients(Matrix::remote().await).await;
}

/// C-014: the reversible fence itself survives a restart, retained reads keep
/// working under it, unfence restores writability, and the terminal archive is
/// additional evidence.
async fn reversible_archive_fence_survives_restart(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let created = admin
        .create_record(
            json!({
                "subject_id": SUBJECT,
                "kind": "nexus.world_building",
                "content": { "text": TEXT },
                "scope": shared_scope(),
                "idempotency_key": "k-c014-retained",
            }),
        )
        .await;
    let record_id = result(&created)["record_id"].as_str().expect("record id").to_owned();
    let fenced = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "fence", 1, "k-c014-fence")
        .await;
    assert_eq!(result(&fenced)["status"], "fenced");
    drop(admin);

    // The fence itself survives the restart.
    let matrix = matrix.restart().await;
    let admin = matrix.admin();
    let read = admin.get_subject(SUBJECT).await;
    assert_eq!(
        read["subject"]["status"], json!("fenced"),
        "the reversible fence survives the restart: {read}"
    );
    assert_eq!(read["subject"]["revision"], json!(2));

    // Retained reads keep working while fenced, writes are refused.
    let retained = admin.get_record(&record_id).await;
    assert_eq!(
        retained["record"]["record_id"],
        json!(record_id),
        "a retained record stays readable under the fence: {retained}"
    );
    assert_eq!(
        admin
            .recall(json!({
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
                "budget": { "max_items": 10 },
            }))
            .await["total_authorized"],
        json!(1),
        "the fenced subject still recalls its retained record"
    );
    let denied = admin
        .capture(capture_body("k-c014-blocked", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&denied), "SUBJECT_FENCED", "{denied}");

    // Unfence after the restart restores writability.
    let unfenced = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "unfence", 2, "k-c014-unfence")
        .await;
    assert_eq!(result(&unfenced)["status"], "active");
    let restored = admin
        .capture(capture_body("k-c014-restored", TEXT, &shared_scope()))
        .await;
    assert!(is_committed(&restored), "unfence restores the write: {restored}");
    drop(admin);

    // Additional evidence: the terminal archive is durable too.
    let matrix = matrix.restart().await;
    let admin = matrix.admin();
    let archived = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "archive", 3, "k-c014-archive")
        .await;
    assert_eq!(result(&archived)["status"], "archived");
    let resurrect = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "unfence", 4, "k-c014-resurrect")
        .await;
    assert_eq!(error_code(&resurrect), "INVALID_STATUS_TRANSITION", "{resurrect}");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_014_reversible_archive_fence_survives_restart_local() {
    reversible_archive_fence_survives_restart(Matrix::local().await).await;
}

#[tokio::test]
async fn c_014_reversible_archive_fence_survives_restart_remote() {
    reversible_archive_fence_survives_restart(Matrix::remote().await).await;
}

/// C-016: a context fence does not collateral-fence a sibling context or the
/// shared placement, and it is enforced for a second independent client.
async fn context_fence_does_not_collateral_fence(matrix: Matrix) {
    bootstrap(&matrix, &["capture"]).await;
    let admin = matrix.admin();
    let peer = matrix.peer();
    let fenced = admin
        .fence_subject(
            SUBJECT,
            json!({ "type": "context", "context_id": CONTEXT }),
            "fence",
            1,
            "k-c016-fence",
        )
        .await;
    assert_eq!(result(&fenced)["status"], "active");
    assert_eq!(result(&fenced)["fenced_contexts"], json!([CONTEXT]));

    // The second authenticated client performs every write below.
    let denied = peer
        .capture(capture_body("k-c016-blocked", TEXT, &context_scope(CONTEXT)))
        .await;
    assert_eq!(error_code(&denied), "SUBJECT_FENCED", "{denied}");
    assert_eq!(
        error_details(&denied)["scope"],
        json!("context"),
        "the fenced-context write names its scope (§7.7): {denied}"
    );
    assert_eq!(retry_class(&denied), "terminal");

    let sibling = peer
        .capture(capture_body("k-c016-sibling", TEXT, &context_scope(OTHER_CONTEXT)))
        .await;
    assert_eq!(result(&sibling)["classified"], json!(1));
    let shared = peer
        .capture(capture_body("k-c016-shared", TEXT, &shared_scope()))
        .await;
    assert_eq!(result(&shared)["classified"], json!(1));
    assert_eq!(
        candidate_ids(&candidates(&admin).await).len(),
        2,
        "only the two admissible writes produced candidates"
    );

    admin
        .fence_subject(
            SUBJECT,
            json!({ "type": "context", "context_id": CONTEXT }),
            "unfence",
            2,
            "k-c016-unfence",
        )
        .await;
    let restored = peer
        .capture(capture_body("k-c016-restored", TEXT, &context_scope(CONTEXT)))
        .await;
    assert_eq!(result(&restored)["classified"], json!(1));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_016_context_fence_does_not_collateral_fence_local() {
    context_fence_does_not_collateral_fence(Matrix::local().await).await;
}

#[tokio::test]
async fn c_016_context_fence_does_not_collateral_fence_remote() {
    context_fence_does_not_collateral_fence(Matrix::remote().await).await;
}

/// C-017: two clients race the same fence revision while the writer lock is
/// held. Both reach the contested boundary; exactly one commits, the other gets
/// `REVISION_CONFLICT`, and the winner's state is not lost-updated.
async fn stale_fence_revision_cannot_lost_update(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = matrix
        .admin()
        .grant(grant_body("admin", &peer_id, "k-c017-admin"))
        .await;
    assert!(is_committed(&granted), "{granted}");

    // Hold the writer lock with a real capture so both fence requests are
    // issued while the contested boundary is occupied.
    let mut hold = match matrix.kind() {
        Kind::Local => matrix.park_next_capture(),
        Kind::Remote => matrix.park_next_capture(),
    };
    let space_id = matrix.space_id().to_owned();
    let capture = tokio::spawn(support::capture_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        space_id.clone(),
        capture_body("k-c017-hold", TEXT, &shared_scope()),
    ));
    hold.wait_reached()
        .await
        .expect("the capture reached the writer boundary");

    let mut first = Box::pin(support::fence_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        space_id.clone(),
        "k-c017-first",
        1,
        SUBJECT,
    ));
    let mut second = Box::pin(support::fence_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        space_id,
        "k-c017-second",
        1,
        SUBJECT,
    ));
    still_pending(&mut first, "the admin fence").await;
    still_pending(&mut second, "the peer fence").await;

    hold.release().await;
    let _ = capture.await.expect("the holding capture completes");
    let first = first.await;
    let second = second.await;

    let committed = [&first, &second]
        .into_iter()
        .filter(|body| is_committed(body))
        .count();
    assert_eq!(committed, 1, "exactly one fence commits: {first} / {second}");
    let loser = if is_committed(&first) { &second } else { &first };
    assert_eq!(error_code(loser), "REVISION_CONFLICT", "{loser}");

    let admin = matrix.admin();
    let read = admin.get_subject(SUBJECT).await;
    assert_eq!(read["subject"]["status"], json!("fenced"), "no lost update: {read}");
    assert_eq!(read["subject"]["revision"], json!(2));
    assert_eq!(
        read["subject"]["fenced_contexts"],
        json!([]),
        "neither racing request wrote a context fence: {read}"
    );

    let unfenced = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "unfence", 2, "k-c017-unfence")
        .await;
    assert_eq!(result(&unfenced)["status"], json!("active"));
    assert_eq!(result(&unfenced)["revision"], json!(3));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_017_stale_fence_revision_cannot_lost_update_local() {
    stale_fence_revision_cannot_lost_update(Matrix::local().await).await;
}

#[tokio::test]
async fn c_017_stale_fence_revision_cannot_lost_update_remote() {
    stale_fence_revision_cannot_lost_update(Matrix::remote().await).await;
}

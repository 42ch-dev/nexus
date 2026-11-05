//! C-matrix families A and B (N3): capability admission/discovery and
//! space/subject lifecycle fencing, on both real providers.
//!
//! Every scenario body is provider-agnostic and runs twice: once against the
//! real `nexus_core` local provider and once against the independent durable
//! fixture provider. The `_local` / `_remote` suffix follows matrix §3.
//!
//! Split-row accounting (§5):
//! - C-011 is `N3 + N4`: this file proves the base-operation freeze allowlist
//!   only. The N4-T export/import allowlist success remainder stays open and is
//!   never asserted here.
//! - C-015 is N4-only and has no scenario in this file at all.
//!
//! Wire-only observations (an op with no dispatch row, an unnegotiated
//! family) go through a real signed raw session, because the typed client
//! cannot express an unknown op name.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use serde_json::{json, Value};

use support::{
    bootstrap, capture_body, context_scope, error_code, error_details, grant_body, is_committed,
    is_rejected, receipt_id, result, retry_class, shared_scope, Actor, Kind, Matrix, CONTEXT,
    OTHER_CONTEXT, OTHER_SUBJECT, SUBJECT, TEXT,
};

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
    assert!(
        body["limits"]["max_page_size"].as_u64().is_some_and(|value| value >= 1),
        "every advertised limit is a positive bound: {body}"
    );
    assert!(
        body["limits"]["max_content_bytes"]
            .as_u64()
            .is_some_and(|value| value >= 1),
        "{body}"
    );
    let receipt_window = body["limits"]["receipt_retention_seconds"]
        .as_u64()
        .expect("receipt window");
    let dedup_window = body["limits"]["dedup_retention_seconds"]
        .as_u64()
        .expect("dedup window");
    assert!(
        dedup_window >= receipt_window,
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
/// refused; neither reaches an effect.
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

    // Every family the provider does not advertise is refused at the signed
    // dispatch gate before its payload is read.
    let mut raw = matrix.raw_admin().await;
    for op in [
        "memory.reflections.state",
        "memory.reflections.list",
        "memory.reflections.generate",
        "memory.exports.create",
        "memory.exports.get",
        "memory.imports.stage",
        "memory.imports.activate",
        "memory.imports.discard",
    ] {
        let response = raw.invoke(op, &json!({})).await;
        let error = response.get("error").expect("dispatch-deny envelope");
        let code = error["code"].as_str().expect("wire code");
        assert!(
            code == "capability_missing" || code == "op_unsupported",
            "{op} answered {code}"
        );
        assert_eq!(error["details"]["retry_class"], json!("terminal"), "{op}");
    }

    // An op name with no dispatch row is not silently served.
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
        "a refused capture or gated family leaves no candidate"
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

/// C-003: reflection requires its sub-capability across state/list/generate and
/// leaves zero effects when it is absent.
async fn reflection_requires_subcapability(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let mut raw = matrix.raw_admin().await;
    for op in [
        "memory.reflections.state",
        "memory.reflections.list",
        "memory.reflections.generate",
    ] {
        let response = raw.invoke(
            op,
            &json!({
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
            }),
        )
        .await;
        let error = response.get("error").expect("capability-deny envelope");
        let code = error["code"].as_str().expect("wire code");
        assert!(
            code == "capability_missing" || code == "op_unsupported",
            "{op} answered {code}"
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
        let code = error["code"].as_str().expect("wire code");
        assert!(
            code == "capability_missing" || code == "op_unsupported",
            "{op} answered {code}"
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
/// catalogue.
async fn describe_has_no_space_catalogue(matrix: Matrix) {
    let body = matrix.admin().describe().await;
    assert!(
        body.get("spaces").is_none(),
        "describe is not a space catalogue: {body}"
    );
    assert_eq!(body["capabilities"], json!(["spoke-memory"]));
    let spaces = matrix.admin().list_spaces().await;
    assert_eq!(
        spaces["spaces"].as_array().expect("space rows").len(),
        1,
        "the catalogue lives in spaces.list, not in describe: {spaces}"
    );
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
    let admin = matrix.admin();
    let before = admin.list_spaces().await;
    let space = &before["spaces"][0];
    let identity = json!({
        "space_id": space["space_id"].clone(),
        "authority_host_id": space["authority_host_id"].clone(),
        "authority_generation": space["authority_generation"].clone(),
        "status": space["status"].clone(),
    });
    assert_eq!(identity["space_id"], json!(matrix.space_id()));
    drop(admin);

    let matrix = {
        let admin = matrix.admin();
        admin.ensure_subject(SUBJECT).await;
        drop(admin);
        matrix
    };
    let after_write = matrix.admin().list_spaces().await;
    assert_eq!(
        after_write["spaces"][0]["authority_generation"],
        identity["authority_generation"],
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
        after_restart["spaces"][0]["revision"],
        after_write["spaces"][0]["revision"],
        "no speculative space revision is written by a restart: {after_restart}"
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

    // Outside the §8.2 allowlist: fresh writes refuse.
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

    // Inside the allowlist: reads, revoke and unfreeze stay admissible.
    let spaces = admin.list_spaces().await;
    assert_eq!(spaces["spaces"][0]["status"], "frozen");
    let subject = admin.get_subject(SUBJECT).await;
    assert_eq!(subject["subject"]["subject_id"], json!(SUBJECT));
    let listing = candidates(&admin).await;
    assert!(!is_rejected(&listing), "candidate listing is a read: {listing}");
    let revoked = admin.revoke_grant(grant_id.as_str(), 1, "k-c011-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true));
    let receipt_view = admin.get_receipt(receipt_id(&frozen)).await;
    assert_eq!(receipt_view["receipt"]["receipt_id"], receipt_id(&frozen));

    let unfrozen = admin.unfreeze_space("k-c011-unfreeze", 2).await;
    assert_eq!(result(&unfrozen)["status"], "active");
    assert_eq!(result(&unfrozen)["revision"], json!(3));
    let after = admin
        .capture(capture_body("k-c011-after", TEXT, &shared_scope()))
        .await;
    assert!(is_committed(&after), "unfreezing restores the write: {after}");

    // N4-T remainder (export/import allowlist success) stays open: no claim.
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
/// outcome — never an admission on the pre-freeze state that commits after the
/// barrier.
///
/// The ordering is produced by the provider's own seam, not by a sleep: on the
/// local leg the capture is parked at its capture point while it holds the
/// writer lock, so the freeze can only commit after it; on the fixture leg the
/// admission is paused before the writer transaction, so the freeze commits
/// first and the released capture must observe the frozen space.
async fn write_racing_freeze_has_one_admission_outcome(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;

    let mut hold = match matrix.kind() {
        Kind::Local => matrix.park_next_capture(),
        Kind::Remote => matrix.park_next_admission(),
    };
    let admin_client = matrix.admin_client();
    let admin_principal = matrix.admin_principal().clone();
    let space_id = matrix.space_id().to_owned();
    let capture = tokio::spawn(support::capture_with(
        admin_client.clone(),
        admin_principal.clone(),
        space_id.clone(),
        capture_body("k-c012-race", TEXT, &shared_scope()),
    ));
    hold.wait_reached()
        .await
        .expect("the capture reached its provider point");

    let freeze = tokio::spawn(support::freeze_with(
        admin_client,
        admin_principal,
        space_id.clone(),
        "k-c012-freeze",
        1,
    ));

    hold.release().await;
    let capture_outcome = capture.await.expect("the racing capture completes");
    let freeze_outcome = freeze.await.expect("the racing freeze completes");

    let admin = matrix.admin();
    let listed = candidates(&admin).await;
    let rows = candidate_ids(&listed);
    if is_committed(&capture_outcome) {
        assert_eq!(rows.len(), 1, "a committed capture produced one candidate: {listed}");
    } else {
        assert_eq!(
            error_code(&capture_outcome),
            "SPACE_FROZEN",
            "the losing admission is the freeze refusal: {capture_outcome}"
        );
        assert!(
            rows.is_empty(),
            "a refused admission committed no candidate: {listed}"
        );
    }
    assert_eq!(result(&freeze_outcome)["status"], "frozen");

    // The barrier holds afterwards: no later write is admitted on it.
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

/// C-014: the reversible fence and the terminal archive survive a real restart
/// over the same file.
async fn reversible_archive_fence_survives_restart(matrix: Matrix) {
    bootstrap(&matrix, &["capture"]).await;
    let admin = matrix.admin();
    let fenced = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "fence", 1, "k-c014-fence")
        .await;
    assert_eq!(result(&fenced)["status"], "fenced");
    let unfenced = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "unfence", 2, "k-c014-unfence")
        .await;
    assert_eq!(result(&unfenced)["status"], "active");
    let archived = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "archive", 3, "k-c014-archive")
        .await;
    assert_eq!(result(&archived)["status"], "archived");
    assert_eq!(result(&archived)["revision"], json!(4));
    drop(admin);

    let matrix = matrix.restart().await;
    let admin = matrix.admin();
    let read = admin.get_subject(SUBJECT).await;
    assert_eq!(
        read["subject"]["status"], json!("archived"),
        "the archive survives the restart: {read}"
    );
    assert_eq!(read["subject"]["revision"], json!(4));

    let denied = admin
        .capture(capture_body("k-c014-write", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&denied), "SUBJECT_FENCED", "{denied}");
    assert_eq!(
        admin.get_subject(SUBJECT).await["subject"]["status"],
        json!("archived"),
        "the refused write left the archived fence in place"
    );
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
/// shared placement.
async fn context_fence_does_not_collateral_fence(matrix: Matrix) {
    bootstrap(&matrix, &["capture"]).await;
    let admin = matrix.admin();
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

    let denied = admin
        .capture(capture_body("k-c016-blocked", TEXT, &context_scope(CONTEXT)))
        .await;
    assert_eq!(error_code(&denied), "SUBJECT_FENCED", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");

    let sibling = admin
        .capture(capture_body("k-c016-sibling", TEXT, &context_scope(OTHER_CONTEXT)))
        .await;
    assert_eq!(result(&sibling)["classified"], json!(1));
    let shared = admin
        .capture(capture_body("k-c016-shared", TEXT, &shared_scope()))
        .await;
    assert_eq!(result(&shared)["classified"], json!(1));
    assert_eq!(
        candidate_ids(&candidates(&admin).await).len(),
        2,
        "only the two admissible captures produced candidates"
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
    let restored = admin
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

/// C-017: a stale fence revision cannot overwrite the committed fence state.
async fn stale_fence_revision_cannot_lost_update(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();
    let issued = admin
        .grant(grant_body("admin", &peer_id, "k-c017-admin"))
        .await;
    assert!(is_committed(&issued), "the admin grant commits: {issued}");

    let fenced = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "fence", 1, "k-c017-fence")
        .await;
    assert_eq!(result(&fenced)["revision"], json!(2));

    // A second client holding the pre-fence revision loses the CAS.
    let stale = matrix
        .peer()
        .fence_subject(
            SUBJECT,
            json!({ "type": "context", "context_id": CONTEXT }),
            "fence",
            1,
            "k-c017-stale",
        )
        .await;
    assert_eq!(error_code(&stale), "REVISION_CONFLICT", "{stale}");
    assert_eq!(retry_class(&stale), "terminal");

    let read = admin.get_subject(SUBJECT).await;
    assert_eq!(read["subject"]["status"], json!("fenced"), "no lost update: {read}");
    assert_eq!(read["subject"]["revision"], json!(2));
    assert_eq!(
        read["subject"]["fenced_contexts"],
        json!([]),
        "the stale context fence wrote nothing: {read}"
    );

    let unfenced = admin
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "unfence", 2, "k-c017-unfence")
        .await;
    assert_eq!(result(&unfenced)["status"], "active");
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

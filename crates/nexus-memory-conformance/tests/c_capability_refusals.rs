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
//! Each scenario body takes `&Matrix` plus the runner-owned task/resource sets,
//! so durable effects/no-effects are read from the provider's own rows through
//! the shared read-only observer, raw sessions and observers are closed on
//! failure as well as success, and every spawned operation is joined or
//! aborted+joined by the runner. `pending_receipts` is never an absence proof.
//!
//! C-100's local `retryable` limb prepares the journal-predecessor state through
//! the same served provider's existing operator controls but receives the
//! refusal over the actual signed transport.
//!
//! C-101's unknown-op envelope is asserted against protocol §14.2:750–758; the
//! pinned `spoke-connect` 0.15.1 dispatch attaches no `details` to
//! `op_unsupported` (external prerequisite I-000271, recorded in the report), so
//! that required assertion remains the known external failure. Every reachable
//! side-effect/cleanup part of C-101 still runs.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use serde_json::{json, Value};

use support::{
    assert_no_receipt_disclosure, bootstrap, capture_body, context_scope, error_code, is_committed,
    receipt_id, result, retry_class, run_matrix_scenario, shared_scope, DurableState, Kind,
    Matrix, OwnedResources, TaskSet, CONTEXT, SUBJECT, TEXT,
};

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

fn observed<'a>(state: &'a DurableState, label: &str) -> &'a [(String, String)] {
    state
        .rows
        .iter()
        .find(|(name, _)| name == label)
        .unwrap_or_else(|| panic!("the durable observation names {label}: {state:?}"))
        .1
        .as_slice()
}

fn column<'a>(row: &'a [(String, String)], name: &str) -> &'a str {
    row.iter()
        .find(|(column, _)| column == name)
        .unwrap_or_else(|| panic!("the durable row carries {name}: {row:?}"))
        .1
        .as_str()
}

fn assert_payload_secrecy(receipt: &Value) {
    fn walk(value: &Value) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    assert!(
                        !matches!(
                            key.as_str(),
                            "content" | "text" | "structured" | "document" | "body"
                        ),
                        "a receipt result carries content field {key}: {value}"
                    );
                    walk(child);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item);
                }
            }
            _ => {}
        }
    }
    walk(&receipt["result"]);
    assert!(
        !receipt.to_string().contains(TEXT),
        "the committed receipt leaks the stored body: {receipt}"
    );
}

fn assert_receipt_tuple(body: &Value, op: &str, requester: &str) {
    assert_eq!(body["status"], json!("committed"), "{body}");
    assert_eq!(body["op"], json!(op), "{body}");
    assert_eq!(body["requester_principal_id"], json!(requester), "{body}");
    assert!(
        body["receipt_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "{body}"
    );
    let digest = body["payload_digest"].as_str().expect("payload digest");
    assert_eq!(digest.len(), 64, "{body}");
    assert!(
        digest.chars().all(|c| c.is_ascii_hexdigit()),
        "the payload digest is the schema's hex shape: {body}"
    );
    assert_payload_secrecy(body);
}

/// Normalize a refusal for a security-shape comparison: caller-supplied request
/// identifiers are redacted from every string, so an echo of the requested id is
/// permitted; every other value (including any existence/ownership marker or a
/// differing message/detail) still has to match. The concrete probes pass the
/// exact ids they requested.
fn normalized_refusal(body: &Value, redactions: &[&str]) -> Value {
    fn redact(value: &Value, redactions: &[&str]) -> Value {
        match value {
            Value::String(text) => {
                let mut text = text.clone();
                for marker in redactions {
                    text = text.replace(marker, "<requested-id>");
                }
                Value::String(text)
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, child)| (key.clone(), redact(child, redactions)))
                    .collect(),
            ),
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| redact(item, redactions)).collect())
            }
            other => other.clone(),
        }
    }
    redact(&body["error"], redactions)
}

// ---------------------------------------------------------------------------
// K. Error envelope and retry classes
// ---------------------------------------------------------------------------

/// C-100 (N3 base): terminal, uncertain and no-effect retryable mechanisms.
async fn supported_error_envelopes_and_retry_classes(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();

    // 1. Terminal.
    let denied = matrix
        .peer()
        .capture(capture_body("k-c100-denied", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");

    let granted = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture"],
            "idempotency_key": "k-c100-grant",
        }))
        .await;
    assert!(is_committed(&granted), "{granted}");

    // 2. Uncertain: effect commits, response lost; replay/receipts.get resolve.
    matrix.arm_lost_response();
    let lost = matrix
        .peer()
        .capture(capture_body("k-c100-lost", TEXT, &context_scope(CONTEXT)))
        .await;
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN", "{lost}");
    assert_eq!(retry_class(&lost), "uncertain");

    let replayed = matrix
        .peer()
        .capture(capture_body("k-c100-lost", TEXT, &context_scope(CONTEXT)))
        .await;
    assert!(is_committed(&replayed), "same-key replay resolves: {replayed}");
    assert_receipt_tuple(&replayed, "memory.capture", peer_id.as_str());
    let candidate = support::candidate_ids(&replayed)[0].clone();
    let receipt = receipt_id(&replayed).to_owned();

    let effect = resources
        .open_observer(
            matrix,
            SUBJECT,
            &[],
            Some(&candidate),
            Some(&receipt),
            None,
        )
        .await;
    let state = resources.observe(effect).await;
    let candidate_row = observed(&state, &format!("candidate:{candidate}"));
    assert_eq!(column(candidate_row, "capture_visibility"), "text:context");
    assert_eq!(
        column(candidate_row, "capture_context_id"),
        format!("text:{CONTEXT}")
    );
    assert_eq!(column(candidate_row, "status"), "text:pending");
    assert_eq!(column(candidate_row, "revision"), "int:1");
    assert!(column(candidate_row, "content_json").contains(TEXT));
    let receipt_row = observed(&state, &format!("receipt:{receipt}"));
    assert_eq!(column(receipt_row, "status"), "text:committed");
    assert_eq!(column(receipt_row, "op"), "text:memory.capture");
    assert_eq!(
        column(receipt_row, "requester_principal_id"),
        format!("text:{}", peer_id.as_str())
    );
    assert!(!column(receipt_row, "result_json").contains(TEXT));
    resources
        .close_observer(effect)
        .await
        .expect("the effect observer closes");

    let stored = admin.get_receipt(&receipt).await;
    assert_eq!(stored["receipt"]["receipt_id"], json!(receipt));
    assert_eq!(stored["receipt"]["status"], json!("committed"));
    assert_eq!(pending_candidates(&admin).await, vec![candidate.clone()]);

    // 3. Retryable: a real no-effect failure before the effect commits.
    match matrix.kind() {
        Kind::Remote => {
            let window = resources
                .open_observer(matrix, SUBJECT, &[], None, None, None)
                .await;
            let before = resources.observe(window).await;
            let before_captures = resources.receipt_ids_for(window, "memory.capture").await;
            let before_candidates = pending_candidates(&admin).await;
            let mut hold = matrix.park_next_admission();
            let capture = tasks.spawn(support::capture_with(
                matrix.peer_client(),
                matrix.peer_principal().clone(),
                matrix.space_id().to_owned(),
                capture_body("k-c100-retry", TEXT, &shared_scope()),
            ));
            hold.wait_reached()
                .await
                .expect("the capture reached the bounded admission point");
            drop(hold);
            let failed = tasks
                .join(capture, "the retryable capture")
                .await
                .expect("the retryable capture settles");
            assert_eq!(error_code(&failed), "INTERNAL_ERROR", "{failed}");
            assert_eq!(retry_class(&failed), "retryable");
            let after = resources.observe(window).await;
            assert_eq!(
                resources.receipt_ids_for(window, "memory.capture").await,
                before_captures,
                "the retryable failure produced no capture receipt"
            );
            assert_eq!(after.rows, before.rows);
            assert_eq!(
                pending_candidates(&admin).await,
                before_candidates,
                "the retryable failure added no candidate"
            );
            resources
                .close_observer(window)
                .await
                .expect("the retryable observer closes");
        }
        Kind::Local => {
            local_retryable_over_the_same_signed_provider(matrix, &admin, resources).await;
        }
    }
}

/// C-100 (`_local`): the journal-predecessor state is prepared through the same
/// served provider's operator controls; the retryable refusal is received over
/// the actual signed transport.
async fn local_retryable_over_the_same_signed_provider(
    matrix: &Matrix,
    admin: &support::Actor<'_>,
    resources: &mut OwnedResources,
) {
    let operator = matrix
        .local_operator()
        .expect("the local leg exposes its operator controls");

    let created = admin
        .create_record(json!({
            "subject_id": SUBJECT,
            "kind": "nexus.world_building",
            "content": { "text": TEXT },
            "scope": shared_scope(),
            "idempotency_key": "k-c100-create",
        }))
        .await;
    let record_id = result(&created)["record_id"]
        .as_str()
        .expect("record id")
        .to_owned();

    operator.arm_commit_fault(true);
    let lost_update = admin
        .update_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "content": { "text": "content the caller never saw" },
            "idempotency_key": "k-c100-update",
        }))
        .await;
    assert_eq!(error_code(&lost_update), "OUTCOME_UNCERTAIN", "{lost_update}");
    assert_eq!(retry_class(&lost_update), "uncertain");

    let target = operator.file_root().join(format!("records/{record_id}.md"));
    std::fs::create_dir_all(target.parent().expect("record parent")).expect("record dir");
    let foreign = "foreign bytes this intent never produced";
    std::fs::write(&target, foreign).expect("foreign target bytes");

    let window = resources
        .open_observer(matrix, SUBJECT, &[record_id.clone()], None, None, None)
        .await;
    let before = resources.observe(window).await;
    let deletes_before = resources
        .receipt_ids_for(window, "memory.records.delete")
        .await;
    assert!(deletes_before.is_empty(), "no delete receipt exists yet");
    let refused = admin
        .delete_record(json!({
            "record_id": record_id,
            "expected_revision": 2,
            "idempotency_key": "k-c100-delete",
        }))
        .await;
    assert_eq!(error_code(&refused), "INTERNAL_ERROR", "{refused}");
    assert_eq!(
        retry_class(&refused),
        "retryable",
        "the retry class crossed the actual signed transport: {refused}"
    );
    let predecessor = refused["error"]["details"]["receipt_id"]
        .as_str()
        .expect("the refusal names its unsettled predecessor over the wire")
        .to_owned();
    let after = resources.observe(window).await;
    assert_eq!(
        observed(&after, &format!("record:{record_id}")),
        observed(&before, &format!("record:{record_id}")),
        "the refused delete did not tombstone or rewrite the record"
    );
    assert_eq!(
        resources
            .receipt_ids_for(window, "memory.records.delete")
            .await,
        deletes_before,
        "the refused delete created no delete receipt"
    );
    resources
        .close_observer(window)
        .await
        .expect("the refused-delete observer closes");
    assert_eq!(
        std::fs::read_to_string(&target).expect("target readable"),
        foreign,
        "the refused delete published nothing"
    );

    let predecessor_view = resources
        .open_observer(matrix, SUBJECT, &[], None, Some(&predecessor), None)
        .await;
    let predecessor_state = resources.observe(predecessor_view).await;
    assert_eq!(
        column(
            observed(&predecessor_state, &format!("receipt:{predecessor}")),
            "status"
        ),
        "text:failed"
    );
    resources
        .close_observer(predecessor_view)
        .await
        .expect("the predecessor observer closes");
}

/// C-101: unknown op vs missing capability, with the positive on the same
/// session and the capture baseline taken BEFORE the capability-less request.
/// The final terminal-metadata assertion is the known external SDK prerequisite.
async fn unknown_op_and_missing_capability_are_distinct(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let window = resources
        .open_observer(matrix, SUBJECT, &[], None, None, None)
        .await;
    let before = resources.observe(window).await;
    let captures_before = resources.receipt_ids_for(window, "memory.capture").await;

    let with_memory = resources.open_raw_admin(matrix).await;
    let described = resources
        .invoke(with_memory, "memory.describe", &json!({}))
        .await;
    assert!(
        described.get("error").is_none(),
        "the same signed session serves a real op: {described}"
    );
    assert_eq!(
        described["payload"]["capabilities"],
        json!(["spoke-memory"]),
        "{described}"
    );

    let without_memory = resources.open_raw_without_memory(matrix).await;
    let missing = resources
        .invoke(
            without_memory,
            "memory.capture",
            &json!({ "space_id": matrix.space_id(), "idempotency_key": "k-c101" }),
        )
        .await;
    assert_eq!(missing["error"]["code"], json!("capability_missing"), "{missing}");
    assert_eq!(missing["error"]["details"]["retry_class"], json!("terminal"));
    resources
        .close_raw(without_memory)
        .await
        .expect("the capability-less raw session settles");

    let unknown = resources
        .invoke(with_memory, "memory.not_an_op", &json!({}))
        .await;
    resources
        .close_raw(with_memory)
        .await
        .expect("the raw session settles");
    assert_ne!(
        unknown["error"]["code"], missing["error"]["code"],
        "unknown op and missing capability are distinct: {unknown} {missing}"
    );
    assert_eq!(unknown["error"]["code"], json!("op_unsupported"), "{unknown}");

    let after = resources.observe(window).await;
    assert_eq!(after.rows, before.rows);
    assert_eq!(
        resources.receipt_ids_for(window, "memory.capture").await,
        captures_before,
        "neither request created a capture receipt"
    );
    resources
        .close_observer(window)
        .await
        .expect("the C-101 observer closes");
    assert!(pending_candidates(&admin).await.is_empty());

    // Protocol §14.2:750–758; the pinned SDK currently omits this metadata
    // (external prerequisite I-000271), so this is the known blocked assertion.
    assert_eq!(
        unknown["error"]["details"]["retry_class"],
        json!("terminal"),
        "§14.2 requires terminal retry metadata for op_unsupported: {unknown}"
    );
}

/// C-102: normalized security-relevant refusal shape across foreign/missing
/// records and subjects and a revoked read, with no existence/ownership leak.
async fn foreign_and_missing_ids_have_no_existence_oracle(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
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

    let window = resources
        .open_observer(
            matrix,
            SUBJECT,
            &[covered_id.clone(), foreign_id.clone()],
            None,
            None,
            None,
        )
        .await;
    let before = resources.observe(window).await;

    assert_eq!(
        peer.get_record(&covered_id).await["record"]["record_id"],
        json!(covered_id)
    );

    let foreign_probe = peer.get_record(&foreign_id).await;
    let missing_probe = peer.get_record("rec_c102_missing").await;
    let redactions = [foreign_id.as_str(), "rec_c102_missing"];
    assert_eq!(error_code(&foreign_probe), "NOT_FOUND", "{foreign_probe}");
    assert_eq!(error_code(&missing_probe), "NOT_FOUND", "{missing_probe}");
    assert_eq!(
        normalized_refusal(&foreign_probe, &redactions),
        normalized_refusal(&missing_probe, &redactions),
        "foreign and missing records share one normalized security shape"
    );
    let leaked = format!("{foreign_probe}{missing_probe}");
    assert!(!leaked.contains("a secret from another subject"));
    assert!(!leaked.contains(support::OTHER_SUBJECT));

    let foreign_subject = peer.get_subject(support::OTHER_SUBJECT).await;
    let missing_subject = peer.get_subject("sbj_c102_missing").await;
    let subject_redactions = [support::OTHER_SUBJECT, "sbj_c102_missing"];
    assert_eq!(
        error_code(&foreign_subject),
        error_code(&missing_subject),
        "a foreign subject and a missing one share one refusal class: {foreign_subject} {missing_subject}"
    );
    assert_eq!(
        retry_class(&foreign_subject),
        retry_class(&missing_subject)
    );
    assert_eq!(
        normalized_refusal(&foreign_subject, &subject_redactions),
        normalized_refusal(&missing_subject, &subject_redactions),
        "foreign and missing subjects share one normalized security shape"
    );
    let subject_leak = format!("{foreign_subject}{missing_subject}");
    assert!(!subject_leak.contains("a secret from another subject"));

    let after = resources.observe(window).await;
    assert_eq!(after.rows, before.rows);
    resources
        .close_observer(window)
        .await
        .expect("the C-102 observer closes");

    let revoked = admin.revoke_grant(&grant_id, 1, "k-c102-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");
    let revoked_window = resources
        .open_observer(
            matrix,
            SUBJECT,
            &[covered_id.clone(), foreign_id],
            None,
            None,
            None,
        )
        .await;
    let revoked_before = resources.observe(revoked_window).await;
    let after_revoke = peer.get_record(&covered_id).await;
    assert_eq!(error_code(&after_revoke), "NOT_FOUND", "{after_revoke}");
    assert_eq!(retry_class(&after_revoke), "terminal");
    let revoked_redactions = [covered_id.as_str(), "rec_c102_missing"];
    assert_eq!(
        normalized_refusal(&after_revoke, &revoked_redactions),
        normalized_refusal(&missing_probe, &revoked_redactions),
        "a revoked reader gets the same normalized absence as a missing id"
    );
    let revoked_after = resources.observe(revoked_window).await;
    assert_eq!(revoked_after.rows, revoked_before.rows);
    resources
        .close_observer(revoked_window)
        .await
        .expect("the revoked-read observer closes");
}

/// C-103: pre-COMMIT failure (no capture receipt/row change) and post-COMMIT
/// loss (durable candidate + content-free receipt tuple, replay not double-applied).
async fn internal_error_reports_effect_honesty(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    bootstrap(matrix, &["capture"]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();

    let window = resources
        .open_observer(matrix, SUBJECT, &[], None, None, None)
        .await;
    let before = resources.observe(window).await;
    let captures_before = resources.receipt_ids_for(window, "memory.capture").await;
    let candidates_before = pending_candidates(&admin).await;
    matrix.arm_commit_boundary_failure();
    let failed = matrix
        .peer()
        .capture(capture_body("k-c103-before", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&failed), "OUTCOME_UNCERTAIN", "{failed}");
    assert_eq!(retry_class(&failed), "uncertain");
    let after = resources.observe(window).await;
    assert_eq!(
        resources.receipt_ids_for(window, "memory.capture").await,
        captures_before,
        "a failure before COMMIT produced no capture receipt"
    );
    assert_eq!(after.rows, before.rows);
    assert_eq!(pending_candidates(&admin).await, candidates_before);
    resources
        .close_observer(window)
        .await
        .expect("the pre-commit observer closes");

    matrix.arm_lost_response();
    let lost = matrix
        .peer()
        .capture(capture_body("k-c103-after", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN", "{lost}");
    assert_eq!(retry_class(&lost), "uncertain");

    let replayed = matrix
        .peer()
        .capture(capture_body("k-c103-after", TEXT, &shared_scope()))
        .await;
    assert!(is_committed(&replayed), "{replayed}");
    assert_receipt_tuple(&replayed, "memory.capture", peer_id.as_str());
    let durable = support::candidate_ids(&replayed);
    assert_eq!(durable.len(), 1, "the replay resolves exactly one candidate");
    let candidate = durable[0].clone();
    let receipt = receipt_id(&replayed).to_owned();

    let effect = resources
        .open_observer(
            matrix,
            SUBJECT,
            &[],
            Some(&candidate),
            Some(&receipt),
            None,
        )
        .await;
    let state = resources.observe(effect).await;
    let candidate_row = observed(&state, &format!("candidate:{candidate}"));
    assert_eq!(column(candidate_row, "status"), "text:pending");
    assert_eq!(column(candidate_row, "capture_visibility"), "text:shared");
    assert_eq!(column(candidate_row, "revision"), "int:1");
    assert!(column(candidate_row, "content_json").contains(TEXT));
    let receipt_row = observed(&state, &format!("receipt:{receipt}"));
    assert_eq!(column(receipt_row, "status"), "text:committed");
    assert_eq!(
        column(receipt_row, "requester_principal_id"),
        format!("text:{}", peer_id.as_str())
    );
    assert!(!column(receipt_row, "result_json").contains(TEXT));
    resources
        .close_observer(effect)
        .await
        .expect("the post-commit observer closes");
    assert_eq!(pending_candidates(&admin).await, vec![candidate]);
}

/// C-104: a revoked requester's receipt read is refused with no part of the
/// actual authorized receipt disclosed anywhere.
async fn receipt_read_rechecks_revoked_authorization(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
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
    let candidate = support::candidate_ids(&captured)[0].clone();

    let first = matrix.peer().get_receipt(&receipt).await;
    assert_eq!(first["receipt"]["receipt_id"], json!(receipt), "{first}");

    let revoked = admin.revoke_grant(&grant_id, 1, "k-c104-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");

    let window = resources
        .open_observer(
            matrix,
            SUBJECT,
            &[],
            Some(&candidate),
            Some(&receipt),
            None,
        )
        .await;
    let before = resources.observe(window).await;
    let second = matrix.peer().get_receipt(&receipt).await;
    assert!(
        matches!(error_code(&second), "NOT_AUTHORIZED" | "GRANT_REVOKED"),
        "{second}"
    );
    assert_eq!(retry_class(&second), "terminal");
    assert_no_receipt_disclosure(&second, &first["receipt"]);
    let after = resources.observe(window).await;
    assert_eq!(after.rows, before.rows);
    resources
        .close_observer(window)
        .await
        .expect("the C-104 observer closes");

    assert_eq!(
        admin.get_receipt(&receipt).await["receipt"]["receipt_id"],
        json!(receipt)
    );
}

/// C-073 (N3 refusal): the unavailable reflection generation effect/receipt is
/// absent before BOTH state/list reads and after both, checked on the provider's
/// own durable rows and exact generation receipt ids.
async fn reflection_reads_refuse_without_capability(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let window = resources
        .open_observer(matrix, SUBJECT, &[], None, None, None)
        .await;
    let before = resources.observe(window).await;
    let generation_before = resources
        .receipt_ids_for(window, "memory.reflections.generate")
        .await;
    assert!(
        generation_before.is_empty(),
        "no reflection generation receipt exists before the read entries"
    );
    let spaces_before = admin.list_spaces().await;

    let raw = resources.open_raw_admin(matrix).await;
    for op in ["memory.reflections.state", "memory.reflections.list"] {
        let response = resources
            .invoke(
                raw,
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
    resources
        .close_raw(raw)
        .await
        .expect("the raw session settles");

    let after = resources.observe(window).await;
    assert_eq!(after.rows, before.rows);
    assert_eq!(
        resources
            .receipt_ids_for(window, "memory.reflections.generate")
            .await,
        generation_before,
        "the refused reads created no generation receipt"
    );
    resources
        .close_observer(window)
        .await
        .expect("the C-073 observer closes");
    assert_eq!(
        admin.list_spaces().await["spaces"],
        spaces_before["spaces"],
        "the refused reads did not mutate the space projection"
    );
    assert!(pending_candidates(&admin).await.is_empty());
}

#[tokio::test]
async fn c_100_supported_error_envelopes_and_retry_classes_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { supported_error_envelopes_and_retry_classes(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_100_supported_error_envelopes_and_retry_classes_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { supported_error_envelopes_and_retry_classes(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_101_unknown_op_and_missing_capability_are_distinct_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { unknown_op_and_missing_capability_are_distinct(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_101_unknown_op_and_missing_capability_are_distinct_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { unknown_op_and_missing_capability_are_distinct(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_102_foreign_and_missing_ids_have_no_existence_oracle_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { foreign_and_missing_ids_have_no_existence_oracle(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_102_foreign_and_missing_ids_have_no_existence_oracle_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { foreign_and_missing_ids_have_no_existence_oracle(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_103_internal_error_reports_effect_honesty_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { internal_error_reports_effect_honesty(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_103_internal_error_reports_effect_honesty_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { internal_error_reports_effect_honesty(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_104_receipt_read_rechecks_revoked_authorization_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { receipt_read_rechecks_revoked_authorization(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_104_receipt_read_rechecks_revoked_authorization_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { receipt_read_rechecks_revoked_authorization(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_073_reflection_reads_refuse_without_capability_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { reflection_reads_refuse_without_capability(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_073_reflection_reads_refuse_without_capability_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { reflection_reads_refuse_without_capability(m, r, t).await })
    })
    .await;
}

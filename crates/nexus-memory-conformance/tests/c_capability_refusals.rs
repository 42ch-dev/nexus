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
//! Each scenario body takes `&Matrix` so the `_local` leg runs the real
//! `nexus_core` `LocalMemoryProvider` and the `_remote` leg runs the
//! independent durable fixture provider behind the published `spoke-connect`
//! memory service. Durable effects/no-effects are read from the provider's own
//! rows through the shared read-only observer; `pending_receipts` is never used
//! as an absence proof. Every scenario is driven by [`run_matrix_scenario`],
//! which settles both sessions and the provider on success AND on failure.
//!
//! C-100's `retryable` limb on the local leg is prepared through the provider's
//! existing operator controls but the final refusal is received over the SAME
//! signed transport the other legs use. C-101's unknown-op envelope is asserted
//! against protocol §14.2:750–758; the pinned `spoke-connect` 0.15.1 dispatch
//! attaches no `details` to `op_unsupported` (see the report), so that required
//! assertion is the known external prerequisite, not a waived criterion.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use serde_json::{json, Value};

use support::{
    bootstrap, capture_body, context_scope, error_code, is_committed, receipt_id, result,
    retry_class, run_matrix_scenario, shared_scope, DurableState, Kind, Matrix, BOUND, CONTEXT,
    SUBJECT, TEXT,
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

/// A refused receipt-read response must not disclose a receipt body anywhere,
/// and none of the supplied payload markers may appear.
fn assert_no_receipt_disclosure(response: &Value, markers: &[&str]) {
    fn walk(value: &Value, path: &str) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let here = format!("{path}.{key}");
                    assert_ne!(
                        key, "receipt",
                        "a refused response discloses a receipt at {here}: {value}"
                    );
                    assert!(
                        !matches!(key.as_str(), "result" | "payload_digest" | "completed_at"),
                        "a refused response carries receipt metadata at {here}: {value}"
                    );
                    walk(child, &here);
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &format!("{path}[{index}]"));
                }
            }
            _ => {}
        }
    }
    walk(response, "$");
    let rendered = response.to_string();
    for marker in markers {
        assert!(
            !rendered.contains(marker),
            "the refusal leaks the receipt payload marker {marker}: {response}"
        );
    }
}

async fn joined(handle: tokio::task::JoinHandle<Value>, what: &str) -> Value {
    match tokio::time::timeout(BOUND, handle).await {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => panic!("{what} task failed: {error}"),
        Err(_) => panic!("{what} did not finish within the harness bound"),
    }
}

// ---------------------------------------------------------------------------
// K. Error envelope and retry classes
// ---------------------------------------------------------------------------

/// C-100 (N3 base): a supported terminal refusal, an uncertain commit and a
/// no-effect retryable failure each carry `details.retry_class` that matches
/// their actual meaning. The uncertain outcome is resolved only through
/// same-key replay or `receipts.get`, and the durable effect is checked against
/// the provider's own candidate/receipt rows.
async fn supported_error_envelopes_and_retry_classes(matrix: &Matrix) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();

    // 1. Terminal: no capture grant; retrying unchanged cannot help.
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

    // 2. Uncertain: the effect commits and the response is lost. Same-key
    //    replay and `receipts.get` are the only resolution channels.
    matrix.arm_lost_response();
    let lost = matrix
        .peer()
        .capture(capture_body(
            "k-c100-lost",
            TEXT,
            &context_scope(CONTEXT),
        ))
        .await;
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN", "{lost}");
    assert_eq!(retry_class(&lost), "uncertain");

    let replayed = matrix
        .peer()
        .capture(capture_body(
            "k-c100-lost",
            TEXT,
            &context_scope(CONTEXT),
        ))
        .await;
    assert!(is_committed(&replayed), "same-key replay resolves: {replayed}");
    assert_receipt_tuple(&replayed, "memory.capture", &peer_id);
    let candidate = support::candidate_ids(&replayed)[0].clone();
    let receipt = receipt_id(&replayed).to_owned();

    let mut effect = matrix
        .durable_observer(SUBJECT, &[], Some(&candidate), Some(&receipt), None)
        .await;
    let state = effect.observe().await;
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
    effect.close().await;

    let stored = admin.get_receipt(&receipt).await;
    assert_eq!(stored["receipt"]["receipt_id"], json!(receipt));
    assert_eq!(stored["receipt"]["status"], json!("committed"));
    assert_eq!(
        pending_candidates(&admin).await,
        vec![candidate.clone()],
        "the lost response applied exactly one effect"
    );

    // 3. Retryable: a real no-effect failure before the effect commits.
    match matrix.kind() {
        Kind::Remote => {
            let mut window = matrix
                .durable_observer(SUBJECT, &[], None, None, None)
                .await;
            let before = window.observe().await;
            let before_captures = window.receipt_ids_for("memory.capture").await;
            let before_candidates = pending_candidates(&admin).await;
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
            drop(hold);
            let failed = joined(capture, "the retryable capture").await;
            assert_eq!(error_code(&failed), "INTERNAL_ERROR", "{failed}");
            assert_eq!(retry_class(&failed), "retryable");
            let after = window.observe().await;
            assert_eq!(
                window.receipt_ids_for("memory.capture").await,
                before_captures,
                "the retryable failure produced no capture receipt"
            );
            assert_eq!(after.rows, before.rows);
            assert_eq!(
                pending_candidates(&admin).await,
                before_candidates,
                "the retryable failure added no candidate"
            );
            window.close().await;
        }
        Kind::Local => {
            local_retryable_over_the_same_signed_provider(matrix, &admin).await;
        }
    }
}

/// C-100 (`_local`): prepare the local provider's real journal-predecessor
/// state through its existing operator controls, then receive the retryable
/// refusal over the SAME signed transport (not a direct-only call).
async fn local_retryable_over_the_same_signed_provider(matrix: &Matrix, admin: &support::Actor<'_>) {
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

    // The update commits but its response is lost: the journal intent stays
    // unsettled. This is operator-only preparation; the final refusal is
    // observed over the wire below.
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

    // Make the pending projection target unresolvable through the provider's
    // own file-root control.
    let target = operator.file_root().join(format!("records/{record_id}.md"));
    std::fs::create_dir_all(target.parent().expect("record parent")).expect("record dir");
    let foreign = "foreign bytes this intent never produced";
    std::fs::write(&target, foreign).expect("foreign target bytes");

    let mut window = matrix
        .durable_observer(SUBJECT, &[record_id.clone()], None, None, None)
        .await;
    let before = window.observe().await;
    let deletes_before = window.receipt_ids_for("memory.records.delete").await;
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
    let after = window.observe().await;
    assert_eq!(
        observed(&after, &format!("record:{record_id}")),
        observed(&before, &format!("record:{record_id}")),
        "the refused delete did not tombstone or rewrite the record"
    );
    assert_eq!(
        window.receipt_ids_for("memory.records.delete").await,
        deletes_before,
        "the refused delete created no delete receipt"
    );
    window.close().await;
    assert_eq!(
        std::fs::read_to_string(&target).expect("target readable"),
        foreign,
        "the refused delete published nothing"
    );
    // The predecessor's own durable receipt is the quarantined one the refusal
    // named; that is provider maintenance, not the delete's effect.
    let mut predecessor_view = matrix
        .durable_observer(SUBJECT, &[], None, Some(&predecessor), None)
        .await;
    let predecessor_state = predecessor_view.observe().await;
    assert_eq!(
        column(
            observed(&predecessor_state, &format!("receipt:{predecessor}")),
            "status"
        ),
        "text:failed"
    );
    predecessor_view.close().await;
}

/// C-101: an unknown wire op is `op_unsupported`; a known op on a session that
/// never negotiated `spoke-memory` is `capability_missing` at the dispatch
/// boundary. A valid op on the same boundary succeeds, and neither refusal
/// creates state.
///
/// Protocol §14.2:750–758 requires `details.retry_class = terminal` on EVERY
/// error envelope, including `op_unsupported`. The pinned `spoke-connect`
/// 0.15.1 dispatch attaches that class only for `capability_missing`
/// (`remote/responder.rs:1108-1121`; the envelope inserts `details` only when
/// supplied, `remote/responder.rs:1687-1714`), so the required assertion below
/// is the known external prerequisite recorded in the report.
async fn unknown_op_and_missing_capability_are_distinct(matrix: &Matrix) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let mut window = matrix
        .durable_observer(SUBJECT, &[], None, None, None)
        .await;
    let before = window.observe().await;

    let mut with_memory = matrix.raw_admin().await;
    // The valid positive on the same session and boundary.
    let described = with_memory.invoke("memory.describe", &json!({})).await;
    assert!(
        described.get("error").is_none(),
        "the same signed session serves a real op: {described}"
    );
    assert_eq!(
        described["payload"]["capabilities"],
        json!(["spoke-memory"]),
        "{described}"
    );

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
        "{missing}"
    );
    assert_eq!(missing["error"]["details"]["retry_class"], json!("terminal"));
    without_memory
        .close()
        .await
        .expect("the capability-less raw session settles");

    let captures_before = window.receipt_ids_for("memory.capture").await;
    let unknown = with_memory.invoke("memory.not_an_op", &json!({})).await;
    with_memory.close().await.expect("the raw session settles");
    assert_ne!(
        unknown["error"]["code"], missing["error"]["code"],
        "unknown op and missing capability are distinct: {unknown} {missing}"
    );
    assert_eq!(
        unknown["error"]["code"],
        json!("op_unsupported"),
        "{unknown}"
    );

    // No refusal created provider state.
    let after = window.observe().await;
    assert_eq!(after.rows, before.rows);
    assert_eq!(
        window.receipt_ids_for("memory.capture").await,
        captures_before,
        "neither refusal created a capture receipt"
    );
    window.close().await;
    assert!(
        pending_candidates(&admin).await.is_empty(),
        "neither refusal created a candidate"
    );

    // Required by protocol §14.2:750–758; see the module note for the pinned
    // SDK's current omission. This is the known external prerequisite, not a
    // waived criterion.
    assert_eq!(
        unknown["error"]["details"]["retry_class"],
        json!("terminal"),
        "§14.2 requires terminal retry metadata for op_unsupported: {unknown}"
    );
}

/// C-102: a foreign id and a missing id answer the same permitted refusal; no
/// content or ownership leaks, and authorization is rechecked before existence.
/// The refusal class is compared, not whole provider-specific detail objects.
async fn foreign_and_missing_ids_have_no_existence_oracle(matrix: &Matrix) {
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

    let mut window = matrix
        .durable_observer(
            SUBJECT,
            &[covered_id.clone(), foreign_id.clone()],
            None,
            None,
            None,
        )
        .await;
    let before = window.observe().await;

    assert_eq!(
        peer.get_record(&covered_id).await["record"]["record_id"],
        json!(covered_id)
    );

    let foreign_probe = peer.get_record(&foreign_id).await;
    let missing_probe = peer.get_record("rec_c102_missing").await;
    assert_eq!(error_code(&foreign_probe), "NOT_FOUND", "{foreign_probe}");
    assert_eq!(error_code(&missing_probe), "NOT_FOUND", "{missing_probe}");
    assert_eq!(
        retry_class(&foreign_probe),
        retry_class(&missing_probe),
        "foreign and missing records share one refusal class"
    );
    let leaked = format!("{foreign_probe}{missing_probe}");
    assert!(!leaked.contains("a secret from another subject"));
    assert!(!leaked.contains(support::OTHER_SUBJECT));

    let foreign_subject = peer.get_subject(support::OTHER_SUBJECT).await;
    let missing_subject = peer.get_subject("sbj_c102_missing").await;
    assert_eq!(
        error_code(&foreign_subject),
        error_code(&missing_subject),
        "a foreign subject and a missing one share one refusal class: {foreign_subject} {missing_subject}"
    );
    assert!(matches!(
        error_code(&foreign_subject),
        "NOT_FOUND" | "NOT_AUTHORIZED"
    ));
    assert_eq!(
        retry_class(&foreign_subject),
        retry_class(&missing_subject)
    );
    let subject_leak = format!("{foreign_subject}{missing_subject}");
    assert!(!subject_leak.contains("a secret from another subject"));

    // Authorization is rechecked before existence: revoking the grant turns the
    // covered read into the same absence class as a missing id.
    let after = window.observe().await;
    assert_eq!(after.rows, before.rows);
    window.close().await;

    let revoked = admin.revoke_grant(&grant_id, 1, "k-c102-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");
    let mut revoked_window = matrix
        .durable_observer(
            SUBJECT,
            &[covered_id.clone(), foreign_id],
            None,
            None,
            None,
        )
        .await;
    let revoked_before = revoked_window.observe().await;
    let after_revoke = peer.get_record(&covered_id).await;
    assert_eq!(error_code(&after_revoke), "NOT_FOUND", "{after_revoke}");
    assert_eq!(retry_class(&after_revoke), "terminal");
    assert_eq!(
        error_code(&after_revoke),
        error_code(&missing_probe),
        "a revoked reader gets the same absence as a missing id"
    );
    let revoked_after = revoked_window.observe().await;
    assert_eq!(revoked_after.rows, revoked_before.rows);
    revoked_window.close().await;
}

/// C-103: a failure before COMMIT and a loss after COMMIT are reported honestly.
/// The pre-commit case creates no capture receipt and changes no durable row; the post-commit case's durable candidate and committed
/// content-free receipt tuple are checked, and replay does not double-apply.
async fn internal_error_reports_effect_honesty(matrix: &Matrix) {
    bootstrap(matrix, &["capture"]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();

    // 1. Failure before COMMIT.
    let mut window = matrix
        .durable_observer(SUBJECT, &[], None, None, None)
        .await;
    let before = window.observe().await;
    let captures_before = window.receipt_ids_for("memory.capture").await;
    let candidates_before = pending_candidates(&admin).await;
    matrix.arm_commit_boundary_failure();
    let failed = matrix
        .peer()
        .capture(capture_body("k-c103-before", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&failed), "OUTCOME_UNCERTAIN", "{failed}");
    assert_eq!(retry_class(&failed), "uncertain");
    let after = window.observe().await;
    assert_eq!(
        window.receipt_ids_for("memory.capture").await,
        captures_before,
        "a failure before COMMIT produced no capture receipt"
    );
    assert_eq!(
        after.rows, before.rows,
        "a failure before COMMIT changed no durable row"
    );
    assert_eq!(
        pending_candidates(&admin).await,
        candidates_before,
        "a failure before COMMIT added no candidate"
    );
    window.close().await;

    // 2. Loss after COMMIT.
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
    assert_receipt_tuple(&replayed, "memory.capture", &peer_id);
    let durable = support::candidate_ids(&replayed);
    assert_eq!(durable.len(), 1, "the replay resolves exactly one candidate");
    let candidate = durable[0].clone();
    let receipt = receipt_id(&replayed).to_owned();

    let mut effect = matrix
        .durable_observer(SUBJECT, &[], Some(&candidate), Some(&receipt), None)
        .await;
    let state = effect.observe().await;
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
    effect.close().await;

    assert_eq!(
        pending_candidates(&admin).await,
        vec![candidate],
        "the replay did not double-apply"
    );
}

/// C-104: a receipt read rechecks the caller's current authorization. The
/// refusal discloses no receipt body anywhere; the durable state is unchanged.
async fn receipt_read_rechecks_revoked_authorization(matrix: &Matrix) {
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

    let mut window = matrix
        .durable_observer(SUBJECT, &[], Some(&candidate), Some(&receipt), None)
        .await;
    let before = window.observe().await;
    let second = matrix.peer().get_receipt(&receipt).await;
    assert!(
        matches!(error_code(&second), "NOT_AUTHORIZED" | "GRANT_REVOKED"),
        "{second}"
    );
    assert_eq!(retry_class(&second), "terminal");
    assert_no_receipt_disclosure(&second, &[candidate.as_str(), TEXT]);
    let after = window.observe().await;
    assert_eq!(after.rows, before.rows);
    window.close().await;

    assert_eq!(
        admin.get_receipt(&receipt).await["receipt"]["receipt_id"],
        json!(receipt),
        "the authorized principal still reads the same receipt"
    );
}

/// C-073 (N3 refusal): the unavailable reflection family's state/list reads are
/// refused truthfully with zero durable effect and no generation/receipt
/// mutation. Successful state/list/sufficiency behavior is N4-R and is not
/// claimed.
async fn reflection_reads_refuse_without_capability(matrix: &Matrix) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let mut window = matrix
        .durable_observer(SUBJECT, &[], None, None, None)
        .await;
    let before = window.observe().await;
    let spaces_before = admin.list_spaces().await;

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
    raw.close().await.expect("the raw session settles");

    // A real re-open proves the served session remains usable after the bounded
    // close (a resource observation, not a bare not-throw).
    let mut reopened = matrix.raw_admin().await;
    let served = reopened.invoke("memory.describe", &json!({})).await;
    assert_eq!(
        served["payload"]["capabilities"],
        json!(["spoke-memory"]),
        "{served}"
    );
    reopened.close().await.expect("the reopened raw session settles");

    let after = window.observe().await;
    assert_eq!(after.rows, before.rows);
    for op in ["memory.reflections.state", "memory.reflections.list"] {
        assert!(
            window.receipt_ids_for(op).await.is_empty(),
            "{op} created no receipt"
        );
    }
    window.close().await;
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
    run_matrix_scenario(matrix, |m| Box::pin(async move { supported_error_envelopes_and_retry_classes(m).await })).await;
}

#[tokio::test]
async fn c_100_supported_error_envelopes_and_retry_classes_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { supported_error_envelopes_and_retry_classes(m).await })).await;
}

#[tokio::test]
async fn c_101_unknown_op_and_missing_capability_are_distinct_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { unknown_op_and_missing_capability_are_distinct(m).await })).await;
}

#[tokio::test]
async fn c_101_unknown_op_and_missing_capability_are_distinct_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { unknown_op_and_missing_capability_are_distinct(m).await })).await;
}

#[tokio::test]
async fn c_102_foreign_and_missing_ids_have_no_existence_oracle_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { foreign_and_missing_ids_have_no_existence_oracle(m).await })).await;
}

#[tokio::test]
async fn c_102_foreign_and_missing_ids_have_no_existence_oracle_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { foreign_and_missing_ids_have_no_existence_oracle(m).await })).await;
}

#[tokio::test]
async fn c_103_internal_error_reports_effect_honesty_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { internal_error_reports_effect_honesty(m).await })).await;
}

#[tokio::test]
async fn c_103_internal_error_reports_effect_honesty_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { internal_error_reports_effect_honesty(m).await })).await;
}

#[tokio::test]
async fn c_104_receipt_read_rechecks_revoked_authorization_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { receipt_read_rechecks_revoked_authorization(m).await })).await;
}

#[tokio::test]
async fn c_104_receipt_read_rechecks_revoked_authorization_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { receipt_read_rechecks_revoked_authorization(m).await })).await;
}

#[tokio::test]
async fn c_073_reflection_reads_refuse_without_capability_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { reflection_reads_refuse_without_capability(m).await })).await;
}

#[tokio::test]
async fn c_073_reflection_reads_refuse_without_capability_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m| Box::pin(async move { reflection_reads_refuse_without_capability(m).await })).await;
}

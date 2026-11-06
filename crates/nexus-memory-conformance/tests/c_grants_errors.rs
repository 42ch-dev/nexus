//! C-matrix family J (N3): grant scope, revocation at the admission boundary,
//! concurrent writes, revoke/fence races, non-amplification and operator
//! bootstrap, on both real providers.
//!
//! The contract under test is the protocol's own: MEM-R003 (grants derive from
//! authenticated context and provider state), MEM-R007 (CAS), MEM-R017
//! (provider-enforced fences), MEM-R018 (revocation takes effect at the
//! admission of subsequent ops while admitted ops complete and record a
//! receipt), MEM-R020 (self-contained, non-amplifying grants) and §7.0.6
//! (operator provisioning of the first space and admin).
//!
//! Each scenario body takes `&Matrix` plus the runner-owned task/resource sets
//! so the `_local` leg runs the real `nexus_core` `LocalMemoryProvider` and the
//! `_remote` leg runs the independent durable fixture provider, both behind the
//! published `spoke-connect` memory service. Every spawned operation stays in
//! the runner's [`TaskSet`] (joined or aborted+joined never detached), every
//! raw session/observer stays in [`OwnedResources`] and is closed on failure as
//! well as success, and an admitted operation's result is bound to its actual
//! durable candidate/receipt rows.
//!
//! C-090's reflection/export surfaces and C-096's successful capability
//! remainders stay N4-R/N4-T.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use std::sync::Arc;

use futures_util::FutureExt as _;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use nexus_core::{
    CoreAccess, CoreHomeService, CoreOpenOptions, CoreService, MemoryProviderSelection,
};
use nexus_contracts::{CoreRegisterCreatorRequest, SetActiveWorkspaceRequest};
use spoke_connect::remote::RemoteAdapter;
use spoke_operations::spoke_schemas::Principal;
use spoke_operations::MemoryPort;

use support::{
    assert_no_receipt_disclosure, bootstrap, capture_body, context_scope, error_code, is_committed,
    principal, receipt_id, record_ids, result, retry_class, run_matrix_scenario, shared_scope,
    Actor, DurableState, Kind, Matrix, OwnedResources, TaskSet, CONTEXT, OTHER_CONTEXT, SUBJECT,
    TEXT,
};

const OUTSIDE_CONTEXT: &str = "ctx_c090_outside";

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

async fn pending_candidates(actor: &Actor<'_>) -> Vec<String> {
    candidates(
        &actor
            .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
            .await,
    )
}

/// One durable observed row by its label (`record:<id>`, `candidate:<id>`, …).
fn observed<'a>(state: &'a DurableState, label: &str) -> &'a [(String, String)] {
    state
        .rows
        .iter()
        .find(|(name, _)| name == label)
        .unwrap_or_else(|| panic!("the durable observation names {label}: {state:?}"))
        .1
        .as_slice()
}

/// One column of an observed row, as the database reported it.
fn column<'a>(row: &'a [(String, String)], name: &str) -> &'a str {
    row.iter()
        .find(|(column, _)| column == name)
        .unwrap_or_else(|| panic!("the durable row carries {name}: {row:?}"))
        .1
        .as_str()
}

/// The receipt row's `result_json` column, parsed as the provider's own JSON:
/// the durable result is compared as the actual structure, never by substring.
fn parsed_result_json(row: &[(String, String)]) -> Value {
    let raw = column(row, "result_json");
    let text = raw.strip_prefix("text:").unwrap_or(raw);
    serde_json::from_str(text).expect("the durable receipt result_json is valid JSON")
}

fn observed_records(state: &DurableState) -> Vec<(String, Vec<(String, String)>)> {
    state
        .rows
        .iter()
        .filter(|(label, _)| label.starts_with("record:"))
        .cloned()
        .collect()
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

/// The protocol receipt tuple of one committed mutation (§6.10): identity, op,
/// requester, a content-free result and the schema's digest shape.
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

/// One record update with owned inputs, so it can be a runner-owned task.
async fn update_with(
    client: Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    body: Value,
) -> Value {
    let mut body = body;
    body["space_id"] = json!(space_id);
    support::wire(&client.update_record(&principal, dto(body)).await)
}

async fn seed_record(admin: &Actor<'_>, key: &str, scope: &Value) -> String {
    let captured = admin.capture(capture_body(key, TEXT, scope)).await;
    let candidate = support::candidate_ids(&captured)[0].clone();
    let reviewed = admin
        .review_candidate(support::review_body(&candidate, 1, &format!("{key}-review")))
        .await;
    assert_eq!(result(&reviewed)["outcome"], json!("promoted"), "{reviewed}");
    record_ids(&reviewed)[0].clone()
}

async fn candidate_after(actor: &Actor<'_>, key: &str, scope: &Value) -> String {
    let captured = actor.capture(capture_body(key, TEXT, scope)).await;
    support::candidate_ids(&captured)[0].clone()
}

// ---------------------------------------------------------------------------
// J. Grants and concurrency
// ---------------------------------------------------------------------------

/// C-090: every base surface — by-id records and candidates, receipts, profiles,
/// reads and recall plus their counts/selection facets — enforces one grant's
/// scope; out-of-scope probes are indistinguishable from absence and change
/// nothing durable.
///
/// The two grants carry the SAME `records_read`/`recall` capabilities over
/// disjoint contexts A and B, so each context is independently readable and the
/// combined multi-context selection is the real non-union negative.
async fn grant_scope_covers_base_surfaces(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();

    let r_a = seed_record(&admin, "k-c090-a", &context_scope(CONTEXT)).await;
    let r_b = seed_record(&admin, "k-c090-b", &context_scope(OTHER_CONTEXT)).await;
    let r_out = seed_record(&admin, "k-c090-out", &context_scope(OUTSIDE_CONTEXT)).await;
    let r_shared = {
        let created = admin
            .create_record(json!({
                "subject_id": SUBJECT,
                "kind": "nexus.world_building",
                "content": { "text": TEXT },
                "scope": shared_scope(),
                "idempotency_key": "k-c090-shared",
            }))
            .await;
        result(&created)["record_id"]
            .as_str()
            .expect("record id")
            .to_owned()
    };
    let c_a = candidate_after(&admin, "k-c090-pending-a", &context_scope(CONTEXT)).await;
    let c_b = candidate_after(&admin, "k-c090-pending-b", &context_scope(OTHER_CONTEXT)).await;

    let a = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture", "review", "records_read", "recall"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c090-scope-a",
        }))
        .await;
    assert!(is_committed(&a), "grant A commits: {a}");
    let b = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["records_read", "recall"],
            "context_ids": [OTHER_CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c090-scope-b",
        }))
        .await;
    assert!(is_committed(&b), "grant B commits: {b}");

    let peer = matrix.peer();
    let observer = resources
        .open_observer(
            matrix,
            SUBJECT,
            &[r_a.clone(), r_b.clone(), r_out.clone(), r_shared.clone()],
            Some(&c_a),
            None,
            None,
        )
        .await;
    let before = resources.observe(observer).await;
    let captures_before = resources.receipt_ids_for(observer, "memory.capture").await;

    // 1. Independent same-capability reads: each context succeeds.
    assert_eq!(
        peer.get_record(&r_a).await["record"]["content"]["text"],
        json!(TEXT)
    );
    assert_eq!(
        peer.get_record(&r_b).await["record"]["content"]["text"],
        json!(TEXT)
    );

    // 2. The combined selection needs ONE grant covering both contexts.
    let union = peer
        .list_records(json!({
            "subject_id": SUBJECT,
            "selection": {
                "mode": "selected_contexts",
                "context_ids": [CONTEXT, OTHER_CONTEXT],
            },
        }))
        .await;
    assert_eq!(
        error_code(&union),
        "NOT_AUTHORIZED",
        "two disjoint same-capability grants never union: {union}"
    );
    assert_eq!(
        error_code(
            &peer
                .recall(json!({
                    "subject_id": SUBJECT,
                    "selection": {
                        "mode": "selected_contexts",
                        "context_ids": [CONTEXT, OTHER_CONTEXT],
                    },
                    "budget": { "max_items": 20 },
                }))
                .await
        ),
        "NOT_AUTHORIZED",
        "recall applies the same single-grant coverage"
    );
    assert_eq!(
        error_code(
            &peer
                .recall(json!({
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "budget": { "max_items": 20 },
                }))
                .await
        ),
        "NOT_AUTHORIZED",
        "a context-limited grant never obtains subject_all"
    );
    assert_eq!(
        error_code(
            &peer
                .recall(json!({
                    "subject_id": SUBJECT,
                    "selection": { "mode": "shared_only" },
                    "budget": { "max_items": 20 },
                }))
                .await
        ),
        "NOT_AUTHORIZED",
        "shared visibility needs the shared_access facet"
    );

    // 3. Real uncovered targets: shared (no shared_access), ungranted context,
    //    and an absent id all answer the same NOT_FOUND class.
    for record in [&r_shared, &r_out] {
        let refused = peer.get_record(record).await;
        assert_eq!(error_code(&refused), "NOT_FOUND", "{refused}");
    }
    assert_eq!(
        error_code(&peer.get_record("rec_c090_absent").await),
        "NOT_FOUND"
    );

    // 4. Candidate surface.
    assert_eq!(pending_candidates(&peer).await, vec![c_a.clone()]);
    assert_eq!(
        peer.get_candidate(&c_a).await["candidate"]["candidate_id"],
        json!(c_a)
    );
    assert_eq!(
        error_code(&peer.get_candidate(&c_b).await),
        "NOT_FOUND"
    );

    // 5. Selection counts: one per independently covered context.
    for (context, expected) in [(CONTEXT, &r_a), (OTHER_CONTEXT, &r_b)] {
        let page = peer
            .list_records(json!({
                "subject_id": SUBJECT,
                "selection": { "mode": "selected_contexts", "context_ids": [context] },
            }))
            .await;
        assert_eq!(page["total_authorized"], json!(1), "{page}");
        assert_eq!(page["records"][0]["record_id"], json!(expected));
        let recalled = peer
            .recall(json!({
                "subject_id": SUBJECT,
                "selection": { "mode": "selected_contexts", "context_ids": [context] },
                "budget": { "max_items": 20 },
            }))
            .await;
        assert_eq!(recalled["total_authorized"], json!(1), "{recalled}");
        assert_eq!(recalled["records"][0]["record_id"], json!(expected));
    }

    // 6. Profile documents are subject-level shared reads.
    assert_eq!(
        error_code(&peer.get_profile(json!({ "subject_id": SUBJECT })).await),
        "NOT_AUTHORIZED"
    );
    assert_eq!(
        admin.get_profile(json!({ "subject_id": SUBJECT })).await["profiles"],
        json!([])
    );

    // 7. Capture surface: covered commits; ungranted and shared are refused.
    let refused_capture = peer
        .capture(capture_body(
            "k-c090-outside",
            TEXT,
            &context_scope(OUTSIDE_CONTEXT),
        ))
        .await;
    assert_eq!(error_code(&refused_capture), "NOT_AUTHORIZED");
    let refused_shared = peer
        .capture(capture_body("k-c090-shared-capture", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&refused_shared), "NOT_AUTHORIZED");

    // 8. Durable no-effect: no capture receipt, no changed seeded row and no
    //    new visible candidate from the denied probes.
    let after = resources.observe(observer).await;
    assert_eq!(
        resources.receipt_ids_for(observer, "memory.capture").await,
        captures_before,
        "no denied probe produced a capture receipt"
    );
    assert_eq!(after.rows, before.rows, "no seeded row changed");
    assert_eq!(
        pending_candidates(&peer).await,
        vec![c_a.clone()],
        "no denied probe added a candidate the holder can see"
    );
    resources
        .close_observer(observer)
        .await
        .expect("the denied-probe observer closes");

    // 9. Allowed capture in A: bind the result candidate to its durable row and
    //    the receipt tuple before any later baseline.
    let allowed = peer
        .capture(capture_body("k-c090-allowed", TEXT, &context_scope(CONTEXT)))
        .await;
    assert_receipt_tuple(&allowed, "memory.capture", peer_id.as_str());
    let candidate = support::candidate_ids(&allowed)[0].clone();
    let receipt = receipt_id(&allowed).to_owned();
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
    assert_eq!(column(candidate_row, "subject_id"), format!("text:{SUBJECT}"));
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
    let durable_result = parsed_result_json(receipt_row);
    assert_eq!(
        durable_result,
        allowed["result"],
        "the durable receipt result is the returned committed result structure: {state:?}"
    );
    assert!(!column(receipt_row, "result_json").contains(TEXT));
    resources
        .close_observer(effect)
        .await
        .expect("the effect observer closes");

    // 10. Receipt surface: the holder reads its own; another principal's or
    //     out-of-coverage receipt is refused with no receipt disclosure.
    let own = peer.get_receipt(&receipt).await;
    assert_eq!(own["receipt"]["receipt_id"], json!(receipt));
    let foreign_capture = admin
        .capture(capture_body(
            "k-c090-admin-other",
            TEXT,
            &context_scope(OTHER_CONTEXT),
        ))
        .await;
    let foreign_receipt = receipt_id(&foreign_capture).to_owned();
    let refused = peer.get_receipt(&foreign_receipt).await;
    assert!(
        matches!(error_code(&refused), "NOT_FOUND" | "NOT_AUTHORIZED"),
        "{refused}"
    );
    assert_eq!(retry_class(&refused), "terminal");
    assert_no_receipt_disclosure(
        &refused,
        &foreign_capture,
        &json!({
            "op": "memory.receipts.get",
            "space_id": matrix.space_id(),
            "receipt_id": foreign_receipt,
        }),
    );
    assert_eq!(
        admin.get_receipt(&foreign_receipt).await["receipt"]["receipt_id"],
        json!(foreign_receipt)
    );

    // 11. No retained record was replaced or mutated by any probe.
    let final_observer = resources
        .open_observer(matrix, SUBJECT, &[r_a, r_b, r_out, r_shared], None, None, None)
        .await;
    let final_state = resources.observe(final_observer).await;
    assert_eq!(
        observed_records(&final_state),
        observed_records(&before),
        "the denied probes and the allowed capture changed no retained record"
    );
    resources
        .close_observer(final_observer)
        .await
        .expect("the final observer closes");
}

/// C-091: revoking a grant takes effect at the admission boundary. An operation
/// that already passed admission runs to its durable candidate/receipt while the
/// revoke commits; subsequent operations are refused and persist nothing.
async fn revoke_applies_at_admission_boundary(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();
    let issued = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture"],
            "idempotency_key": "k-c091-grant",
        }))
        .await;
    let grant_id = result(&issued)["grant_id"]
        .as_str()
        .expect("grant id")
        .to_owned();

    let mut capture_hold = matrix.park_next_capture();
    let capture = tasks.spawn(support::capture_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-c091-admitted", TEXT, &shared_scope()),
    ));
    capture_hold
        .wait_reached()
        .await
        .expect("the capture reached its admitted-but-uncommitted boundary");

    let revoke = match matrix.kind() {
        Kind::Local => {
            let mut arrival = matrix.park_next_admission();
            let revoke = tasks.spawn(revoke_from(
                matrix.admin_client(),
                matrix.admin_principal().clone(),
                matrix.space_id().to_owned(),
                grant_id.clone(),
                "k-c091-revoke",
            ));
            arrival
                .wait_reached()
                .await
                .expect("the revoke reached the local admission machinery while the capture was held");
            arrival.release().await;
            revoke
        }
        Kind::Remote => {
            // The fixture's existing handler-entry arm, now also consulted by
            // `revoke_grant` after authenticated typed admission and before its
            // writer transaction: a real arrival, not a spawn proxy.
            let mut arrival = matrix.park_next_target("memory.grants.revoke");
            let revoke = tasks.spawn(revoke_from(
                matrix.admin_client(),
                matrix.admin_principal().clone(),
                matrix.space_id().to_owned(),
                grant_id.clone(),
                "k-c091-revoke",
            ));
            arrival
                .wait_reached()
                .await
                .expect("the revoke reached the fixture handler while the capture was held");
            arrival.release().await;
            revoke
        }
    };
    capture_hold.release().await;

    let admitted = tasks
        .join(capture, "the admitted capture")
        .await
        .expect("the admitted capture settles");
    assert!(is_committed(&admitted), "the admitted op completes: {admitted}");
    let admitted_receipt = receipt_id(&admitted).to_owned();
    let admitted_candidate = support::candidate_ids(&admitted)[0].clone();
    assert_receipt_tuple(&admitted, "memory.capture", peer_id.as_str());

    // The admitted effect is the actual durable candidate, bound before any
    // later baseline can mask a missing effect.
    let effect = resources
        .open_observer(
            matrix,
            SUBJECT,
            &[],
            Some(&admitted_candidate),
            Some(&admitted_receipt),
            None,
        )
        .await;
    let effect_state = resources.observe(effect).await;
    let candidate_row = observed(&effect_state, &format!("candidate:{admitted_candidate}"));
    assert_eq!(column(candidate_row, "subject_id"), format!("text:{SUBJECT}"));
    assert_eq!(column(candidate_row, "capture_visibility"), "text:shared");
    assert_eq!(column(candidate_row, "status"), "text:pending");
    assert_eq!(column(candidate_row, "revision"), "int:1");
    assert!(column(candidate_row, "content_json").contains(TEXT));
    let receipt_row = observed(&effect_state, &format!("receipt:{admitted_receipt}"));
    assert_eq!(column(receipt_row, "status"), "text:committed");
    assert_eq!(column(receipt_row, "op"), "text:memory.capture");
    assert_eq!(
        column(receipt_row, "requester_principal_id"),
        format!("text:{}", peer_id.as_str())
    );
    assert_eq!(
        parsed_result_json(receipt_row),
        admitted["result"],
        "the durable receipt result is the returned committed capture result: {effect_state:?}"
    );
    assert!(!column(receipt_row, "result_json").contains(TEXT));
    resources
        .close_observer(effect)
        .await
        .expect("the effect observer closes");

    let revoked = tasks
        .join(revoke, "the revoke")
        .await
        .expect("the revoke settles");
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");
    assert_eq!(result(&revoked)["revision"], json!(2));
    let stored = admin.get_receipt(&admitted_receipt).await;
    assert_eq!(stored["receipt"]["receipt_id"], json!(admitted_receipt));
    assert_eq!(stored["receipt"]["status"], json!("committed"));

    // Subsequent admissions see the revocation: typed refusal, no new effect.
    let window = resources
        .open_observer(matrix, SUBJECT, &[], None, None, None)
        .await;
    let refusal_before = resources.observe(window).await;
    let captures_before = resources.receipt_ids_for(window, "memory.capture").await;
    let pending_before = pending_candidates(&admin).await;
    let after_revoke = matrix
        .peer()
        .capture(capture_body("k-c091-after", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&after_revoke), "NOT_AUTHORIZED", "{after_revoke}");
    assert_eq!(retry_class(&after_revoke), "terminal");
    let refused_list = matrix
        .peer()
        .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
        .await;
    assert_eq!(error_code(&refused_list), "NOT_AUTHORIZED", "{refused_list}");
    let refusal_after = resources.observe(window).await;
    assert_eq!(
        resources.receipt_ids_for(window, "memory.capture").await,
        captures_before,
        "the refused post-revocation capture produced no receipt"
    );
    assert_eq!(refusal_after.rows, refusal_before.rows);
    assert_eq!(
        pending_candidates(&admin).await,
        pending_before,
        "the refused capture added no candidate"
    );
    resources
        .close_observer(window)
        .await
        .expect("the refusal observer closes");
}

/// One revoke with owned inputs so it can be a runner-owned task.
async fn revoke_from(
    client: Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    grant_id: String,
    key: &'static str,
) -> Value {
    support::wire(
        &client
            .revoke_grant(
                &principal,
                dto(json!({
                    "space_id": space_id,
                    "grant_id": grant_id,
                    "expected_revision": 1,
                    "idempotency_key": key,
                })),
            )
            .await,
    )
}

/// C-092: two independent authenticated clients update one record from the same
/// CAS revision with complete, distinguishable content. Exactly one commits; the
/// other is `REVISION_CONFLICT`, and the final record is the winner's complete
/// write with every baseline field intact — never a merge or lost update.
async fn concurrent_record_update_has_one_winner(
    matrix: &Matrix,
    _resources: &mut OwnedResources,
    tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["records_write", "records_read"],
            "idempotency_key": "k-c092-write",
        }))
        .await;
    assert!(is_committed(&granted), "{granted}");

    let baseline_content = json!({
        "text": "the original line",
        "structured": { "writer": "baseline", "phase": 0, "tags": ["base"] },
    });
    let created = admin
        .create_record(json!({
            "subject_id": SUBJECT,
            "kind": "nexus.world_building",
            "content": baseline_content,
            "scope": shared_scope(),
            "idempotency_key": "k-c092-create",
        }))
        .await;
    let record_id = result(&created)["record_id"]
        .as_str()
        .expect("record id")
        .to_owned();
    let baseline = admin.get_record(&record_id).await["record"].clone();
    assert_eq!(baseline["revision"], json!(1));
    assert_eq!(baseline["content"]["structured"]["writer"], json!("baseline"));

    let admin_content = json!({
        "text": "written by the admin",
        "structured": { "writer": "admin", "phase": 1, "tags": ["a", "b"] },
    });
    let peer_content = json!({
        "text": "written by the second client",
        "structured": { "writer": "peer", "phase": 2, "tags": ["c"] },
    });

    let admin_update = tasks.spawn(update_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        matrix.space_id().to_owned(),
        json!({
            "record_id": record_id,
            "expected_revision": 1,
            "content": admin_content,
            "idempotency_key": "k-c092-admin",
        }),
    ));
    let peer_update = tasks.spawn(update_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        json!({
            "record_id": record_id,
            "expected_revision": 1,
            "content": peer_content,
            "idempotency_key": "k-c092-peer",
        }),
    ));
    // Both owned tasks are joined before either result is asserted.
    let admin_outcome = tasks.join(admin_update, "the admin update").await;
    let peer_outcome = tasks.join(peer_update, "the peer update").await;
    let admin_result = admin_outcome.expect("the admin update settles");
    let peer_result = peer_outcome.expect("the peer update settles");

    let committed = [&admin_result, &peer_result]
        .into_iter()
        .filter(|outcome| is_committed(outcome))
        .count();
    assert_eq!(
        committed, 1,
        "exactly one CAS write wins: {admin_result} {peer_result}"
    );
    let (loser, winner_content) = if is_committed(&admin_result) {
        (&peer_result, admin_content.clone())
    } else {
        (&admin_result, peer_content.clone())
    };
    assert_eq!(error_code(loser), "REVISION_CONFLICT", "{loser}");
    assert_eq!(retry_class(loser), "terminal");

    let final_record = admin.get_record(&record_id).await["record"].clone();
    let mut expected = baseline.clone();
    expected["content"] = winner_content.clone();
    expected["revision"] = json!(2);
    expected
        .as_object_mut()
        .expect("record object")
        .remove("updated_at");
    let mut actual = final_record.clone();
    actual
        .as_object_mut()
        .expect("record object")
        .remove("updated_at");
    assert_eq!(
        actual, expected,
        "the final record is exactly the winning write over the untouched baseline"
    );
    assert_ne!(
        final_record["content"]["structured"],
        if final_record["content"] == admin_content {
            peer_content["structured"].clone()
        } else {
            admin_content["structured"].clone()
        },
        "the losing structured content left no trace"
    );
}

/// C-093: a revocation or a subject fence concurrent with a write has exactly
/// one outcome decided by admission order, and the receipt/durable state agrees
/// with it. Revoke-first/fence-first admit the control first; write-first holds
/// the admitted write and proves the control reached the provider boundary
/// before the write is released.
async fn revoke_or_fence_race_has_one_outcome(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();
    let issued = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture"],
            "idempotency_key": "k-c093-grant",
        }))
        .await;
    let grant_id = result(&issued)["grant_id"]
        .as_str()
        .expect("grant id")
        .to_owned();

    // (a) Revoke admitted first.
    let mut write_hold = matrix.park_next_admission();
    let write = tasks.spawn(support::capture_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-c093-revoke-first", TEXT, &shared_scope()),
    ));
    write_hold
        .wait_reached()
        .await
        .expect("the write entered the admission machinery");
    let revoked = admin.revoke_grant(&grant_id, 1, "k-c093-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");
    let window = resources
        .open_observer(matrix, SUBJECT, &[], None, None, None)
        .await;
    let refusal_before = resources.observe(window).await;
    let captures_before = resources.receipt_ids_for(window, "memory.capture").await;
    let pending_before = pending_candidates(&admin).await;
    write_hold.release().await;
    let refused = tasks
        .join(write, "the revoke-first write")
        .await
        .expect("the revoke-first write settles");
    assert_eq!(error_code(&refused), "NOT_AUTHORIZED", "{refused}");
    assert_eq!(retry_class(&refused), "terminal");
    let refusal_after = resources.observe(window).await;
    assert_eq!(
        resources.receipt_ids_for(window, "memory.capture").await,
        captures_before,
        "the non-admitted write produced no receipt"
    );
    assert_eq!(refusal_after.rows, refusal_before.rows);
    assert_eq!(pending_candidates(&admin).await, pending_before);
    resources
        .close_observer(window)
        .await
        .expect("the revoke-first observer closes");

    // (b) Fence admitted first.
    let regranted = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture"],
            "idempotency_key": "k-c093-regrant",
        }))
        .await;
    assert!(is_committed(&regranted), "{regranted}");
    let mut write_hold = matrix.park_next_admission();
    let write = tasks.spawn(support::capture_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-c093-fence-first", TEXT, &shared_scope()),
    ));
    write_hold
        .wait_reached()
        .await
        .expect("the write entered the admission machinery");
    let fenced = admin
        .fence_subject(
            SUBJECT,
            json!({ "type": "subject" }),
            "fence",
            1,
            "k-c093-fence",
        )
        .await;
    assert_eq!(result(&fenced)["status"], json!("fenced"), "{fenced}");
    let window = resources
        .open_observer(matrix, SUBJECT, &[], None, None, None)
        .await;
    let refusal_before = resources.observe(window).await;
    let captures_before = resources.receipt_ids_for(window, "memory.capture").await;
    let pending_before = pending_candidates(&admin).await;
    write_hold.release().await;
    let refused = tasks
        .join(write, "the fence-first write")
        .await
        .expect("the fence-first write settles");
    assert_eq!(error_code(&refused), "SUBJECT_FENCED", "{refused}");
    let refusal_after = resources.observe(window).await;
    assert_eq!(
        resources.receipt_ids_for(window, "memory.capture").await,
        captures_before,
        "the post-fence write produced no receipt"
    );
    assert_eq!(refusal_after.rows, refusal_before.rows);
    assert_eq!(pending_candidates(&admin).await, pending_before);
    resources
        .close_observer(window)
        .await
        .expect("the fence-first observer closes");

    // (c) Write admitted first, control arriving while it is held.
    let unfenced = admin
        .fence_subject(
            SUBJECT,
            json!({ "type": "subject" }),
            "unfence",
            2,
            "k-c093-unfence",
        )
        .await;
    assert_eq!(result(&unfenced)["status"], json!("active"), "{unfenced}");

    let mut capture_hold = matrix.park_next_capture();
    let write = tasks.spawn(support::capture_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-c093-write-first", TEXT, &shared_scope()),
    ));
    capture_hold
        .wait_reached()
        .await
        .expect("the write reached its admitted-but-uncommitted boundary");
    let fence = match matrix.kind() {
        Kind::Local => {
            let mut arrival = matrix.park_next_admission();
            let fence = tasks.spawn(fence_from(
                matrix.admin_client(),
                matrix.admin_principal().clone(),
                matrix.space_id().to_owned(),
                3,
                "k-c093-fence-late",
            ));
            arrival
                .wait_reached()
                .await
                .expect("the fence reached the local admission machinery while the write was held");
            arrival.release().await;
            fence
        }
        Kind::Remote => {
            let mut arrival = matrix.park_next_target("memory.subjects.fence");
            let fence = tasks.spawn(fence_from(
                matrix.admin_client(),
                matrix.admin_principal().clone(),
                matrix.space_id().to_owned(),
                3,
                "k-c093-fence-late",
            ));
            arrival
                .wait_reached()
                .await
                .expect("the fence reached the fixture handler while the write was held");
            arrival.release().await;
            fence
        }
    };
    capture_hold.release().await;
    let admitted = tasks
        .join(write, "the admitted write")
        .await
        .expect("the admitted write settles");
    let fenced_late = tasks
        .join(fence, "the late fence")
        .await
        .expect("the late fence settles");
    assert!(is_committed(&admitted), "the admitted write completes: {admitted}");
    assert_receipt_tuple(&admitted, "memory.capture", peer_id.as_str());
    let candidate = support::candidate_ids(&admitted)[0].clone();
    let receipt = receipt_id(&admitted).to_owned();
    assert_eq!(result(&fenced_late)["status"], json!("fenced"), "{fenced_late}");
    assert_eq!(result(&fenced_late)["revision"], json!(4));

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
    assert_eq!(column(candidate_row, "subject_id"), format!("text:{SUBJECT}"));
    assert_eq!(column(candidate_row, "capture_visibility"), "text:shared");
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
    assert_eq!(
        parsed_result_json(receipt_row),
        admitted["result"],
        "the durable receipt result is the returned committed result structure: {state:?}"
    );
    assert!(!column(receipt_row, "result_json").contains(TEXT));
    resources
        .close_observer(effect)
        .await
        .expect("the race effect observer closes");

    let window = resources
        .open_observer(matrix, SUBJECT, &[], None, None, None)
        .await;
    let refusal_before = resources.observe(window).await;
    let captures_before = resources.receipt_ids_for(window, "memory.capture").await;
    let pending_before = pending_candidates(&admin).await;
    let afterwards = matrix
        .peer()
        .capture(capture_body("k-c093-after-fence", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&afterwards), "SUBJECT_FENCED", "{afterwards}");
    let refusal_after = resources.observe(window).await;
    assert_eq!(
        resources.receipt_ids_for(window, "memory.capture").await,
        captures_before,
        "the refusals after the race produced no receipt"
    );
    assert_eq!(refusal_after.rows, refusal_before.rows);
    assert_eq!(pending_candidates(&admin).await, pending_before);
    resources
        .close_observer(window)
        .await
        .expect("the post-race observer closes");
}

/// One subject fence with owned inputs, so it can be a runner-owned task.
async fn fence_from(
    client: Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    revision: u64,
    key: &'static str,
) -> Value {
    support::wire(
        &client
            .fence_subject(
                &principal,
                dto(json!({
                    "space_id": space_id,
                    "subject_id": SUBJECT,
                    "target": { "type": "subject" },
                    "action": "fence",
                    "expected_revision": revision,
                    "idempotency_key": key,
                })),
            )
            .await,
    )
}

/// C-095: a scoped admin cannot amplify its own coverage. The context-superset
/// and shared-access probes carry the SAME valid earlier expiry as the
/// legitimate positive; the longer-expiry probe is isolated. Refusals leave the
/// full effective grant set unchanged.
async fn scoped_admin_cannot_amplify_grant(
    matrix: &Matrix,
    _resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    bootstrap(matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();
    let third = "prn_c095_third";

    let granter_expiry = (chrono::Utc::now() + chrono::Duration::hours(2))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let scoped = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["admin"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "expires_at": granter_expiry,
            "idempotency_key": "k-c095-admin",
        }))
        .await;
    assert!(is_committed(&scoped), "the scoped admin grant commits: {scoped}");
    let peer = matrix.peer();
    let grants_before = admin.list_grants(json!({})).await["grants"].clone();
    let earlier = (chrono::Utc::now() + chrono::Duration::minutes(30))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let superset = peer
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": third,
            "capabilities": ["capture"],
            "context_ids": [CONTEXT, OTHER_CONTEXT],
            "shared_access": false,
            "expires_at": earlier,
            "idempotency_key": "k-c095-superset",
        }))
        .await;
    assert_eq!(error_code(&superset), "NOT_AUTHORIZED", "{superset}");
    assert_eq!(retry_class(&superset), "terminal");

    let upgraded = peer
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": third,
            "capabilities": ["capture"],
            "context_ids": [CONTEXT],
            "shared_access": true,
            "expires_at": earlier,
            "idempotency_key": "k-c095-shared",
        }))
        .await;
    assert_eq!(error_code(&upgraded), "NOT_AUTHORIZED", "{upgraded}");

    let longer = (chrono::Utc::now() + chrono::Duration::hours(6))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let prolonged = peer
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": third,
            "capabilities": ["capture"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "expires_at": longer,
            "idempotency_key": "k-c095-longer",
        }))
        .await;
    assert_eq!(error_code(&prolonged), "NOT_AUTHORIZED", "{prolonged}");

    assert_eq!(
        admin.list_grants(json!({})).await["grants"],
        grants_before,
        "the refused amplifications changed no effective grant"
    );

    let legitimate = peer
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": third,
            "capabilities": ["capture"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "expires_at": earlier,
            "idempotency_key": "k-c095-legit",
        }))
        .await;
    assert!(is_committed(&legitimate), "{legitimate}");
    let third_grants = admin
        .list_grants(json!({ "principal_id": third }))
        .await["grants"]
        .as_array()
        .expect("grant page")
        .clone();
    assert_eq!(third_grants.len(), 1, "exactly the legitimate grant: {third_grants:?}");
    assert_eq!(third_grants[0]["capabilities"], json!(["capture"]));
    assert_eq!(third_grants[0]["context_ids"], json!([CONTEXT]));
    assert_eq!(third_grants[0]["shared_access"], json!(false));

    let self_view = peer.list_grants(json!({})).await;
    assert!(
        self_view["grants"]
            .as_array()
            .expect("grant page")
            .iter()
            .any(|grant| {
                grant["principal_id"] == json!(peer_id)
                    && grant["capabilities"] == json!(["admin"])
                    && grant["context_ids"] == json!([CONTEXT])
            }),
        "the scoped admin's self-view is callable: {self_view}"
    );
}

/// C-096: a fresh provider's operator provisioning creates the initial space and
/// administrator without any protocol grant cycle, and both are discoverable
/// through `memory.describe` / `memory.spaces.list`. No protocol space-creation
/// op exists.
async fn operator_bootstrap_is_callable_and_visible(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    let admin = matrix.admin();
    let described = admin.describe().await;
    assert_eq!(described["capabilities"], json!(["spoke-memory"]), "{described}");
    assert_eq!(described["required_profile_kinds"], json!([]));
    let spaces = admin.list_spaces().await;
    let rows = spaces["spaces"].as_array().expect("space page");
    assert_eq!(rows.len(), 1, "exactly the provisioned space: {spaces}");
    assert_eq!(rows[0]["space_id"], json!(matrix.space_id()));
    assert_eq!(rows[0]["status"], json!("active"));
    assert_eq!(rows[0]["authority_generation"], json!(1));
    let grants = admin.list_grants(json!({})).await;
    assert!(
        grants["grants"]
            .as_array()
            .expect("grant page")
            .iter()
            .any(|grant| {
                grant["principal_id"] == json!(matrix.admin_principal().principal_id)
                    && grant["capabilities"] == json!(["admin"])
            }),
        "the operator admin grant is visible: {grants}"
    );

    // No protocol v1 space-creation op was invented; the raw session is
    // runner-owned and closed by the same settle path that owns the matrix.
    let raw = resources.open_raw_admin(matrix).await;
    let invented = resources
        .invoke(raw, "memory.spaces.create", &json!({ "space_id": matrix.space_id() }))
        .await;
    assert_eq!(
        invented["error"]["code"],
        json!("op_unsupported"),
        "space creation is not a protocol op: {invented}"
    );
    resources
        .close_raw(raw)
        .await
        .expect("the raw session settles");
    assert_eq!(
        matrix.admin().list_spaces().await["spaces"]
            .as_array()
            .expect("space page")
            .len(),
        1
    );
}

/// C-096 (`_local` only): the real operator `CoreService` open provisions the
/// initial space and admin grant; its close must report a confirmed cleanup.
/// The served-surface half runs on a fresh local matrix.
async fn local_operator_bootstrap_through_core() {
    let home = tempfile::TempDir::new().expect("temporary Core home");
    let user_home = home.path().to_path_buf();
    let home_service = CoreHomeService::open(user_home.clone()).expect("open the core home");
    let creator_id = home_service
        .register_creator(CoreRegisterCreatorRequest {
            display_name: Some("C096 Operator".parse().expect("valid display name")),
            platform_creator_id: None,
        })
        .await
        .expect("register the operator creator")
        .creator_id;
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home,
        &creator_id,
        "default",
    ))
    .expect("workspace dir");
    home_service
        .select_workspace(SetActiveWorkspaceRequest {
            creator_id: None,
            workspace_slug: "default".to_owned(),
        })
        .await
        .expect("select the workspace");

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::EngineOwner,
        memory: Some(MemoryProviderSelection::Local),
    })
    .await
    .expect("open Core over the disposable home");

    let scenario = std::panic::AssertUnwindSafe(async {
        let provider = core
            .memory_provider_for_test()
            .expect("the open pinned the local provider");
        let operator = principal(&creator_id);
        let space_id = provider.space_id().to_owned();
        assert!(!space_id.is_empty(), "Core provisioning created the space");
        let described = support::wire(&provider.describe(&operator, dto(json!({}))).await);
        assert_eq!(described["capabilities"], json!(["spoke-memory"]), "{described}");
        let spaces = support::wire(&provider.list_spaces(&operator, dto(json!({}))).await);
        let rows = spaces["spaces"].as_array().expect("space page");
        assert_eq!(rows.len(), 1, "the Core open provisioned one space: {spaces}");
        assert_eq!(rows[0]["space_id"], json!(space_id));
        assert_eq!(rows[0]["status"], json!("active"));
        let grants = support::wire(
            &provider
                .list_grants(&operator, dto(json!({ "space_id": space_id })))
                .await,
        );
        assert!(
            grants["grants"].as_array().expect("grant page").iter().any(|grant| {
                grant["principal_id"] == json!(creator_id)
                    && grant["capabilities"] == json!(["admin"])
            }),
            "the operator admin grant exists without a protocol issuance: {grants}"
        );
        drop(provider);
    })
    .catch_unwind()
    .await;
    let cleanup = support::close_core_bounded(&core).await;
    drop(core);
    match (scenario, cleanup) {
        (Ok(()), Ok(report)) => {
            assert!(report.cleanup_confirmed, "the Core close reports a confirmed cleanup");
            assert!(report.pending_operations.is_empty(), "{report:?}");
            eprintln!("C-096 local: Core close report state={} confirmed={}", report.state, report.cleanup_confirmed);
        }
        (Ok(()), Err(error)) => {
            panic!("the operator Core close failed after the bootstrap evidence: {error}")
        }
        (Err(payload), Ok(_)) => std::panic::resume_unwind(payload),
        (Err(payload), Err(error)) => {
            eprintln!("the operator Core close also failed: {error}");
            std::panic::resume_unwind(payload);
        }
    }
}

/// The served-surface half of C-096 on a fresh local matrix.
async fn local_served_surface_refusal(
    matrix: &Matrix,
    resources: &mut OwnedResources,
    _tasks: TaskSet,
) {
    let raw = resources.open_raw_admin(matrix).await;
    let invented = resources
        .invoke(raw, "memory.spaces.create", &json!({ "space_id": matrix.space_id() }))
        .await;
    assert_eq!(invented["error"]["code"], json!("op_unsupported"), "{invented}");
    resources
        .close_raw(raw)
        .await
        .expect("the raw session settles");
    assert_eq!(
        matrix.admin().list_spaces().await["spaces"]
            .as_array()
            .expect("space page")
            .len(),
        1
    );
}

#[tokio::test]
async fn c_090_grant_scope_covers_base_surfaces_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { grant_scope_covers_base_surfaces(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_090_grant_scope_covers_base_surfaces_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { grant_scope_covers_base_surfaces(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_091_revoke_applies_at_admission_boundary_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { revoke_applies_at_admission_boundary(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_091_revoke_applies_at_admission_boundary_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { revoke_applies_at_admission_boundary(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_092_concurrent_record_update_has_one_winner_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { concurrent_record_update_has_one_winner(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_092_concurrent_record_update_has_one_winner_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { concurrent_record_update_has_one_winner(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_093_revoke_or_fence_race_has_one_outcome_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { revoke_or_fence_race_has_one_outcome(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_093_revoke_or_fence_race_has_one_outcome_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { revoke_or_fence_race_has_one_outcome(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_095_scoped_admin_cannot_amplify_grant_local() {
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { scoped_admin_cannot_amplify_grant(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_095_scoped_admin_cannot_amplify_grant_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { scoped_admin_cannot_amplify_grant(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_096_operator_bootstrap_is_callable_and_visible_local() {
    support::prove_failed_endpoint_settles().await;
    support::prove_raw_setup_failure_settles().await;
    local_operator_bootstrap_through_core().await;
    let matrix = Matrix::local().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { local_served_surface_refusal(m, r, t).await })
    })
    .await;
}

#[tokio::test]
async fn c_096_operator_bootstrap_is_callable_and_visible_remote() {
    let matrix = Matrix::remote().await;
    run_matrix_scenario(matrix, |m, r, t| {
        Box::pin(async move { operator_bootstrap_is_callable_and_visible(m, r, t).await })
    })
    .await;
}

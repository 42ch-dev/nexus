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
//! Each scenario body takes a [`Matrix`] so the `_local` leg runs the real
//! `nexus_core` `LocalMemoryProvider` and the `_remote` leg runs the
//! independent durable fixture provider, both behind the published
//! `spoke-connect` memory service. Assertions read the provider's answers,
//! refusals and receipts; every wait is bounded by the harness, and every
//! "no effect" claim is paired with a positive control that the same state
//! projection can detect a real change.
//!
//! C-090's reflection/export surfaces and C-096's protocol-side bootstrap are
//! not part of N3 here: reflection and transfer remain N4-R/N4-T.
//!
//! C-096's `_local` leg deliberately does not use [`Matrix`]: the row is about
//! the *operator* `CoreService` open provisioning the initial space and admin
//! grant, so it opens the real Core over a disposable home and reads the
//! provider that open pinned. The served-surface half of the row (no invented
//! space-creation op) is proved on a fresh local matrix in the same test.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use std::sync::Arc;

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
    bootstrap, capture_body, context_scope, error_code, is_committed, principal, receipt_id,
    result, retry_class, shared_scope, Actor, Matrix, CONTEXT, OTHER_CONTEXT, SUBJECT, TEXT,
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

async fn pending_candidates(actor: &Actor<'_>) -> Vec<String> {
    candidates(
        &actor
            .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
            .await,
    )
}

/// One record update with owned inputs, so the future can be raced.
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

// ---------------------------------------------------------------------------
// J. Grants and concurrency
// ---------------------------------------------------------------------------

/// C-090: every base surface — by-id records and candidates, receipts, profiles,
/// reads and recall plus their counts/selection facets — enforces one grant's
/// scope; out-of-scope probes are indistinguishable from absence and change
/// nothing, and two narrower grants never union into coverage neither has.
///
/// The reflection and export surfaces the historical row also named stay
/// N4-R/N4-T (no N3 success is claimed here).
async fn grant_scope_covers_base_surfaces(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();

    // Seed three retained records, one per placement, and two still-pending
    // candidates in the covered and the foreign context.
    let seeded: [(&str, Value); 3] = [
        ("k-c090-context", context_scope(CONTEXT)),
        ("k-c090-other", context_scope(OTHER_CONTEXT)),
        ("k-c090-shared", shared_scope()),
    ];
    let mut record_ids = Vec::new();
    for (key, scope) in &seeded {
        let captured = admin.capture(capture_body(*key, TEXT, scope)).await;
        let candidate = support::candidate_ids(&captured)[0].clone();
        let reviewed = admin
            .review_candidate(support::review_body(&candidate, 1, &format!("{key}-review")))
            .await;
        assert_eq!(result(&reviewed)["outcome"], json!("promoted"), "{reviewed}");
        record_ids.push(support::record_ids(&reviewed)[0].clone());
    }
    let covered_record = record_ids[0].clone();
    let foreign_record = record_ids[1].clone();
    let shared_record = record_ids[2].clone();
    let pending_covered = candidate_after(
        &admin,
        "k-c090-pending-covered",
        &context_scope(CONTEXT),
    )
    .await;
    let pending_foreign = candidate_after(
        &admin,
        "k-c090-pending-foreign",
        &context_scope(OTHER_CONTEXT),
    )
    .await;

    // The admin's own unrestricted grant is the positive projection: all three
    // records and both candidates are visible to it.
    assert_eq!(
        admin
            .list_records(json!({
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
            }))
            .await["total_authorized"],
        json!(3),
        "the positive control sees the whole seeded set"
    );

    // The scoped holder: one grant covering CONTEXT for every base capability.
    let scoped = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": [
                "capture", "review", "records_read", "records_write", "recall",
            ],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c090-scope",
        }))
        .await;
    assert!(is_committed(&scoped), "the scoped grant commits: {scoped}");
    // A second, disjoint grant: `OTHER_CONTEXT` capture only. It must never
    // combine with the first to reach a selection neither covers.
    let disjoint = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture"],
            "context_ids": [OTHER_CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c090-disjoint",
        }))
        .await;
    assert!(is_committed(&disjoint), "{disjoint}");

    let peer = matrix.peer();

    // 1. By-id record reads: covered placement succeeds, the other placements
    //    answer the same NOT_FOUND as an absent id.
    let covered_view = peer.get_record(&covered_record).await;
    assert_eq!(
        covered_view["record"]["content"]["text"],
        json!(TEXT),
        "{covered_view}"
    );
    let foreign = peer.get_record(&foreign_record).await;
    let shared = peer.get_record(&shared_record).await;
    let absent = peer.get_record("rec_c090_absent").await;
    assert_eq!(error_code(&foreign), "NOT_FOUND", "{foreign}");
    assert_eq!(error_code(&shared), "NOT_FOUND", "{shared}");
    assert_eq!(error_code(&absent), "NOT_FOUND", "{absent}");
    assert_eq!(
        retry_class(&foreign),
        retry_class(&absent),
        "an out-of-scope record is indistinguishable from a missing one"
    );

    // 2. Candidate surface: the covered pending candidate is readable and
    //    listed; the foreign one is not.
    let listed = pending_candidates(&peer).await;
    assert_eq!(listed, vec![pending_covered.clone()], "{listed:?}");
    assert_eq!(
        peer.get_candidate(&pending_covered).await["candidate"]["candidate_id"],
        json!(pending_covered)
    );
    assert_eq!(
        error_code(&peer.get_candidate(&pending_foreign).await),
        "NOT_FOUND",
        "an out-of-scope candidate is absent"
    );

    // 3. Read selection and its exposed count: only the covered context is
    //    authorized, so the count is one, and naming both contexts in one
    //    selection is refused because no single grant covers them.
    let scoped_read = peer
        .list_records(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "selected_contexts", "context_ids": [CONTEXT] },
        }))
        .await;
    assert_eq!(scoped_read["total_authorized"], json!(1), "{scoped_read}");
    assert_eq!(
        scoped_read["records"][0]["record_id"],
        json!(covered_record)
    );
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
        "two narrower grants never union: {union}"
    );

    // 4. Recall over the covered selection and its count; wider selections
    //    (subject_all, shared) are refused rather than filtered silently.
    let recall = peer
        .recall(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "selected_contexts", "context_ids": [CONTEXT] },
            "budget": { "max_items": 20 },
        }))
        .await;
    assert_eq!(recall["total_authorized"], json!(1), "{recall}");
    assert_eq!(recall["records"][0]["record_id"], json!(covered_record));
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

    // 5. Profile reads are subject-level shared documents: the context-limited
    //    holder is refused, while the unrestricted admin reads the honest-empty
    //    document set.
    assert_eq!(
        error_code(&peer.get_profile(json!({ "subject_id": SUBJECT })).await),
        "NOT_AUTHORIZED",
        "a context-restricted grant does not reach shared profile documents"
    );
    assert_eq!(
        admin.get_profile(json!({ "subject_id": SUBJECT })).await["profiles"],
        json!([]),
        "the unrestricted positive control reads the honest-empty profile set"
    );

    // 6. Capture surface: the covered placement commits; a placement no grant
    //    covers is refused with zero candidates.
    let before = pending_candidates(&admin).await.len();
    let allowed = peer
        .capture(capture_body("k-c090-peer-capture", TEXT, &context_scope(CONTEXT)))
        .await;
    assert!(is_committed(&allowed), "{allowed}");
    assert_eq!(pending_candidates(&admin).await.len(), before + 1);
    let refused = peer
        .capture(capture_body(
            "k-c090-outside",
            TEXT,
            &context_scope("ctx_c090_outside"),
        ))
        .await;
    assert_eq!(error_code(&refused), "NOT_AUTHORIZED", "{refused}");
    assert_eq!(
        pending_candidates(&admin).await.len(),
        before + 1,
        "the refused capture persisted nothing"
    );

    // 7. Receipt surface: the holder reads its own covered receipt, but another
    //    principal's or an out-of-scope receipt is not disclosed.
    let own_receipt = receipt_id(&allowed).to_owned();
    let own_view = peer.get_receipt(&own_receipt).await;
    assert_eq!(own_view["receipt"]["receipt_id"], json!(own_receipt));
    let foreign_capture = admin
        .capture(capture_body(
            "k-c090-admin-other",
            TEXT,
            &context_scope(OTHER_CONTEXT),
        ))
        .await;
    let foreign_receipt = receipt_id(&foreign_capture).to_owned();
    let foreign_view = peer.get_receipt(&foreign_receipt).await;
    assert!(
        matches!(error_code(&foreign_view), "NOT_FOUND" | "NOT_AUTHORIZED"),
        "a receipt whose target the current grant does not cover is refused: {foreign_view}"
    );
    assert_eq!(retry_class(&foreign_view), "terminal");
    assert!(
        foreign_view["error"].get("receipt").is_none(),
        "the refusal discloses no receipt body: {foreign_view}"
    );
    assert_eq!(
        admin.get_receipt(&foreign_receipt).await["receipt"]["receipt_id"],
        json!(foreign_receipt),
        "the positive control reads the same receipt as its requester"
    );

    // The denied probes changed no retained record: the unrestricted projection
    // still sees exactly the seeded three.
    assert_eq!(
        admin
            .list_records(json!({
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
            }))
            .await["total_authorized"],
        json!(3)
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0)
    );
    matrix.shutdown().await;
}

/// Capture one candidate through the admin and return its id.
async fn candidate_after(actor: &Actor<'_>, key: &str, scope: &Value) -> String {
    let captured = actor.capture(capture_body(key, TEXT, scope)).await;
    support::candidate_ids(&captured)[0].clone()
}

/// C-091: revoking a grant takes effect at the admission boundary. An operation
/// that already passed admission runs to its durable receipt while the revoke
/// commits; subsequent operations are refused and persist nothing.
async fn revoke_applies_at_admission_boundary(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
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

    // Park the capture AFTER its admission check (authorization passed) and
    // before its commit, then issue the real revocation while it is held.
    let mut hold = matrix.park_next_capture();
    let capture = tokio::spawn(support::capture_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-c091-admitted", TEXT, &shared_scope()),
    ));
    hold.wait_reached()
        .await
        .expect("the capture reached the admitted-but-uncommitted boundary");

    let revoke = tokio::spawn(revoke_from(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        matrix.space_id().to_owned(),
        grant_id.clone(),
        "k-c091-revoke",
    ));
    hold.release().await;

    let admitted = capture.await.expect("the admitted capture completes");
    assert!(is_committed(&admitted), "the admitted op completes: {admitted}");
    let admitted_receipt = receipt_id(&admitted).to_owned();
    assert_eq!(
        support::candidate_ids(&admitted).len(),
        1,
        "the admitted op committed its effect exactly once"
    );

    let revoked = revoke.await.expect("the revoke completes");
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");
    assert_eq!(result(&revoked)["revision"], json!(2));

    // The admitted operation's receipt is durable and content-free, readable by
    // its requester and by the admin.
    let stored = admin.get_receipt(&admitted_receipt).await;
    assert_eq!(stored["receipt"]["receipt_id"], json!(admitted_receipt));
    assert_eq!(stored["receipt"]["status"], json!("committed"));
    assert!(
        stored["receipt"]["result"].get("content").is_none(),
        "the receipt is content-free: {stored}"
    );

    // Subsequent admissions see the revocation: typed refusal, no new effect.
    let before = pending_candidates(&admin).await.len();
    let after_revoke = matrix
        .peer()
        .capture(capture_body("k-c091-after", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&after_revoke), "NOT_AUTHORIZED", "{after_revoke}");
    assert_eq!(retry_class(&after_revoke), "terminal");
    assert_eq!(
        pending_candidates(&admin).await.len(),
        before,
        "the post-revocation capture persisted nothing"
    );
    let revoked_list = matrix
        .peer()
        .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
        .await;
    assert_eq!(
        error_code(&revoked_list),
        "NOT_AUTHORIZED",
        "the revoked principal cannot list candidates: {revoked_list}"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0)
    );
    matrix.shutdown().await;
}

/// One revoke with owned inputs so it can be issued while another op is held.
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
/// CAS revision. Exactly one commits; the other is `REVISION_CONFLICT`, and the
/// final record is exactly the winner's write — never a merge or lost update.
async fn concurrent_record_update_has_one_winner(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
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

    let created = admin
        .create_record(json!({
            "subject_id": SUBJECT,
            "kind": "nexus.world_building",
            "content": { "text": "the original line" },
            "scope": shared_scope(),
            "idempotency_key": "k-c092-create",
        }))
        .await;
    let record_id = result(&created)["record_id"]
        .as_str()
        .expect("record id")
        .to_owned();

    // Positive control: the state projection can see the pre-race revision and
    // content, and the two writes are distinguishable strings.
    let before = admin.get_record(&record_id).await;
    assert_eq!(before["record"]["revision"], json!(1), "{before}");
    assert_eq!(before["record"]["content"]["text"], json!("the original line"));
    let admin_text = "written by the admin";
    let peer_text = "written by the second client";

    let admin_update = tokio::spawn(update_with(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        matrix.space_id().to_owned(),
        json!({
            "record_id": record_id,
            "expected_revision": 1,
            "content": { "text": admin_text },
            "idempotency_key": "k-c092-admin",
        }),
    ));
    let peer_update = tokio::spawn(update_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        json!({
            "record_id": record_id,
            "expected_revision": 1,
            "content": { "text": peer_text },
            "idempotency_key": "k-c092-peer",
        }),
    ));
    let admin_result = admin_update.await.expect("the admin update completes");
    let peer_result = peer_update.await.expect("the peer update completes");

    let committed = [&admin_result, &peer_result]
        .into_iter()
        .filter(|outcome| is_committed(outcome))
        .count();
    assert_eq!(committed, 1, "exactly one CAS write wins: {admin_result} {peer_result}");
    let loser = if is_committed(&admin_result) {
        &peer_result
    } else {
        &admin_result
    };
    assert_eq!(error_code(loser), "REVISION_CONFLICT", "{loser}");
    assert_eq!(retry_class(loser), "terminal");

    let winner_text = if is_committed(&admin_result) {
        admin_text
    } else {
        peer_text
    };
    let final_view = admin.get_record(&record_id).await;
    assert_eq!(final_view["record"]["revision"], json!(2), "{final_view}");
    assert_eq!(
        final_view["record"]["content"]["text"],
        json!(winner_text),
        "the final record is exactly the winning write, never a merge"
    );
    let loser_text = if winner_text == admin_text {
        peer_text
    } else {
        admin_text
    };
    assert_ne!(
        final_view["record"]["content"]["text"],
        json!(loser_text),
        "the losing write left no trace"
    );
    matrix.shutdown().await;
}

/// C-093: a revocation or a subject fence concurrent with a write has exactly
/// one outcome decided by admission order, and the receipt/effect state agrees
/// with it. The two directions are established through the provider's real
/// admission seams, never a sleep or a "not finished yet" proxy.
async fn revoke_or_fence_race_has_one_outcome(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
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

    // (a) Revoke admitted first: the write is parked before its admission
    //     decision, the revoke commits, and the released write is refused with
    //     no effect and no receipt.
    let mut hold = matrix.park_next_admission();
    let write = tokio::spawn(support::capture_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-c093-revoke-first", TEXT, &shared_scope()),
    ));
    hold.wait_reached()
        .await
        .expect("the write entered the admission machinery");
    let revoked = admin.revoke_grant(&grant_id, 1, "k-c093-revoke").await;
    assert_eq!(result(&revoked)["revoked"], json!(true), "{revoked}");
    hold.release().await;
    let refused = write.await.expect("the write completes");
    assert_eq!(error_code(&refused), "NOT_AUTHORIZED", "{refused}");
    assert_eq!(retry_class(&refused), "terminal");
    assert_eq!(
        pending_candidates(&admin).await.len(),
        0,
        "the non-admitted write persisted nothing"
    );
    assert_eq!(
        admin.list_spaces().await["spaces"][0]["pending_receipts"],
        json!(0),
        "a refused write creates no receipt"
    );

    // (b) Fence admitted first: re-grant capture, park the write before its
    //     admission, commit the real subject fence, then release. Admission
    //     refuses on the fenced subject with zero effect.
    let regranted = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["capture"],
            "idempotency_key": "k-c093-regrant",
        }))
        .await;
    assert!(is_committed(&regranted), "{regranted}");
    let mut hold = matrix.park_next_admission();
    let write = tokio::spawn(support::capture_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-c093-fence-first", TEXT, &shared_scope()),
    ));
    hold.wait_reached()
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
    assert_eq!(result(&fenced)["revision"], json!(2));
    hold.release().await;
    let refused = write.await.expect("the write completes");
    assert_eq!(error_code(&refused), "SUBJECT_FENCED", "{refused}");
    assert_eq!(retry_class(&refused), "terminal");
    assert_eq!(
        pending_candidates(&admin).await.len(),
        0,
        "the post-fence write persisted nothing"
    );

    // (c) Write admitted first: the opposite direction. Unfence, park the write
    //     after its authorization, commit the fence while it is held, then
    //     release: the admitted write completes with a durable receipt and the
    //     fence commits after it. A later write is refused.
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
    assert_eq!(result(&unfenced)["revision"], json!(3));

    let mut hold = matrix.park_next_capture();
    let write = tokio::spawn(support::capture_with(
        matrix.peer_client(),
        matrix.peer_principal().clone(),
        matrix.space_id().to_owned(),
        capture_body("k-c093-write-first", TEXT, &shared_scope()),
    ));
    hold.wait_reached()
        .await
        .expect("the write reached its admitted-but-uncommitted boundary");
    let fence = tokio::spawn(fence_from(
        matrix.admin_client(),
        matrix.admin_principal().clone(),
        matrix.space_id().to_owned(),
        3,
        "k-c093-fence-late",
    ));
    hold.release().await;
    let admitted = write.await.expect("the admitted write completes");
    assert!(is_committed(&admitted), "the admitted write completes: {admitted}");
    assert_eq!(support::candidate_ids(&admitted).len(), 1);
    let fenced_late = fence.await.expect("the late fence completes");
    assert_eq!(result(&fenced_late)["status"], json!("fenced"), "{fenced_late}");
    assert_eq!(result(&fenced_late)["revision"], json!(4));

    // The admitted write's receipt is durable; the receipt channel agrees with
    // the effect.
    let stored = admin.get_receipt(receipt_id(&admitted)).await;
    assert_eq!(stored["receipt"]["status"], json!("committed"), "{stored}");
    let afterwards = matrix
        .peer()
        .capture(capture_body("k-c093-after-fence", TEXT, &shared_scope()))
        .await;
    assert_eq!(error_code(&afterwards), "SUBJECT_FENCED", "{afterwards}");
    assert_eq!(pending_candidates(&admin).await.len(), 1);
    matrix.shutdown().await;
}

/// One subject fence with owned inputs, so it can be issued while another op is
/// held.
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

/// C-095: a scoped admin cannot amplify its own coverage. A context superset, a
/// `shared_access` upgrade and a longer expiry are each refused with
/// `NOT_AUTHORIZED`; a sub-scope issuance with an earlier expiry commits and is
/// the only new grant. Refusals leave the grant set, and therefore the real
/// effective permissions, unchanged.
async fn scoped_admin_cannot_amplify_grant(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.clone();
    let third = "prn_c095_third";

    // The scoped admin grant: admin over CONTEXT only, no shared access, and an
    // explicit expiry.
    let expiry = (chrono::Utc::now() + chrono::Duration::hours(2))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let scoped = admin
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["admin"],
            "context_ids": [CONTEXT],
            "shared_access": false,
            "expires_at": expiry,
            "idempotency_key": "k-c095-admin",
        }))
        .await;
    assert!(is_committed(&scoped), "the scoped admin grant commits: {scoped}");
    let peer = matrix.peer();

    // 1. Context superset.
    let superset = peer
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": third,
            "capabilities": ["capture"],
            "context_ids": [CONTEXT, OTHER_CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c095-superset",
        }))
        .await;
    assert_eq!(error_code(&superset), "NOT_AUTHORIZED", "{superset}");
    assert_eq!(retry_class(&superset), "terminal");

    // 2. shared_access upgrade.
    let upgraded = peer
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": third,
            "capabilities": ["capture"],
            "context_ids": [CONTEXT],
            "shared_access": true,
            "idempotency_key": "k-c095-shared",
        }))
        .await;
    assert_eq!(error_code(&upgraded), "NOT_AUTHORIZED", "{upgraded}");

    // 3. Longer expiry than the granter's own.
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

    // No amplified grant exists for the third principal.
    assert!(
        admin
            .list_grants(json!({ "principal_id": third }))
            .await["grants"]
            .as_array()
            .expect("grant page")
            .is_empty(),
        "the refused amplifications created no grant"
    );

    // 4. The legitimate in-scope positive: a subset of the granter's coverage
    //    with an earlier expiry commits.
    let earlier = (chrono::Utc::now() + chrono::Duration::minutes(30))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
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

    // The scoped admin's own legitimate in-scope positive: its self-view is
    // readable and still names the grant it was issued.
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
    matrix.shutdown().await;
}

/// C-096: a fresh provider's operator provisioning creates the initial space and
/// administrator without any protocol grant cycle, and both are discoverable
/// through `memory.describe` / `memory.spaces.list`. No protocol space-creation
/// op exists.
///
/// The local leg opens the real `CoreService` (the operator entry) over a
/// disposable home and reads the provider that open pinned — a later
/// `LocalMemoryProvider::open` would not prove the bootstrap. The remote leg
/// runs the fixture's own operator provisioning (`FixtureMemoryProvider::open`
/// plus the operator admin grant) behind the signed service.
async fn operator_bootstrap_is_callable_and_visible(matrix: Matrix) {
    let admin = matrix.admin();
    let described = admin.describe().await;
    assert_eq!(
        described["capabilities"],
        json!(["spoke-memory"]),
        "the provider advertises the memory family: {described}"
    );
    assert_eq!(
        described["required_profile_kinds"],
        json!([]),
        "no unsupported capability prerequisite is advertised"
    );

    // The space exists and is active without any protocol creation call.
    let spaces = admin.list_spaces().await;
    let rows = spaces["spaces"].as_array().expect("space page");
    assert_eq!(rows.len(), 1, "exactly the provisioned space: {spaces}");
    assert_eq!(rows[0]["space_id"], json!(matrix.space_id()));
    assert_eq!(rows[0]["status"], json!("active"));
    assert_eq!(rows[0]["authority_generation"], json!(1));

    // The operator admin grant is already callable: the same principal lists it
    // as its self-view without the test issuing any grant.
    let grants = admin.list_grants(json!({})).await;
    let own = grants["grants"].as_array().expect("grant page");
    assert!(
        own.iter().any(|grant| {
            grant["principal_id"] == json!(matrix.admin_principal().principal_id)
                && grant["capabilities"] == json!(["admin"])
        }),
        "the operator admin grant is visible: {grants}"
    );

    // No protocol v1 space-creation op was invented: the wire refuses the name.
    let mut raw = matrix.raw_admin().await;
    let invented = raw
        .invoke("memory.spaces.create", &json!({ "space_id": matrix.space_id() }))
        .await;
    assert_eq!(
        invented["error"]["code"],
        json!("op_unsupported"),
        "space creation is not a protocol op: {invented}"
    );
    raw.close().await;

    // And the space count did not change.
    assert_eq!(
        matrix.admin().list_spaces().await["spaces"]
            .as_array()
            .expect("space page")
            .len(),
        1
    );
    matrix.shutdown().await;
}

/// C-096 (`_local` only): the real operator `CoreService` open provisions the
/// initial space and admin grant. The provider read here is the very instance
/// that open pinned, so the bootstrap is proved before any test-side provider
/// helper runs.
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

    // The operator open: this is the bootstrap, not a test-side helper.
    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::EngineOwner,
        memory: Some(MemoryProviderSelection::Local),
    })
    .await
    .expect("open Core over the disposable home");
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
            grant["principal_id"] == json!(creator_id) && grant["capabilities"] == json!(["admin"])
        }),
        "the operator admin grant exists without a protocol issuance: {grants}"
    );

    drop(provider);
    drop(core);

    // The served local surface still has no space-creation op, and refuses the
    // invented name without creating anything.
    let matrix = Matrix::local().await;
    let mut raw = matrix.raw_admin().await;
    let invented = raw
        .invoke("memory.spaces.create", &json!({ "space_id": matrix.space_id() }))
        .await;
    assert_eq!(
        invented["error"]["code"],
        json!("op_unsupported"),
        "{invented}"
    );
    raw.close().await;
    assert_eq!(
        matrix.admin().list_spaces().await["spaces"]
            .as_array()
            .expect("space page")
            .len(),
        1
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_090_grant_scope_covers_base_surfaces_local() {
    grant_scope_covers_base_surfaces(Matrix::local().await).await;
}

#[tokio::test]
async fn c_090_grant_scope_covers_base_surfaces_remote() {
    grant_scope_covers_base_surfaces(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_091_revoke_applies_at_admission_boundary_local() {
    revoke_applies_at_admission_boundary(Matrix::local().await).await;
}

#[tokio::test]
async fn c_091_revoke_applies_at_admission_boundary_remote() {
    revoke_applies_at_admission_boundary(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_092_concurrent_record_update_has_one_winner_local() {
    concurrent_record_update_has_one_winner(Matrix::local().await).await;
}

#[tokio::test]
async fn c_092_concurrent_record_update_has_one_winner_remote() {
    concurrent_record_update_has_one_winner(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_093_revoke_or_fence_race_has_one_outcome_local() {
    revoke_or_fence_race_has_one_outcome(Matrix::local().await).await;
}

#[tokio::test]
async fn c_093_revoke_or_fence_race_has_one_outcome_remote() {
    revoke_or_fence_race_has_one_outcome(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_095_scoped_admin_cannot_amplify_grant_local() {
    scoped_admin_cannot_amplify_grant(Matrix::local().await).await;
}

#[tokio::test]
async fn c_095_scoped_admin_cannot_amplify_grant_remote() {
    scoped_admin_cannot_amplify_grant(Matrix::remote().await).await;
}

#[tokio::test]
async fn c_096_operator_bootstrap_is_callable_and_visible_local() {
    local_operator_bootstrap_through_core().await;
    // The served-surface half of the row on a fresh local matrix is covered in
    // the same test; the provisioning half is the Core open above.
}

#[tokio::test]
async fn c_096_operator_bootstrap_is_callable_and_visible_remote() {
    operator_bootstrap_is_callable_and_visible(Matrix::remote().await).await;
}

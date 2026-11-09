//! The N4-R `_remote` reflection conformance selectors, over real loopback
//! Connect against the advertised durable fixture provider.
//!
//! Every selector drives the three served reflection operations through the
//! signed SDK sessions the matrix harness owns (no lookalike transport), and
//! reads the provider's own answers: committed receipts, honest staleness,
//! read-time basis digests and the §14.2 retry classes. The deliberately
//! unadvertised continuity shape lives where it always has — `c_003`/`c_073`
//! in `c_admission_lifecycle.rs`/`c_capability_refusals.rs` over
//! `Matrix::remote_unadvertised()`.
//!
//! Split-row accounting (§5): the `_local` mirrors of every row here are owned
//! by `v230-n4r-local-reflection` and are never asserted in this file.

mod support;

use std::time::Duration;

use serde_json::{json, Value};
use spoke_connect::remote::RemoteAdapter;
use spoke_operations::spoke_schemas::Principal;
use spoke_operations::MemoryPort;

use nexus_memory_conformance::{FaultHold, FaultPoint, FixtureFaults};

use support::{
    bootstrap, error_code, error_details, grant_body, is_committed, record_body, result, retry_class,
    wire, context_scope, shared_scope, Matrix, OTHER_SUBJECT, SUBJECT,
};

/// 64 hex characters: the CAS token shape `state` publishes.
fn is_hex_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_state_digest(value: &Value) -> bool {
    value.as_str().is_some_and(is_hex_digest)
}

fn state_body(subject: &str) -> Value {
    json!({
        "subject_id": subject,
        "selection": { "mode": "subject_all" },
    })
}

fn generate_body(subject: &str, digest: &str, key: &str) -> Value {
    json!({
        "subject_id": subject,
        "selection": { "mode": "subject_all" },
        "expected_state_digest": digest,
        "idempotency_key": key,
    })
}

fn dto<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid request")
}

/// One committed shared record on the matrix subject; returns its record id.
async fn create_shared_record(matrix: &Matrix, key: &str, text: &str) -> String {
    let created = matrix
        .admin()
        .create_record(record_body(key, "nexus.world_building", text, &shared_scope()))
        .await;
    assert!(
        is_committed(&created),
        "the seed record commits: {created}"
    );
    result(&created)["record_id"]
        .as_str()
        .expect("a created record names itself")
        .to_owned()
}

/// One committed context record on the matrix subject; returns its record id.
async fn create_context_record(matrix: &Matrix, key: &str, text: &str, context: &str) -> String {
    let created = matrix
        .admin()
        .create_record(record_body(
            key,
            "nexus.world_building",
            text,
            &context_scope(context),
        ))
        .await;
    assert!(
        is_committed(&created),
        "the seed record commits: {created}"
    );
    result(&created)["record_id"]
        .as_str()
        .expect("a created record names itself")
        .to_owned()
}

/// The operator's current `state` view: the digest CAS token the generate flow
/// pins (the admin grant implies `reflect`).
async fn admin_state(matrix: &Matrix, subject: &str) -> Value {
    matrix.admin().reflections_state(state_body(subject)).await
}

/// The granted peer's current `state` view.
async fn peer_state(matrix: &Matrix, subject: &str) -> Value {
    matrix.peer().reflections_state(state_body(subject)).await
}

/// Grant the second peer one capability at space level (unrestricted).
async fn grant_peer(matrix: &Matrix, capability: &str, key: &str) {
    let peer_id = matrix.peer_principal().principal_id.clone();
    let granted = matrix
        .admin()
        .grant(grant_body(capability, peer_id.as_str(), key))
        .await;
    assert!(
        is_committed(&granted),
        "the {capability} grant commits: {granted}"
    );
}

/// Generate with owned inputs, so a task can race the provider's fault points.
async fn generate_with(
    client: std::sync::Arc<RemoteAdapter>,
    principal: Principal,
    space_id: String,
    body: Value,
) -> Value {
    let mut body = body;
    body["space_id"] = json!(space_id);
    wire(&client.generate_reflection(&principal, dto(body)).await)
}

async fn join_bounded(
    handle: tokio::task::JoinHandle<Value>,
    what: &str,
) -> Value {
    tokio::time::timeout(Duration::from_secs(20), handle)
        .await
        .unwrap_or_else(|_| panic!("{what} exceeded the harness bound"))
        .expect("{what} task did not panic")
}

/// The whole-subject reflection listing, oldest-first.
async fn list_rows(matrix: &Matrix) -> Value {
    matrix
        .admin()
        .reflections_list(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "subject_all" },
        }))
        .await
}

// ---------------------------------------------------------------------------
// C-042: the read-time basis digest moves with every basis-affecting change
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_042_basis_changes_invalidate_reflection_state_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();

    create_shared_record(&matrix, "k-c042-r1", "The northern gate opens at dawn.").await;
    let first = admin_state(&matrix, SUBJECT).await;
    assert_eq!(first["data_sufficiency"], json!("sufficient"));
    assert_eq!(first["stale"], json!(false));
    let digest_one = first["state_digest"].clone();
    assert!(is_state_digest(&digest_one), "{first}");

    // A second source record changes the authorized basis: the next read's
    // digest differs, though nothing published has moved.
    create_shared_record(&matrix, "k-c042-r2", "Mira keeps the key under the barrel.").await;
    let second = admin_state(&matrix, SUBJECT).await;
    let digest_two = second["state_digest"].clone();
    assert_ne!(digest_one, digest_two, "{second}");
    assert_eq!(second["stale"], json!(false));

    // Publish against the current digest, then change the basis again: the
    // published row goes stale at the next read, computed never stored.
    let generated = admin
        .reflections_generate(generate_body(SUBJECT, digest_two.as_str().expect("digest"), "k-c042-gen"))
        .await;
    assert!(is_committed(&generated), "{generated}");
    let reflection_id = result(&generated)["reflection_id"]
        .as_str()
        .expect("the committed receipt names its reflection")
        .to_owned();
    let published = admin_state(&matrix, SUBJECT).await;
    assert_eq!(published["stale"], json!(false));
    assert_eq!(published["last_reflection_id"], json!(reflection_id));

    create_shared_record(&matrix, "k-c042-r3", "The harbor bell rings twice.").await;
    let stale = admin_state(&matrix, SUBJECT).await;
    assert_eq!(stale["stale"], json!(true), "{stale}");
    assert_ne!(stale["state_digest"], digest_two);
    assert_eq!(stale["last_reflection_id"], json!(reflection_id));

    // Regenerating against the moved basis publishes an honest fresh row.
    let regenerated = admin
        .reflections_generate(generate_body(
            SUBJECT,
            stale["state_digest"].as_str().expect("digest"),
            "k-c042-gen-2",
        ))
        .await;
    assert!(is_committed(&regenerated), "{regenerated}");
    let fresh = admin_state(&matrix, SUBJECT).await;
    assert_eq!(fresh["stale"], json!(false), "{fresh}");
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-070: committed generation, one terminal outcome per idempotency key
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_070_reflect_grant_committed_and_pending_generation_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    grant_peer(&matrix, "reflect", "k-c070-grant").await;
    create_shared_record(&matrix, "k-c070-r1", "The tide ledger records every arrival.").await;
    let digest = peer_state(&matrix, SUBJECT).await["state_digest"].clone();

    // A reflect holder's generate commits exactly one terminal receipt.
    let committed = matrix
        .peer()
        .reflections_generate(generate_body(SUBJECT, digest.as_str().expect("digest"), "k-c070-a"))
        .await;
    assert!(is_committed(&committed), "{committed}");
    let receipt_id = committed["receipt_id"].as_str().expect("receipt id");

    // The same key replays the stored terminal receipt, never re-executes.
    let replayed = matrix
        .peer()
        .reflections_generate(generate_body(SUBJECT, digest.as_str().expect("digest"), "k-c070-a"))
        .await;
    assert_eq!(replayed["receipt_id"], json!(receipt_id), "{replayed}");
    assert_eq!(
        replayed["result"]["reflection_id"],
        committed["result"]["reflection_id"]
    );

    // A paused admission cannot mint a second terminal outcome: while the
    // first identical admission is parked before its writer transaction, the
    // same-key request commits, and the released one replays that receipt.
    let mut hold: FaultHold = matrix
        .fixture_faults()
        .expect("the fixture leg exposes fault control")
        .hold_next_bounded(FaultPoint::AdmissionPause, Duration::from_secs(20));
    let client = matrix.peer_client();
    let principal = matrix.peer_principal().clone();
    let space = matrix.space_id().to_owned();
    let parked = tokio::spawn(generate_with(
        client,
        principal,
        space,
        generate_body(SUBJECT, digest.as_str().expect("digest"), "k-c070-race"),
    ));
    hold.wait_reached()
        .await
        .expect("the parked admission reached the pause point");
    let racing = join_bounded(
        tokio::spawn({
            let client = matrix.peer_client();
            let principal = matrix.peer_principal().clone();
            let space = matrix.space_id().to_owned();
            generate_with(
                client,
                principal,
                space,
                generate_body(SUBJECT, digest.as_str().expect("digest"), "k-c070-race"),
            )
        }),
        "the racing same-key generate",
    )
    .await;
    assert!(is_committed(&racing), "{racing}");
    hold.release();
    let resumed = join_bounded(parked, "the parked generate").await;
    assert_eq!(
        resumed["receipt_id"], racing["receipt_id"],
        "the released admission replays the one terminal outcome: {resumed} {racing}"
    );
    let rows = matrix
        .peer()
        .reflections_list(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "subject_all" },
        }))
        .await;
    let listed = rows["reflections"].as_array().expect("reflection rows");
    assert_eq!(listed.len(), 2, "two keys, exactly two rows: {rows}");
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-071: the terminal receipt and the pinned basis listing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_071_terminal_receipt_and_pinned_basis_listing_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-c071-r1", "Charts are revised, not rewritten.").await;
    let state = admin_state(&matrix, SUBJECT).await;
    let generated = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            state["state_digest"].as_str().expect("digest"),
            "k-c071-gen",
        ))
        .await;
    assert!(is_committed(&generated), "{generated}");
    let receipt_id = generated["receipt_id"].as_str().expect("receipt id");

    // The terminal receipt is durable and content-free: it names the
    // reflection and the one charged cost, never the body text.
    let stored = matrix.admin().get_receipt(receipt_id).await;
    let receipt = &stored["receipt"];
    assert_eq!(receipt["status"], json!("committed"), "{stored}");
    assert_eq!(receipt["op"], json!("memory.reflections.generate"));
    assert_eq!(
        receipt["result"]["reflection_id"],
        generated["result"]["reflection_id"]
    );
    assert_eq!(
        receipt["result"]["cost"],
        json!({ "unit_kind": "fixture_reflection_unit", "units": 1.0 }),
        "{stored}"
    );
    let rendered = stored.to_string();
    assert!(
        !rendered.contains("Charts are revised"),
        "the receipt never carries the source content: {stored}"
    );

    // The listing serves the pinned basis: the four published members, the
    // pinned selection and the deterministic markdown body.
    let page = matrix
        .admin()
        .reflections_list(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "subject_all" },
        }))
        .await;
    let rows = page["reflections"].as_array().expect("reflection rows");
    assert_eq!(rows.len(), 1, "{page}");
    let row = &rows[0];
    assert_eq!(row["reflection_id"], generated["result"]["reflection_id"]);
    assert_eq!(row["space_id"], json!(matrix.space_id()));
    assert_eq!(row["subject_id"], json!(SUBJECT));
    assert_eq!(row["revision"], json!(1));
    let schema_version = row["schema_version"].as_u64().expect("schema version");
    assert!(schema_version >= 1);
    let basis = &row["basis"];
    assert_eq!(basis["record_count"], json!(1), "{row}");
    assert!(
        basis["source_set_digest"].as_str().is_some_and(is_hex_digest),
        "the basis pins a 64-hex source-set digest: {row}"
    );
    assert!(
        basis["state_digest"].as_str().is_some_and(is_hex_digest),
        "the basis pins a 64-hex state digest: {row}"
    );
    assert!(
        row["selection"] == json!({ "mode": "subject_all" }),
        "the pinned selection is served verbatim: {row}"
    );
    assert_eq!(row["body"]["format"], json!("markdown"), "{row}");
    assert!(
        row["body"]["body"].as_str().is_some_and(|b| !b.is_empty()),
        "the body is served: {row}"
    );
    assert_eq!(page["next_cursor"], Value::Null, "{page}");
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-072: the generated envelope shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_072_generated_envelope_shape_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-c072-r1", "The envelope carries facts, not prose.").await;
    let described = matrix.admin().describe().await;
    let state = admin_state(&matrix, SUBJECT).await;

    // The state view's own shape: the published members, the CAS token, and
    // the fixture's empty required-kinds list.
    assert_eq!(
        state["missing_required_profile_kinds"],
        json!([]),
        "{state}"
    );
    assert!(
        state["basis"]["profile_kinds"].is_array(),
        "the basis names its profile kinds: {state}"
    );
    assert_eq!(state["basis"]["record_count"], json!(1));
    assert!(is_state_digest(&state["state_digest"]));

    let generated = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            state["state_digest"].as_str().expect("digest"),
            "k-c072-gen",
        ))
        .await;
    assert!(is_committed(&generated), "{generated}");
    // Receipt envelope: content-free, costed exactly once, self-naming.
    assert_eq!(
        generated["op"],
        json!("memory.reflections.generate"),
        "{generated}"
    );
    assert!(
        generated["payload_digest"]
            .as_str()
            .is_some_and(is_hex_digest),
        "the receipt carries a JCS payload digest: {generated}"
    );
    assert!(generated["created_at"].is_string());
    assert!(generated["completed_at"].is_string());
    assert_eq!(
        generated["result"]["cost"]["units"],
        json!(1.0),
        "the committed receipt charges once: {generated}"
    );

    let page = matrix
        .admin()
        .reflections_list(json!({ "subject_id": SUBJECT }))
        .await;
    let row = page["reflections"][0].clone();
    // The published Reflection object: every required member, the generator
    // facts and the read-time staleness verdict.
    for member in [
        "reflection_id",
        "space_id",
        "subject_id",
        "revision",
        "schema_version",
        "created_at",
        "selection",
        "basis",
        "body",
        "generator",
        "extensions",
        "stale",
    ] {
        assert!(
            row.get(member).is_some(),
            "the published reflection carries {member}: {row}"
        );
    }
    assert_eq!(row["extensions"], json!({}), "{row}");
    assert_eq!(
        row["generator"]["engine_policy_version"],
        described["engine_policy_version"],
        "the row stamps the provider's own policy version: {row}"
    );
    assert!(
        row["generator"]["model"].is_null(),
        "the fixture's deterministic generator names no model: {row}"
    );
    assert_eq!(row["stale"], json!(false));
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-073 (served half): reflection reads succeed with the capability
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_073_reflection_reads_succeed_with_capability_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    grant_peer(&matrix, "reflect", "k-c073-grant").await;
    create_shared_record(&matrix, "k-c073-r1", "A granted reader reads the real basis.").await;
    let digest = peer_state(&matrix, SUBJECT).await["state_digest"].clone();
    let generated = matrix
        .peer()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-c073-gen",
        ))
        .await;
    assert!(
        is_committed(&generated),
        "a reflect holder generates over real loopback: {generated}"
    );

    // Both reads answer real data over the same signed session — the family
    // is genuinely served, never capability_missing.
    let state = peer_state(&matrix, SUBJECT).await;
    assert!(state.get("error").is_none(), "{state}");
    assert_eq!(state["basis"]["record_count"], json!(1));
    let page = matrix
        .peer()
        .reflections_list(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "subject_all" },
        }))
        .await;
    assert!(page.get("error").is_none(), "{page}");
    assert_eq!(page["reflections"].as_array().expect("rows").len(), 1);
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-075: subject_all requires an unrestricted grant
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_075_reflection_subject_all_requires_unrestricted_grant_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-c075-r1", "Whole-subject reads need whole coverage.").await;

    // A context-restricted reflect grant does not cover subject_all.
    let peer_id = matrix.peer_principal().principal_id.clone();
    let restricted = matrix
        .admin()
        .grant(json!({
            "subject_id": SUBJECT,
            "principal_id": peer_id,
            "capabilities": ["reflect"],
            "context_ids": [support::CONTEXT],
            "shared_access": false,
            "idempotency_key": "k-c075-restricted",
        }))
        .await;
    assert!(is_committed(&restricted), "{restricted}");
    let denied = peer_state(&matrix, SUBJECT).await;
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");

    // The same grant covers exactly the context it names.
    let scoped = matrix
        .peer()
        .reflections_state(json!({
            "subject_id": SUBJECT,
            "selection": {
                "mode": "selected_contexts",
                "context_ids": [support::CONTEXT],
            },
        }))
        .await;
    assert!(scoped.get("error").is_none(), "{scoped}");

    // An unrestricted space-level reflect grant covers subject_all.
    grant_peer(&matrix, "reflect", "k-c075-unrestricted").await;
    let allowed = peer_state(&matrix, SUBJECT).await;
    assert!(allowed.get("error").is_none(), "{allowed}");
    assert!(is_state_digest(&allowed["state_digest"]));
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-076: pinned basis publishes and reports honest staleness
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_076_pinned_basis_publishes_and_reports_honest_staleness_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-c076-r1", "The first chart of the harbor.").await;
    let state = admin_state(&matrix, SUBJECT).await;
    let generated = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            state["state_digest"].as_str().expect("digest"),
            "k-c076-gen",
        ))
        .await;
    assert!(is_committed(&generated), "{generated}");

    let rows = list_rows(&matrix).await["reflections"].clone();
    assert_eq!(rows.as_array().expect("rows").len(), 1);
    assert_eq!(rows[0]["stale"], json!(false), "{rows}");

    // The basis moves; the pinned row reports stale at read time — the stored
    // flag is never trusted.
    create_shared_record(&matrix, "k-c076-r2", "A new lighthouse changes the chart.").await;
    let rows = list_rows(&matrix).await["reflections"].clone();
    assert_eq!(rows[0]["stale"], json!(true), "{rows}");

    // A fresh publication is honest: the new row fresh, the old row stale.
    let digest = admin_state(&matrix, SUBJECT).await["state_digest"].clone();
    let regenerated = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-c076-gen-2",
        ))
        .await;
    assert!(is_committed(&regenerated), "{regenerated}");
    let rows = list_rows(&matrix).await["reflections"].clone();
    let rows = rows.as_array().expect("rows");
    assert_eq!(rows.len(), 2, "rows are never rewritten in place");
    let stale_flags: Vec<bool> = rows
        .iter()
        .map(|row| row["stale"] == json!(true))
        .collect();
    assert_eq!(
        stale_flags,
        vec![true, false],
        "oldest-first: the superseded row stays stale, the new one is fresh: {rows:#?}"
    );
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-077: tombstoned bases are excluded from list and the state lookup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_077_tombstoned_bases_excluded_from_list_get_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    let context_record =
        create_context_record(&matrix, "k-c077-r1", "Only the market context.", support::CONTEXT)
            .await;
    create_shared_record(&matrix, "k-c077-r2", "Everyone can read this chart.").await;
    create_shared_record(&matrix, "k-c077-r3", "And this second shared chart.").await;

    // One reflection pinned to the shared_only selection, one to the context
    // selection, so each row pins a disjoint source set.
    let shared_state = matrix
        .admin()
        .reflections_state(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "shared_only" },
        }))
        .await;
    let context_state = matrix
        .admin()
        .reflections_state(json!({
            "subject_id": SUBJECT,
            "selection": {
                "mode": "selected_contexts",
                "context_ids": [support::CONTEXT],
            },
        }))
        .await;
    let shared_row = matrix
        .admin()
        .reflections_generate(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "shared_only" },
            "expected_state_digest": shared_state["state_digest"].as_str().expect("digest"),
            "idempotency_key": "k-c077-gen-shared",
        }))
        .await;
    let context_row = matrix
        .admin()
        .reflections_generate(json!({
            "subject_id": SUBJECT,
            "selection": {
                "mode": "selected_contexts",
                "context_ids": [support::CONTEXT],
            },
            "expected_state_digest": context_state["state_digest"].as_str().expect("digest"),
            "idempotency_key": "k-c077-gen-ctx",
        }))
        .await;
    assert!(is_committed(&shared_row) && is_committed(&context_row));

    // Tombstone the context record: the context row's WHOLE pinned source set
    // is dead, so it is excluded from the listing and from the state's
    // last-reflection lookup, while the shared row keeps serving.
    let deleted = matrix
        .admin()
        .delete_record(json!({
            "record_id": context_record,
            "expected_revision": 1,
            "reason": "tombstoned by the C-077 selector",
            "idempotency_key": "k-c077-del-ctx",
        }))
        .await;
    assert!(is_committed(&deleted), "{deleted}");
    let page = matrix
        .admin()
        .reflections_list(json!({ "subject_id": SUBJECT }))
        .await;
    let rows = page["reflections"].as_array().expect("rows");
    assert_eq!(
        rows.len(),
        1,
        "the fully tombstoned row is excluded from the listing: {page}"
    );
    assert_eq!(
        rows[0]["reflection_id"],
        shared_row["result"]["reflection_id"],
        "{page}"
    );
    let state = matrix
        .admin()
        .reflections_state(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "shared_only" },
        }))
        .await;
    assert_eq!(
        state["last_reflection_id"],
        shared_row["result"]["reflection_id"],
        "the surviving selection's state lookup serves its pinned row: {state}"
    );
    let context_lookup = matrix
        .admin()
        .reflections_state(json!({
            "subject_id": SUBJECT,
            "selection": {
                "mode": "selected_contexts",
                "context_ids": [support::CONTEXT],
            },
        }))
        .await;
    assert!(
        context_lookup.get("last_reflection_id").is_none(),
        "a fully tombstoned row is no reflection history: {context_lookup}"
    );

    // A partial tombstone keeps the row but reports it stale.
    let records = matrix
        .admin()
        .list_records(json!({
            "subject_id": SUBJECT,
            "selection": { "mode": "shared_only" },
        }))
        .await;
    let record_id = records["records"][0]["record_id"]
        .as_str()
        .expect("record id")
        .to_owned();
    let deleted = matrix
        .admin()
        .delete_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "reason": "partially tombstoned by the C-077 selector",
            "idempotency_key": "k-c077-del-shared",
        }))
        .await;
    assert!(is_committed(&deleted), "{deleted}");
    let page = matrix
        .admin()
        .reflections_list(json!({ "subject_id": SUBJECT }))
        .await;
    let rows = page["reflections"].as_array().expect("rows");
    assert_eq!(
        rows.len(),
        1,
        "a partially tombstoned set keeps its reflection: {page}"
    );
    assert_eq!(rows[0]["stale"], json!(true), "{page}");
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-078: generation failure honesty and no double charge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_078_generation_failure_honesty_and_no_double_charge_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-c078-r1", "The generator can fail honestly.").await;
    let digest = admin_state(&matrix, SUBJECT).await["state_digest"].clone();

    // The one-shot fault makes the generation step fail: transaction 2
    // commits a terminal failed receipt with an error envelope, retry class
    // and no cost — never a fabricated success.
    let faults: FixtureFaults = matrix
        .fixture_faults()
        .expect("the fixture leg exposes fault control");
    faults.fail_next(FaultPoint::GenerationFailure);
    let failed = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-c078-gen",
        ))
        .await;
    assert_eq!(failed["status"], json!("failed"), "{failed}");
    assert_eq!(failed["error"]["code"], json!("INTERNAL_ERROR"), "{failed}");
    assert_eq!(retry_class(&failed), "retryable");
    assert!(
        failed.get("result").is_none() || failed["result"].is_null(),
        "a failed receipt carries no result and charges nothing: {failed}"
    );
    let receipt_id = failed["receipt_id"].as_str().expect("receipt id");
    let stored = matrix.admin().get_receipt(receipt_id).await;
    assert_eq!(stored["receipt"]["status"], json!("failed"), "{stored}");
    assert_eq!(
        stored["receipt"]["error"]["code"],
        json!("INTERNAL_ERROR"),
        "{stored}"
    );

    // The failed run published nothing.
    let page = matrix
        .admin()
        .reflections_list(json!({ "subject_id": SUBJECT }))
        .await;
    assert!(
        page["reflections"].as_array().expect("rows").is_empty(),
        "a failed generation publishes no reflection: {page}"
    );

    // The same key replays the same failed receipt; no second charge, no
    // re-run of the generation step.
    let replayed = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-c078-gen",
        ))
        .await;
    assert_eq!(replayed["receipt_id"], json!(receipt_id), "{replayed}");
    assert_eq!(replayed["status"], json!("failed"));

    // A fresh key generates for real, and only the committed run is charged.
    let committed = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-c078-gen-2",
        ))
        .await;
    assert!(is_committed(&committed), "{committed}");
    assert_eq!(
        committed["result"]["cost"]["units"],
        json!(1.0),
        "exactly the committed run carries the cost: {committed}"
    );
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-079: the fixture's insufficient-data policy outcomes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_079_insufficient_data_policy_outcomes_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    let ensured = matrix.admin().ensure_subject(OTHER_SUBJECT).await;
    assert!(is_committed(&ensured), "{ensured}");

    // No authorized source record: the state verdict is honest and the
    // generate refusal is the fixture's provider-variable policy outcome.
    let empty = matrix
        .admin()
        .reflections_state(state_body(OTHER_SUBJECT))
        .await;
    assert_eq!(empty["data_sufficiency"], json!("insufficient"), "{empty}");
    assert_eq!(empty["basis"]["record_count"], json!(0));
    assert_eq!(empty["stale"], json!(false));
    assert_eq!(
        empty["missing_required_profile_kinds"],
        json!([]),
        "the fixture requires no authored kinds: {empty}"
    );
    let refused = matrix
        .admin()
        .reflections_generate(json!({
            "subject_id": OTHER_SUBJECT,
            "selection": { "mode": "subject_all" },
            "expected_state_digest": empty["state_digest"].as_str().expect("digest"),
            "idempotency_key": "k-c079-gen",
        }))
        .await;
    assert_eq!(error_code(&refused), "INSUFFICIENT_DATA", "{refused}");
    assert_eq!(retry_class(&refused), "terminal");
    let page = matrix
        .admin()
        .reflections_list(json!({ "subject_id": OTHER_SUBJECT }))
        .await;
    assert!(page["reflections"].as_array().expect("rows").is_empty());

    // One authorized record flips the verdict to sufficient.
    let created = matrix
        .admin()
        .create_record(json!({
            "subject_id": OTHER_SUBJECT,
            "kind": "nexus.world_building",
            "content": { "text": "Now there is something to reflect over." },
            "scope": shared_scope(),
            "idempotency_key": "k-c079-r1",
        }))
        .await;
    assert!(is_committed(&created), "{created}");
    let sufficient = matrix
        .admin()
        .reflections_state(state_body(OTHER_SUBJECT))
        .await;
    assert_eq!(
        sufficient["data_sufficiency"],
        json!("sufficient"),
        "{sufficient}"
    );
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-015 (reflection half): the pending receipt is a real long-running operation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_015_pending_receipt_long_running_operation_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    grant_peer(&matrix, "reflect", "k-c015-grant").await;
    create_shared_record(&matrix, "k-c015-r1", "Generation takes a real while.").await;
    let digest = peer_state(&matrix, SUBJECT).await["state_digest"].clone();

    // Park the generate after its pending receipt commit, before generation.
    let mut hold = matrix
        .fixture_faults()
        .expect("the fixture leg exposes fault control")
        .hold_next_bounded(FaultPoint::GenerationPause, Duration::from_secs(20));
    let parked = tokio::spawn({
        let client = matrix.peer_client();
        let principal = matrix.peer_principal().clone();
        let space = matrix.space_id().to_owned();
        generate_with(
            client,
            principal,
            space,
            generate_body(SUBJECT, digest.as_str().expect("digest"), "k-c015-gen"),
        )
    });
    hold.wait_reached()
        .await
        .expect("the generate reached its generation pause");

    // The pending receipt is durably observable while the operation runs.
    let mut observer = matrix
        .durable_observer(SUBJECT, &[], None, None, None)
        .await;
    let pending_ids = observer
        .receipt_ids_for("memory.reflections.generate")
        .await;
    assert_eq!(pending_ids.len(), 1, "exactly the pending receipt");
    let pending = matrix.admin().get_receipt(&pending_ids[0]).await;
    assert_eq!(pending["receipt"]["status"], json!("pending"), "{pending}");
    assert!(
        pending["receipt"]["result"].is_null(),
        "a pending receipt has no result: {pending}"
    );
    observer.close().await;

    // Release: the generation completes and the same receipt terminalizes.
    hold.release();
    let resumed = join_bounded(parked, "the parked generate").await;
    assert_eq!(resumed["receipt_id"], json!(pending_ids[0]), "{resumed}");
    assert!(is_committed(&resumed), "{resumed}");
    let terminal = matrix.admin().get_receipt(&pending_ids[0]).await;
    assert_eq!(terminal["receipt"]["status"], json!("committed"));
    assert_eq!(
        terminal["receipt"]["result"]["reflection_id"],
        resumed["result"]["reflection_id"]
    );
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-090: the reflection grant surfaces
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_090_reflection_grant_surfaces_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-c090-r1", "Surfaces decide who reflects.").await;
    let state = admin_state(&matrix, SUBJECT).await;
    let digest = state["state_digest"].clone();

    // No reflect grant: all three ops refuse NOT_AUTHORIZED and stage nothing.
    let denied = matrix.peer().reflections_state(state_body(SUBJECT)).await;
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");
    let denied_list = matrix
        .peer()
        .reflections_list(json!({ "subject_id": SUBJECT }))
        .await;
    assert_eq!(error_code(&denied_list), "NOT_AUTHORIZED");
    let denied_gen = matrix
        .peer()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-c090-gen",
        ))
        .await;
    assert_eq!(error_code(&denied_gen), "NOT_AUTHORIZED", "{denied_gen}");
    let spaces = matrix.admin().list_spaces().await;
    assert_eq!(
        spaces["spaces"][0]["pending_receipts"],
        json!(0),
        "refused reflections stage no receipt: {spaces}"
    );

    // The reflect grant opens all three surfaces for the grantee.
    grant_peer(&matrix, "reflect", "k-c090-grant").await;
    assert!(matrix
        .peer()
        .reflections_state(state_body(SUBJECT))
        .await
        .get("error")
        .is_none());
    assert!(matrix
        .peer()
        .reflections_list(json!({ "subject_id": SUBJECT }))
        .await
        .get("error")
        .is_none());
    let generated = matrix
        .peer()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-c090-gen",
        ))
        .await;
    assert!(is_committed(&generated), "{generated}");

    // The operator's admin grant implies reflect (published evaluator).
    assert!(matrix
        .admin()
        .reflections_state(state_body(SUBJECT))
        .await
        .get("error")
        .is_none());
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// C-100: the reflection error envelopes and their retry classes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn c_100_reflection_error_classes_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-c100-r1", "Refusals carry honest classes.").await;
    let digest = admin_state(&matrix, SUBJECT).await["state_digest"].clone();

    // No grant: terminal.
    let denied = matrix.peer().reflections_state(state_body(SUBJECT)).await;
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED");
    assert_eq!(retry_class(&denied), "terminal");

    // The cognition family is not served: the honest provider refusal is
    // crate-tested; over the typed SDK the cognition requirement is decided
    // at Connect negotiation before the payload reaches the provider. The
    // INVALID_INPUT envelope itself is exercised through the validator's
    // page-bound refusal instead.
    let over_limit = matrix
        .admin()
        .reflections_list(json!({
            "subject_id": SUBJECT,
            "limit": 9_999,
        }))
        .await;
    assert_eq!(error_code(&over_limit), "INVALID_INPUT", "{over_limit}");
    assert_eq!(retry_class(&over_limit), "terminal");

    // A CAS token that is not the current basis: REVISION_CONFLICT, terminal.
    let conflict = matrix
        .admin()
        .reflections_generate(generate_body(SUBJECT, &"0".repeat(64), "k-c100-cas"))
        .await;
    assert_eq!(error_code(&conflict), "REVISION_CONFLICT", "{conflict}");
    assert_eq!(retry_class(&conflict), "terminal");

    // No source records: INSUFFICIENT_DATA, terminal.
    let ensured = matrix.admin().ensure_subject(OTHER_SUBJECT).await;
    assert!(is_committed(&ensured));
    let empty = matrix
        .admin()
        .reflections_state(state_body(OTHER_SUBJECT))
        .await;
    let insufficient = matrix
        .admin()
        .reflections_generate(json!({
            "subject_id": OTHER_SUBJECT,
            "selection": { "mode": "subject_all" },
            "expected_state_digest": empty["state_digest"].as_str().expect("digest"),
            "idempotency_key": "k-c100-insufficient",
        }))
        .await;
    assert_eq!(error_code(&insufficient), "INSUFFICIENT_DATA");
    assert_eq!(retry_class(&insufficient), "terminal");

    // A frozen space admits the reads and refuses the generate: terminal.
    let frozen = matrix.admin().freeze_space("k-c100-freeze", 1).await;
    assert!(is_committed(&frozen), "{frozen}");
    let read = matrix
        .admin()
        .reflections_state(state_body(SUBJECT))
        .await;
    assert!(read.get("error").is_none(), "reads are freeze-allowlisted");
    let frozen_generate = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-c100-frozen",
        ))
        .await;
    assert_eq!(error_code(&frozen_generate), "SPACE_FROZEN", "{frozen_generate}");
    assert_eq!(retry_class(&frozen_generate), "terminal");
    let unfrozen = matrix.admin().unfreeze_space("k-c100-unfreeze", 2).await;
    assert!(is_committed(&unfrozen), "{unfrozen}");

    // A malformed list cursor is refused, never silently resumed: terminal.
    let cursor = matrix
        .admin()
        .reflections_list(json!({
            "subject_id": SUBJECT,
            "cursor": "not-a-real-cursor",
        }))
        .await;
    assert_eq!(error_code(&cursor), "CURSOR_INVALID", "{cursor}");
    assert_eq!(retry_class(&cursor), "terminal");

    // None of the refusals published a reflection row.
    let page = matrix
        .admin()
        .reflections_list(json!({ "subject_id": SUBJECT }))
        .await;
    assert!(
        page["reflections"].as_array().expect("rows").is_empty(),
        "refusals publish nothing: {page}"
    );
    matrix.shutdown().await;
}

// ---------------------------------------------------------------------------
// Fix round: blocking-warning regressions (qc-consolidated B3, B5)
// ---------------------------------------------------------------------------

/// B3: an authorized same-key replay outranks the §8.2 freeze gate over the
/// fixture boundary. A committed generation whose response was lost is
/// recoverable through the identical retry while the space is frozen; a fresh
/// key still answers `SPACE_FROZEN`, and unfreezing lets it run exactly one
/// new generation. The fixture checked freeze before its replay resolution,
/// which made already-committed work unrecoverable during a freeze.
#[tokio::test]
async fn qc_b3_committed_generation_replays_while_frozen_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-b3-r1", "The committed chart stays recoverable.").await;
    let digest = admin_state(&matrix, SUBJECT).await["state_digest"].clone();
    let first = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-b3-commit",
        ))
        .await;
    assert!(is_committed(&first), "{first}");
    let receipt_id = first["receipt_id"].as_str().expect("receipt id").to_owned();

    let frozen = matrix.admin().freeze_space("k-b3-freeze", 1).await;
    assert!(is_committed(&frozen), "{frozen}");

    let replayed = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-b3-commit",
        ))
        .await;
    assert!(is_committed(&replayed), "the replay outranks the freeze: {replayed}");
    assert_eq!(replayed["receipt_id"], json!(receipt_id), "{replayed}");

    let fresh = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-b3-fresh",
        ))
        .await;
    assert_eq!(error_code(&fresh), "SPACE_FROZEN", "{fresh}");
    assert_eq!(retry_class(&fresh), "terminal");

    let unfrozen = matrix.admin().unfreeze_space("k-b3-unfreeze", 2).await;
    assert!(is_committed(&unfrozen), "{unfrozen}");
    let second = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-b3-fresh",
        ))
        .await;
    assert!(is_committed(&second), "{second}");
    assert_ne!(second["receipt_id"], json!(receipt_id), "{second}");
    let rows = list_rows(&matrix).await["reflections"].clone();
    assert_eq!(
        rows.as_array().expect("rows").len(),
        2,
        "the replay added no envelope; the fresh key added exactly one: {rows}"
    );
    matrix.shutdown().await;
}

/// B5: transaction 1 enforces the §7.7 subject lifecycle gate after replay
/// resolution and before staging fresh work. A fenced subject rejects a fresh
/// generation with the protocol's fence refusal, while its already-admitted
/// completion still replays above the fence and its reads stay served.
#[tokio::test]
async fn qc_b5_fenced_subject_refuses_fresh_generation_remote() {
    let matrix = Matrix::remote().await;
    bootstrap(&matrix, &[]).await;
    create_shared_record(&matrix, "k-b5-r1", "The fence stops fresh work only.").await;
    let digest = admin_state(&matrix, SUBJECT).await["state_digest"].clone();
    let first = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-b5-commit",
        ))
        .await;
    assert!(is_committed(&first), "{first}");
    let receipt_id = first["receipt_id"].as_str().expect("receipt id").to_owned();

    let fenced = matrix
        .admin()
        .fence_subject(SUBJECT, json!({ "type": "subject" }), "fence", 1, "k-b5-fence")
        .await;
    assert!(is_committed(&fenced), "{fenced}");

    // The already-admitted completion keeps replaying above the fence.
    let replayed = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-b5-commit",
        ))
        .await;
    assert!(is_committed(&replayed), "{replayed}");
    assert_eq!(replayed["receipt_id"], json!(receipt_id), "{replayed}");

    // Fresh work on the fenced subject is refused with the fence refusal.
    let fresh = matrix
        .admin()
        .reflections_generate(generate_body(
            SUBJECT,
            digest.as_str().expect("digest"),
            "k-b5-fresh",
        ))
        .await;
    assert_eq!(error_code(&fresh), "SUBJECT_FENCED", "{fresh}");
    assert_eq!(retry_class(&fresh), "terminal");
    assert_eq!(error_details(&fresh)["scope"], json!("subject"), "{fresh}");
    assert!(
        fresh["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(SUBJECT)),
        "the refusal names the fenced subject: {fresh}"
    );

    // Reads are not fenced: the fenced subject's state stays served.
    let read = matrix.admin().reflections_state(state_body(SUBJECT)).await;
    assert!(read.get("error").is_none(), "reads stay admitted: {read}");
    matrix.shutdown().await;
}

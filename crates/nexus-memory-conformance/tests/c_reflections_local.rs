//! N4-R `_local` reflection selectors (`v230-n4r-local-reflection`).
//!
//! The full `## Verification` selector set (13 rows), completed by Task 6 over
//! the Task 2/3 admissions and read path. The rows drive the local provider's
//! own [`MemoryPort`] directly — this file's convention since Task 2, keeping
//! each row on the admission/flow rule it owns; the signed-wire dispatch shapes
//! (C-003/C-073's rows) stay in their own files, where the unadvertised matrix
//! exercises the dispatch gate.
//!
//! C-075 row: a context-restricted `reflect` grant cannot call the
//! reflection family on a subject-wide selection — `state`, `list` and
//! `generate` all refuse `NOT_AUTHORIZED` (terminal), because a
//! context-limited grant cannot read or generate all-subject derived text. No
//! derived-text scope leaks: the refusal carries no payload, and the same
//! grantee's context-scoped selection is not coverage-refused (the refusal is
//! the grant's reach, not the family), while the unrestricted `admin` grant
//! still implies `reflect` within its coverage.
//!
//! Task 3 rows (read path): C-073 — `state`/`list` serve under `reflect`
//! coverage and stay served through a freeze (§8.2 read-only allowlist);
//! C-042 — every basis-affecting fact class (create, edit, delete, promote,
//! profile update, authorization change altering the authorized set) moves the
//! published `state_digest` at the next read with no eager invalidation;
//! C-076 — the content-free basis publishes honestly (kinds, count,
//! sufficiency, required kinds) and the digest binds the selection/filter
//! resolution; C-071 — the listing contract (empty and bounded pages, absent
//! end-of-results cursor, opaque-cursor binding with `CURSOR_INVALID`);
//! C-077 — a tombstoned row leaves the authorized basis at read time. Task 6
//! completes the pinned-envelope halves (staleness against a pinned basis,
//! envelope paging, tombstoned-basis exclusion) and adds the generation-flow
//! rows (C-070/C-072/C-078/C-079/C-015), the grant-surface row (C-090) and the
//! error-class walk (C-100) over the deterministic test-hooks seams.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

use serde_json::{json, Value};
use spoke_operations::{
    spoke_schemas as schemas, MemoryPort, SpokeReject, SpokeRejectCode, SpokeResult,
};

use nexus_core::memory_provider::LocalMemoryProvider;
use sqlx::Connection as _;
use std::sync::Arc;

/// C-075's granter: the provider's own provisioning identity, whose bootstrap
/// grant is a space-wide unrestricted `admin`.
const ADMIN: &str = "ctr_c075_admin";
/// The context-restricted grantee under test.
const GRANTEE: &str = "ctr_c075_grantee";
/// The subject the subject-wide selections name. It is deliberately never
/// ensured: admission must not consult subject existence (no existence
/// oracle), and C-075's refusal is a grant-coverage verdict.
const SUBJECT: &str = "sbj_c075";
/// The only placement context the grantee's `reflect` grant covers.
const CONTEXT: &str = "ctx_c075_a";
/// A well-formed 64-lowercase-hex CAS token for the generate leg; the row
/// isolates coverage, so the digest is shape-valid but arbitrary.
const STATE_DIGEST: &str = "a7b4c2d0e9f183726554433f2e1d0c9b8a7968594a3b2c1d0e9f887766554433";

fn dto<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("request payload deserializes")
}

fn principal(principal_id: &str) -> schemas::Principal {
    serde_json::from_value(json!({ "principal_id": principal_id, "extensions": {} }))
        .expect("principal deserializes")
}

/// Assert one reflection refusal is the C-075 verdict: `NOT_AUTHORIZED`,
/// classified terminal, carrying no payload branch.
fn expect_subject_wide_refusal(reject: &SpokeReject, op: &str) {
    assert_eq!(
        reject.code,
        SpokeRejectCode::NotAuthorized,
        "{op} on a subject-wide selection under a context-restricted grant is refused: {reject:?}"
    );
    assert_eq!(
        reject
            .details
            .as_ref()
            .and_then(|details| details["retry_class"].as_str()),
        Some("terminal"),
        "{op} refusal names its retry class: {reject:?}"
    );
}

#[tokio::test]
async fn c_075_reflection_subject_all_requires_unrestricted_grant_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let space = provider.space_id().to_owned();
    let admin = principal(ADMIN);
    let grantee = principal(GRANTEE);

    let grant_receipt = provider
        .grant(
            &admin,
            dto(json!({
                "space_id": space,
                "subject_id": SUBJECT,
                "principal_id": GRANTEE,
                "capabilities": ["reflect"],
                "context_ids": [CONTEXT],
                "shared_access": false,
                "idempotency_key": "c075-reflect-c1",
            })),
        )
        .await;
    let receipt = match grant_receipt {
        SpokeResult::Ok(response) => {
            serde_json::to_value(&response).expect("grant receipt serializes")
        }
        SpokeResult::Reject(reject) => {
            panic!("the context-restricted reflect grant issues: {reject:?}")
        }
    };
    assert_eq!(
        receipt["status"],
        json!("committed"),
        "the reflect grant commits: {receipt}"
    );

    // The C-075 refusal itself: all three ops, subject-wide selection.
    match provider
        .get_reflection_state(
            &grantee,
            dto(json!({
                "space_id": space,
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => {
            expect_subject_wide_refusal(&reject, "memory.reflections.state");
        }
        SpokeResult::Ok(_) => {
            panic!("subject-wide state must not serve under a context-restricted grant")
        }
    }
    match provider
        .list_reflections(
            &grantee,
            dto(json!({
                "space_id": space,
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => {
            expect_subject_wide_refusal(&reject, "memory.reflections.list");
        }
        SpokeResult::Ok(_) => {
            panic!("subject-wide list must not serve under a context-restricted grant")
        }
    }
    match provider
        .generate_reflection(
            &grantee,
            dto(json!({
                "space_id": space,
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
                "expected_state_digest": STATE_DIGEST,
                "idempotency_key": "c075-generate",
            })),
        )
        .await
    {
        SpokeResult::Reject(reject) => {
            expect_subject_wide_refusal(&reject, "memory.reflections.generate");
        }
        SpokeResult::Ok(_) => {
            panic!("subject-wide generate must not serve under a context-restricted grant")
        }
    }

    // Coverage control: the same grantee's context-scoped selection is inside
    // the grant's reach, so it must not answer the C-075 refusal. The admitted
    // request serves the (empty) basis; the refusal is the grant's reach, not
    // the family.
    let scoped = provider
        .get_reflection_state(
            &grantee,
            dto(json!({
                "space_id": space,
                "subject_id": SUBJECT,
                "selection": { "mode": "selected_contexts", "context_ids": [CONTEXT] },
            })),
        )
        .await;
    match &scoped {
        SpokeResult::Reject(reject) => assert_ne!(
            reject.code,
            SpokeRejectCode::NotAuthorized,
            "a context-scoped selection is covered by the restricted grant: {reject:?}"
        ),
        SpokeResult::Ok(_) => {}
    }

    // Coverage control: the admin's unrestricted `admin` grant implies
    // `reflect` within its coverage, so the same subject-wide selection is not
    // coverage-refused for the admin.
    let admin_wide = provider
        .get_reflection_state(
            &admin,
            dto(json!({
                "space_id": space,
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
            })),
        )
        .await;
    match &admin_wide {
        SpokeResult::Reject(reject) => assert_ne!(
            reject.code,
            SpokeRejectCode::NotAuthorized,
            "an unrestricted admin grant implies reflect within its coverage: {reject:?}"
        ),
        SpokeResult::Ok(_) => {}
    }
}

// ── Task 3: the read path (C-073, C-042, C-076, C-071, C-077) ───────────

/// The subject the Task-3 rows read; every row ensures it before writing.
const T3_SUBJECT: &str = "sbj_t3_reads";
/// The unrestricted grantee that holds a plain `reflect` grant.
const T3_GRANTEE: &str = "ctr_t3_grantee";
/// The placement context and provenance source the seeded rows use.
const T3_CONTEXT_A: &str = "ctx_t3_a";
const T3_CONTEXT_B: &str = "ctx_t3_b";
const T3_SOURCE_A: &str = "src_t3_a";
const T3_SOURCE_B: &str = "src_t3_b";

/// Unwrap one accepted op result to its wire JSON; a reject fails the row.
fn served<T: serde::Serialize>(result: SpokeResult<T>, what: &str) -> Value {
    match result {
        SpokeResult::Ok(response) => serde_json::to_value(&response).expect("response serializes"),
        SpokeResult::Reject(reject) => panic!("{what} is served: {reject:?}"),
    }
}

/// Assert the published CAS token shape: 64 lowercase hex.
fn expect_state_digest(body: &Value, what: &str) -> String {
    let digest = body["state_digest"]
        .as_str()
        .unwrap_or_else(|| panic!("{what} publishes a state digest: {body}"));
    assert!(
        digest.len() == 64
            && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            && digest.bytes().all(|byte| !byte.is_ascii_uppercase()),
        "{what} publishes a 64-lowercase-hex digest: {digest}"
    );
    digest.to_owned()
}

/// One committed `reflect` grant, unrestricted over the subject: the
/// unrestricted wire variant carries no `context_ids` and no `shared_access`.
async fn grant_subject_all_reflect(provider: &LocalMemoryProvider, admin: &schemas::Principal) {
    let receipt = served(
        provider
            .grant(
                admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "principal_id": T3_GRANTEE,
                    "capabilities": ["reflect"],
                    "idempotency_key": "t3-reflect-subject-all",
                })),
            )
            .await,
        "the subject-wide reflect grant",
    );
    assert_eq!(
        receipt["status"],
        json!("committed"),
        "the reflect grant commits: {receipt}"
    );
}

/// Ensure the row's subject exists, then return the principal pair.
async fn t3_setup(provider: &LocalMemoryProvider) -> (schemas::Principal, schemas::Principal) {
    let admin = principal(ADMIN);
    let grantee = principal(T3_GRANTEE);
    let ensured = served(
        provider
            .ensure_subject(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                })),
            )
            .await,
        "the subject ensure",
    );
    assert_eq!(ensured["status"], json!("committed"), "{ensured}");
    (admin, grantee)
}

/// One context-scoped record write; returns the committed record id.
async fn t3_record(
    provider: &LocalMemoryProvider,
    admin: &schemas::Principal,
    context: &str,
    source: &str,
    text: &str,
) -> String {
    let receipt = served(
        provider
            .create_record(
                admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "kind": "note",
                    "content": { "text": text },
                    "scope": { "visibility": "context", "context_id": context },
                    "source_context_ids": [source],
                    "idempotency_key": format!("t3-record-{text}"),
                })),
            )
            .await,
        "the record create",
    );
    receipt["result"]["record_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the create names its record: {receipt}"))
        .to_owned()
}

/// The subject's whole authorized list page, with each row's id and revision.
async fn t3_records(
    provider: &LocalMemoryProvider,
    caller: &schemas::Principal,
) -> Vec<(String, i64)> {
    let page = served(
        provider
            .list_records(
                caller,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "subject_all" },
                })),
            )
            .await,
        "the record list",
    );
    page["records"]
        .as_array()
        .expect("a record page names its rows")
        .iter()
        .map(|record| {
            (
                record["record_id"].as_str().expect("record id").to_owned(),
                record["revision"].as_i64().expect("record revision"),
            )
        })
        .collect()
}

/// The current `state` body for one selection (+ optional provenance filter).
async fn t3_state(
    provider: &LocalMemoryProvider,
    caller: &schemas::Principal,
    selection: Value,
    filter: Option<&[&str]>,
) -> Value {
    let mut request = json!({
        "space_id": provider.space_id(),
        "subject_id": T3_SUBJECT,
        "selection": selection,
    });
    if let Some(filter) = filter {
        request["filter"] = json!({ "source_context_ids": filter });
    }
    served(
        provider.get_reflection_state(caller, dto(request)).await,
        "the state read",
    )
}

async fn t3_list(
    provider: &LocalMemoryProvider,
    caller: &schemas::Principal,
    cursor: Option<&str>,
    limit: Option<u64>,
) -> Value {
    let mut request = json!({
        "space_id": provider.space_id(),
        "subject_id": T3_SUBJECT,
    });
    if let Some(cursor) = cursor {
        request["cursor"] = json!(cursor);
    }
    if let Some(limit) = limit {
        request["limit"] = json!(limit);
    }
    served(
        provider.list_reflections(caller, dto(request)).await,
        "the reflection list",
    )
}

// ── Task 6: the generation flow, the envelope halves, the grant surface ──

/// Assert one reflection refusal carries the expected code and retry class,
/// and return the reject so a row can keep reading its details.
fn expect_refusal<T: std::fmt::Debug>(
    result: SpokeResult<T>,
    code: SpokeRejectCode,
    retry_class: &str,
    what: &str,
) -> SpokeReject {
    match result {
        SpokeResult::Ok(ok) => panic!("{what} is refused {code:?}, never served: {ok:?}"),
        SpokeResult::Reject(reject) => {
            assert_eq!(reject.code, code, "{what}: {reject:?}");
            assert_eq!(
                reject
                    .details
                    .as_ref()
                    .and_then(|details| details["retry_class"].as_str()),
                Some(retry_class),
                "{what} names its retry class: {reject:?}"
            );
            reject
        }
    }
}

/// One `subject_all` generate payload for the row subject.
fn generate_payload(space: &str, digest: &str, key: &str) -> Value {
    json!({
        "space_id": space,
        "subject_id": T3_SUBJECT,
        "selection": { "mode": "subject_all" },
        "expected_state_digest": digest,
        "idempotency_key": key,
    })
}

async fn t3_generate(
    provider: &LocalMemoryProvider,
    caller: &schemas::Principal,
    digest: &str,
    key: &str,
) -> SpokeResult<schemas::MemoryReflectionsGenerateResponse> {
    provider
        .generate_reflection(
            caller,
            dto(generate_payload(provider.space_id(), digest, key)),
        )
        .await
}

/// The authored `personality` profile: the one MEM-R005 required kind.
async fn author_required_profile(provider: &LocalMemoryProvider, admin: &schemas::Principal) {
    let authored = served(
        provider
            .update_profile(
                admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "profile_kind": "personality",
                    "document": { "format": "markdown", "body": "# Basis subject" },
                    "expected_revision": null,
                    "idempotency_key": "t3-author-profile",
                })),
            )
            .await,
        "the required profile author",
    );
    assert_eq!(authored["status"], json!("committed"), "{authored}");
}

/// Seed the sufficient basis (authored profile + two records) and return its
/// CAS token: the exact shape a generate request pins.
async fn sufficient_basis(provider: &LocalMemoryProvider, admin: &schemas::Principal) -> String {
    author_required_profile(provider, admin).await;
    t3_record(
        provider,
        admin,
        T3_CONTEXT_A,
        T3_SOURCE_A,
        "generation row a",
    )
    .await;
    t3_record(
        provider,
        admin,
        T3_CONTEXT_B,
        T3_SOURCE_B,
        "generation row b",
    )
    .await;
    expect_state_digest(
        &t3_state(provider, admin, json!({ "mode": "subject_all" }), None).await,
        "the sufficient basis",
    )
}

/// The `memory.receipts.get` body of one receipt.
async fn receipt_body_of(
    provider: &LocalMemoryProvider,
    caller: &schemas::Principal,
    receipt_id: &str,
) -> Value {
    served(
        provider
            .get_receipt(
                caller,
                dto(json!({
                    "space_id": provider.space_id(),
                    "receipt_id": receipt_id,
                })),
            )
            .await,
        "the receipt read",
    )["receipt"]
        .clone()
}

/// The durable (`receipt_id`, status) rows the reflection generate op recorded,
/// read read-only over the provider's own state database — the durable-row
/// observation channel the matrix observers use. Safe inside the parked
/// window: the pending receipt is transaction 1's committed row.
async fn durable_generation_receipts(
    user_home: &std::path::Path,
    space: &str,
) -> Vec<(String, String)> {
    let db = nexus_home_layout::workspace_state_db_path(user_home, ADMIN, "default");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db)
        .read_only(true);
    let mut conn = sqlx::sqlite::SqliteConnection::connect_with(&options)
        .await
        .expect("one read-only connection to the provider's own file");
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT receipt_id, status FROM memory_receipts \
         WHERE space_id = ? AND op = 'memory.reflections.generate' ORDER BY receipt_id",
    )
    .bind(space)
    .fetch_all(&mut conn)
    .await
    .expect("the durable generation receipts read");
    conn.close().await.expect("the read-only connection closes");
    rows
}

/// The one pending receipt's id while a generation is parked: the durable
/// rows carry exactly the parked window's committed pending receipt.
async fn parked_pending_receipt_id(user_home: &std::path::Path, space: &str) -> String {
    let rows = durable_generation_receipts(user_home, space).await;
    let pending: Vec<&(String, String)> = rows
        .iter()
        .filter(|(_, status)| status == "pending")
        .collect();
    assert_eq!(
        pending.len(),
        1,
        "the parked window carries exactly one pending receipt: {rows:?}"
    );
    pending[0].0.clone()
}

/// C-073: `state` and `list` serve under `reflect` coverage — for the granted
/// peer and for the `admin` grant whose coverage implies `reflect` — publish
/// the content-free empty basis, and stay served through a space freeze (the
/// §8.2 read-only allowlist), so no basis change ever suppresses a read.
#[tokio::test]
async fn c_073_reflection_reads_succeed_with_capability_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;

    // The granted peer's empty basis, published content-free.
    let state = t3_state(&provider, &grantee, json!({ "mode": "subject_all" }), None).await;
    assert_eq!(
        state["basis"],
        json!({ "profile_kinds": [], "record_count": 0 }),
        "{state}"
    );
    assert_eq!(state["data_sufficiency"], json!("insufficient"), "{state}");
    assert_eq!(
        state["missing_required_profile_kinds"],
        json!(["personality"]),
        "the only required authored kind is missing: {state}"
    );
    assert_eq!(state["stale"], json!(false), "nothing is pinned: {state}");
    assert!(
        state.get("last_reflection_id").is_none(),
        "no envelope exists yet, so no id is published: {state}"
    );
    expect_state_digest(&state, "the state read");

    // The `admin` grant implies `reflect` within its coverage: same reads.
    let admin_state = t3_state(&provider, &admin, json!({ "mode": "subject_all" }), None).await;
    assert_eq!(
        admin_state["basis"]["record_count"],
        json!(0),
        "the admin reads the same empty basis: {admin_state}"
    );

    // Both reads stay served through a freeze (§8.2 read-only allowlist).
    let spaces = served(
        provider.list_spaces(&admin, dto(json!({}))).await,
        "the space list",
    );
    let revision = spaces["spaces"][0]["revision"]
        .as_i64()
        .expect("space revision");
    let frozen = served(
        provider
            .freeze_space(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "expected_revision": revision,
                    "idempotency_key": "t3-freeze",
                })),
            )
            .await,
        "the freeze",
    );
    assert_eq!(frozen["status"], json!("committed"), "{frozen}");
    let frozen_state = t3_state(&provider, &grantee, json!({ "mode": "subject_all" }), None).await;
    assert_eq!(
        frozen_state["state_digest"], state["state_digest"],
        "a frozen space answers the same basis: {frozen_state}"
    );
    let frozen_list = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        frozen_list["reflections"],
        json!([]),
        "the frozen listing is served empty: {frozen_list}"
    );
}

/// C-042: every basis-affecting fact class — record create, edit, delete,
/// scope-promote, profile update, and an authorization change that alters the
/// authorized set — moves the published `state_digest` at the next read, with
/// no eager invalidation writes and `stale` honestly false while nothing is
/// pinned; under a pin the same fact classes answer `stale` at the next read.
#[tokio::test]
async fn c_042_basis_changes_invalidate_reflection_state_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let selection = json!({ "mode": "subject_all" });

    let mut digests = vec![expect_state_digest(
        &t3_state(&provider, &admin, selection.clone(), None).await,
        "the empty state",
    )];

    // Create.
    let record_a = t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_A, "basis row a").await;
    let created = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(created["basis"]["record_count"], json!(1), "{created}");
    digests.push(expect_state_digest(&created, "after create"));

    // Edit.
    let revision = t3_records(&provider, &admin)
        .await
        .iter()
        .find(|(id, _)| *id == record_a)
        .expect("the created row lists")
        .1;
    let edited = served(
        provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": record_a,
                    "content": { "text": "basis row a, edited" },
                    "expected_revision": revision,
                    "idempotency_key": "t3-edit-a",
                })),
            )
            .await,
        "the record edit",
    );
    assert_eq!(edited["status"], json!("committed"), "{edited}");
    digests.push(expect_state_digest(
        &t3_state(&provider, &admin, selection.clone(), None).await,
        "after edit",
    ));

    // Second create, in another context.
    let record_b = t3_record(&provider, &admin, T3_CONTEXT_B, T3_SOURCE_B, "basis row b").await;
    let grown = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(grown["basis"]["record_count"], json!(2), "{grown}");
    digests.push(expect_state_digest(&grown, "after second create"));

    // Delete (a tombstone leaves the authorized set at read time, C-077).
    let revision = t3_records(&provider, &admin)
        .await
        .iter()
        .find(|(id, _)| *id == record_a)
        .expect("the tombstoned row is admin-visible until swept")
        .1;
    let deleted = served(
        provider
            .delete_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": record_a,
                    "expected_revision": revision,
                    "idempotency_key": "t3-delete-a",
                })),
            )
            .await,
        "the record delete",
    );
    assert_eq!(deleted["status"], json!("committed"), "{deleted}");
    let shrunk = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(shrunk["basis"]["record_count"], json!(1), "{shrunk}");
    digests.push(expect_state_digest(&shrunk, "after delete"));

    // Scope-promote: the placement is part of the selection resolution.
    let revision = t3_records(&provider, &admin)
        .await
        .iter()
        .find(|(id, _)| *id == record_b)
        .expect("the surviving row lists")
        .1;
    let promoted = served(
        provider
            .promote_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": record_b,
                    "target_scope": { "visibility": "shared" },
                    "expected_revision": revision,
                    "idempotency_key": "t3-promote-b",
                })),
            )
            .await,
        "the record promote",
    );
    assert_eq!(promoted["status"], json!("committed"), "{promoted}");
    digests.push(expect_state_digest(
        &t3_state(&provider, &admin, selection.clone(), None).await,
        "after promote",
    ));

    // Profile update.
    let profiled = served(
        provider
            .update_profile(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "profile_kind": "personality",
                    "document": { "format": "markdown", "body": "# Basis subject" },
                    "expected_revision": null,
                    "idempotency_key": "t3-profile",
                })),
            )
            .await,
        "the profile update",
    );
    assert_eq!(profiled["status"], json!("committed"), "{profiled}");
    let profile_state = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        profile_state["basis"]["profile_kinds"],
        json!(["personality"]),
        "the authored kind publishes: {profile_state}"
    );
    assert_eq!(
        profile_state["missing_required_profile_kinds"],
        json!([]),
        "no required kind is missing any more: {profile_state}"
    );
    digests.push(expect_state_digest(&profile_state, "after profile update"));

    // Authorization change that alters the authorized set: a fresh peer's
    // context-scoped basis moves when its grant's reach moves.
    let scoped_selection = json!({
        "mode": "selected_contexts",
        "context_ids": [T3_CONTEXT_A],
    });
    served(
        provider
            .grant(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "principal_id": T3_GRANTEE,
                    "capabilities": ["reflect"],
                    "context_ids": [T3_CONTEXT_A],
                    "shared_access": false,
                    "idempotency_key": "t3-reflect-ctx-a",
                })),
            )
            .await,
        "the context-scoped reflect grant",
    );
    let peer_state = t3_state(&provider, &grantee, scoped_selection, None).await;
    digests.push(expect_state_digest(&peer_state, "the peer's scoped basis"));
    assert_eq!(
        peer_state["basis"]["record_count"],
        json!(0),
        "context A holds no live row any more: {peer_state}"
    );

    // Every fact class moved the digest; nothing is pinned, so `stale` stays
    // honestly false throughout.
    for state in [created, grown, shrunk, profile_state] {
        assert_eq!(state["stale"], json!(false), "{state}");
    }
    let unique: std::collections::HashSet<&String> = digests.iter().collect();
    assert_eq!(
        unique.len(),
        digests.len(),
        "every basis-affecting fact changed the digest: {digests:?}"
    );

    // The pinned half (Task 6): an envelope pins this basis, and the same
    // fact classes now answer `stale` honestly at the next read — computed,
    // never stored, with no eager invalidation writes.
    let pinned_digest = expect_state_digest(
        &t3_state(&provider, &admin, selection.clone(), None).await,
        "the pin source",
    );
    let receipt = served(
        t3_generate(&provider, &admin, &pinned_digest, "t3-pin-generation").await,
        "the pinned generation",
    );
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");
    let pinned = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        pinned["stale"],
        json!(false),
        "the pin matches the current basis: {pinned}"
    );
    let revision = t3_records(&provider, &admin)
        .await
        .iter()
        .find(|(id, _)| *id == record_b)
        .expect("the pinned-basis row lists")
        .1;
    served(
        provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": record_b,
                    "content": { "text": "basis row b, edited under the pin" },
                    "expected_revision": revision,
                    "idempotency_key": "t3-pin-edit-b",
                })),
            )
            .await,
        "the pinned-basis edit",
    );
    let moved = t3_state(&provider, &admin, selection, None).await;
    assert_eq!(
        moved["stale"],
        json!(true),
        "the pin is stale at the next read, honestly recomputed: {moved}"
    );
    assert_ne!(
        moved["state_digest"], pinned_digest,
        "the edit moved the digest under the pin: {moved}"
    );
}

/// C-076: the published basis is honest about what it reads — authored kinds,
/// exact counts, the sufficiency verdict over the records (profiles never flip
/// it), a deterministic digest over unchanged state, and a digest that binds
/// the selection/filter resolution. `stale` is recomputed at read time; with
/// nothing pinned it reports false even as the basis moves.
#[tokio::test]
async fn c_076_pinned_basis_publishes_and_reports_honest_staleness_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, _) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let selection = json!({ "mode": "subject_all" });

    // A profile alone never flips the sufficiency verdict: the verdict reads
    // the authorized records.
    let profiled = served(
        provider
            .update_profile(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "profile_kind": "personality",
                    "document": { "format": "markdown", "body": "# Honest basis" },
                    "expected_revision": null,
                    "idempotency_key": "t3c076-profile",
                })),
            )
            .await,
        "the profile update",
    );
    assert_eq!(profiled["status"], json!("committed"), "{profiled}");
    let profile_only = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        profile_only["basis"],
        json!({ "profile_kinds": ["personality"], "record_count": 0 }),
        "{profile_only}"
    );
    assert_eq!(
        profile_only["data_sufficiency"],
        json!("insufficient"),
        "{profile_only}"
    );
    assert_eq!(
        profile_only["missing_required_profile_kinds"],
        json!([]),
        "{profile_only}"
    );

    // Two rows in different provenance sources: the count is exact, two
    // consecutive reads agree, and the filter resolution is part of the
    // digest's input.
    t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_A, "honest row a").await;
    t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_B, "honest row b").await;
    let full = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(full["basis"]["record_count"], json!(2), "{full}");
    assert_eq!(full["data_sufficiency"], json!("sufficient"), "{full}");
    let digest = expect_state_digest(&full, "the full state");
    let repeat = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        repeat["state_digest"], full["state_digest"],
        "unchanged state recomputes the same digest: {repeat}"
    );
    let filtered = t3_state(&provider, &admin, selection.clone(), Some(&[T3_SOURCE_A])).await;
    let filtered_digest = expect_state_digest(&filtered, "the filtered state");
    assert_eq!(filtered["basis"]["record_count"], json!(1), "{filtered}");
    assert_ne!(
        digest, filtered_digest,
        "the filter resolution is part of the digest: {full} vs {filtered}"
    );

    // A basis-affecting edit moves the digest; nothing is pinned, so the
    // recomputed staleness stays honestly false.
    let (record_id, revision) = t3_records(&provider, &admin).await.remove(0);
    let edited = served(
        provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": record_id,
                    "content": { "text": "honest row a, edited" },
                    "expected_revision": revision,
                    "idempotency_key": "t3c076-edit",
                })),
            )
            .await,
        "the record edit",
    );
    assert_eq!(edited["status"], json!("committed"), "{edited}");
    let after_edit = t3_state(&provider, &admin, selection, None).await;
    assert_ne!(
        after_edit["state_digest"], digest,
        "the edit moved the digest: {after_edit}"
    );
    assert_eq!(after_edit["stale"], json!(false), "{after_edit}");

    // The pinned half (Task 6): pin this basis, then move it — the envelope
    // and the state read report the recomputed staleness honestly; the stored
    // write-time cache is never consulted.
    let pin_digest = expect_state_digest(&after_edit, "the pin source");
    let receipt = served(
        t3_generate(&provider, &admin, &pin_digest, "t3c076-pin").await,
        "the pinned generation",
    );
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");
    let fresh = t3_list(&provider, &admin, None, None).await;
    assert_eq!(
        fresh["reflections"][0]["stale"],
        json!(false),
        "the fresh pin is not stale: {fresh}"
    );
    let (record_id, revision) = t3_records(&provider, &admin).await.remove(0);
    served(
        provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": record_id,
                    "content": { "text": "honest row, edited under the pin" },
                    "expected_revision": revision,
                    "idempotency_key": "t3c076-pin-edit",
                })),
            )
            .await,
        "the pinned-basis edit",
    );
    let stale_page = t3_list(&provider, &admin, None, None).await;
    assert_eq!(
        stale_page["reflections"][0]["stale"],
        json!(true),
        "the envelope reports the recomputed staleness: {stale_page}"
    );
    let stale_state = t3_state(&provider, &admin, json!({ "mode": "subject_all" }), None).await;
    assert_eq!(
        stale_state["stale"],
        json!(true),
        "the state read answers the same recomputation: {stale_state}"
    );
    assert_ne!(
        stale_state["state_digest"], pin_digest,
        "the pin no longer matches: {stale_state}"
    );
}

/// C-071: the listing contract — an empty page names no end-of-results
/// cursor, the limit bound is honored, the cursor is opaque and bound to the
/// op that minted it, and a foreign or malformed cursor answers
/// `CURSOR_INVALID` (terminal). The terminal-receipt half: a committed
/// generation's receipt names its reflection and carries no envelope body,
/// `receipts.get` serves it verbatim, and two envelopes page with a bound and
/// an absent end-of-results cursor.
#[tokio::test]
async fn c_071_terminal_receipt_and_pinned_basis_listing_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;

    // Empty listing: no rows, no cursor, and the limit bound is honored.
    let page = t3_list(&provider, &grantee, None, Some(1)).await;
    assert_eq!(page["reflections"], json!([]), "{page}");
    assert!(
        page.get("next_cursor").is_none(),
        "an empty page is the end of results: {page}"
    );

    // A cursor minted by another op is foreign here: cursors are opaque and
    // bound to the query that minted them (MEM-R026). Two record rows let the
    // record listing mint a real continuation cursor.
    t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_A, "listed row a").await;
    t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_A, "listed row b").await;
    let records_page = served(
        provider
            .list_records(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "limit": 1,
                })),
            )
            .await,
        "the record list page",
    );
    let foreign_cursor = records_page["next_cursor"]
        .as_str()
        .expect("a two-row subject pages, so the record listing mints a cursor");

    // The reflection listing under a foreign cursor is refused
    // `CURSOR_INVALID` whether the text parses as a cursor or not: the
    // binding is by content.
    for cursor in ["not-a-cursor", foreign_cursor] {
        let refused = match provider
            .list_reflections(
                &grantee,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "cursor": cursor,
                })),
            )
            .await
        {
            SpokeResult::Reject(reject) => reject,
            SpokeResult::Ok(_) => {
                panic!("a foreign cursor must not serve a reflection page: {cursor}")
            }
        };
        assert_eq!(
            refused.code,
            SpokeRejectCode::CursorInvalid,
            "a foreign cursor answers CURSOR_INVALID: {refused:?}"
        );
        assert_eq!(
            refused
                .details
                .as_ref()
                .and_then(|details| details["retry_class"].as_str()),
            Some("terminal"),
            "the cursor refusal names its retry class: {refused:?}"
        );
    }

    // The terminal-receipt half (Task 6): a committed generation's receipt
    // names its reflection and carries no envelope body; `receipts.get`
    // serves the same terminal receipt verbatim; two envelopes page under the
    // bound with an absent end-of-results cursor.
    author_required_profile(&provider, &admin).await;
    let digest = expect_state_digest(
        &t3_state(&provider, &admin, json!({ "mode": "subject_all" }), None).await,
        "the two-row state",
    );
    let first = served(
        t3_generate(&provider, &grantee, &digest, "t3c071-pin-a").await,
        "the first generation",
    );
    assert_eq!(first["status"], json!("committed"), "{first}");
    let second = served(
        t3_generate(&provider, &grantee, &digest, "t3c071-pin-b").await,
        "the second generation",
    );
    assert_eq!(second["status"], json!("committed"), "{second}");
    for receipt in [&first, &second] {
        assert!(
            receipt["result"]["reflection_id"].is_string(),
            "a committed receipt names its reflection: {receipt}"
        );
        assert!(
            receipt["result"].get("body").is_none(),
            "the envelope body is served by the listing, never the receipt: {receipt}"
        );
        let stored = receipt_body_of(
            &provider,
            &grantee,
            receipt["receipt_id"].as_str().expect("receipt id"),
        )
        .await;
        assert_eq!(stored["status"], json!("committed"), "{stored}");
        assert_eq!(
            stored["result"]["reflection_id"], receipt["result"]["reflection_id"],
            "the receipt channel serves the recorded outcome: {stored}"
        );
    }

    let page_one = t3_list(&provider, &grantee, None, Some(1)).await;
    let rows = page_one["reflections"].as_array().expect("envelope page");
    assert_eq!(rows.len(), 1, "the bound limits the page: {page_one}");
    let cursor = page_one["next_cursor"]
        .as_str()
        .expect("a partial page names its continuation");
    let page_two = t3_list(&provider, &grantee, Some(cursor), None).await;
    let rest = page_two["reflections"].as_array().expect("envelope page");
    assert_eq!(
        rest.len(),
        1,
        "the continuation serves the rest: {page_two}"
    );
    assert!(
        page_two.get("next_cursor").is_none(),
        "the exhausted listing reports no next cursor: {page_two}"
    );
    let mut served_ids: Vec<&str> = rows
        .iter()
        .chain(rest.iter())
        .map(|envelope| envelope["reflection_id"].as_str().expect("reflection id"))
        .collect();
    served_ids.sort_unstable();
    let mut expected_ids = vec![
        first["result"]["reflection_id"]
            .as_str()
            .expect("reflection id"),
        second["result"]["reflection_id"]
            .as_str()
            .expect("reflection id"),
    ];
    expected_ids.sort_unstable();
    assert_eq!(
        served_ids, expected_ids,
        "the two pages serve both envelopes"
    );
}

/// C-077: a tombstoned row leaves the authorized basis at read time — the
/// published count drops, the digest moves, and the same exclusion governs
/// the record listing. The envelope-exclusion half: a reflection pinned over
/// the subject's full record set is excluded from the listing once the pin's
/// only delta is tombstoning, and stops surfacing as the state's last
/// reflection — while the row itself is immutable (no deletion path).
#[tokio::test]
async fn c_077_tombstoned_bases_excluded_from_list_get_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let selection = json!({ "mode": "subject_all" });

    // Seed the full basis (profile + two records), pin it, then tombstone one
    // record: the pin's only delta is that tombstone.
    author_required_profile(&provider, &admin).await;
    let record_a = t3_record(
        &provider,
        &admin,
        T3_CONTEXT_A,
        T3_SOURCE_A,
        "tombstone row a",
    )
    .await;
    t3_record(
        &provider,
        &admin,
        T3_CONTEXT_A,
        T3_SOURCE_A,
        "tombstone row b",
    )
    .await;
    let before = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(before["basis"]["record_count"], json!(2), "{before}");
    let digest = expect_state_digest(&before, "the two-row state");
    let receipt = served(
        t3_generate(&provider, &grantee, &digest, "t3c077-pin").await,
        "the pinned generation",
    );
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");
    let reflection_id = receipt["result"]["reflection_id"]
        .as_str()
        .expect("the committed generation names its reflection");
    let pinned_page = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        pinned_page["reflections"][0]["reflection_id"],
        json!(reflection_id),
        "the envelope serves while its basis is live: {pinned_page}"
    );
    assert_eq!(
        pinned_page["reflections"][0]["basis"]["record_count"],
        json!(2),
        "the pin carries the full source set: {pinned_page}"
    );

    // Delete one basis record; the read recomputes the basis without it.
    let revision = t3_records(&provider, &admin)
        .await
        .iter()
        .find(|(id, _)| *id == record_a)
        .expect("the row lists before its tombstone")
        .1;
    served(
        provider
            .delete_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": record_a,
                    "expected_revision": revision,
                    "idempotency_key": "t3c077-delete",
                })),
            )
            .await,
        "the record delete",
    );

    let after = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        after["basis"]["record_count"],
        json!(1),
        "the tombstoned row left the authorized set: {after}"
    );
    assert_ne!(
        after["state_digest"], digest,
        "the tombstone moved the digest at the next read: {after}"
    );
    let listed = t3_records(&provider, &admin).await;
    assert_eq!(
        listed.len(),
        1,
        "the same exclusion governs the record listing: {listed:?}"
    );
    assert!(
        listed.iter().all(|(id, _)| *id != record_a),
        "the tombstoned row is excluded: {listed:?}"
    );

    // The envelope's pinned source set went tombstone-only: it leaves the
    // listing and stops surfacing as the state's last reflection — the
    // deleted content must not leak through the derived envelope.
    let excluded = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        excluded["reflections"],
        json!([]),
        "a tombstone-only pinned set leaves the listing: {excluded}"
    );
    let excluded_state = t3_state(&provider, &admin, selection, None).await;
    assert_eq!(
        excluded_state["basis"]["record_count"],
        json!(1),
        "the live basis still serves: {excluded_state}"
    );
    assert!(
        excluded_state.get("last_reflection_id").is_none(),
        "the excluded envelope no longer surfaces as the last reflection: {excluded_state}"
    );
    assert_eq!(
        excluded_state["stale"],
        json!(false),
        "with no visible envelope there is no pin to go stale: {excluded_state}"
    );
}

/// C-070: a plain `reflect` grant carries the whole generation flow — the
/// pending receipt publishes before any generation runs (observable while the
/// generation is parked: the receipt is pending and the listing is still
/// empty), exactly one terminal outcome commits, the same key replays that
/// outcome without a second generation, and a changed payload under the same
/// key answers `IDEMPOTENCY_CONFLICT` before any effect.
#[tokio::test]
async fn c_070_reflect_grant_committed_and_pending_generation_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let digest = sufficient_basis(&provider, &admin).await;
    let space = provider.space_id().to_owned();
    let home_path = home.path().to_path_buf();

    // The parked window: pending published, generation not run yet.
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Barrier::new(2));
    provider.park_next_generation(Arc::clone(&entered), Arc::clone(&release));
    let generation = t3_generate(&provider, &grantee, &digest, "t4-c070-parked");
    let window = async {
        entered.notified().await;
        let pending_id = parked_pending_receipt_id(&home_path, &space).await;
        let pending = receipt_body_of(&provider, &grantee, &pending_id).await;
        assert_eq!(pending["status"], json!("pending"), "{pending}");
        assert_eq!(
            pending["op"],
            json!("memory.reflections.generate"),
            "{pending}"
        );
        assert!(
            pending.get("result").is_none(),
            "a pending receipt carries no result: {pending}"
        );
        assert!(
            pending.get("completed_at").is_none(),
            "a pending receipt carries no terminal stamp: {pending}"
        );
        let page = t3_list(&provider, &grantee, None, None).await;
        assert_eq!(
            page["reflections"],
            json!([]),
            "no envelope exists before generation runs: {page}"
        );
        release.wait().await;
    };
    let (generation, ()) = tokio::join!(generation, window);
    let receipt = served(generation, "the parked generation");
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");

    // Exactly one terminal outcome; the same key replays it verbatim.
    let envelope_page = t3_list(&provider, &grantee, None, None).await;
    let envelopes = envelope_page["reflections"]
        .as_array()
        .expect("envelope page");
    assert_eq!(
        envelopes.len(),
        1,
        "one generation wrote one envelope: {envelope_page}"
    );
    let replay = served(
        t3_generate(&provider, &grantee, &digest, "t4-c070-parked").await,
        "the same-key replay",
    );
    assert_eq!(replay["status"], json!("committed"), "{replay}");
    assert_eq!(
        replay["receipt_id"], receipt["receipt_id"],
        "the replay serves the recorded receipt: {replay}"
    );
    let after_replay = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        after_replay["reflections"], envelope_page["reflections"],
        "a replay never generates again"
    );

    // Changed payload under the same key: a pre-effect conflict.
    expect_refusal(
        provider
            .generate_reflection(
                &grantee,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "selected_contexts", "context_ids": [T3_CONTEXT_A] },
                    "expected_state_digest": digest,
                    "idempotency_key": "t4-c070-parked",
                })),
            )
            .await,
        SpokeRejectCode::IdempotencyConflict,
        "terminal",
        "the same key with a changed payload",
    );
}

/// C-072: the generated envelope carries the published §7.27 shape — the
/// pinned selection verbatim (absent optionals never null-substituted), the
/// pinned basis matching the generating state's digest, a markdown body, the
/// generator's policy version, the cost, and honest `stale: false`; the
/// receipt names the reflection and the cost but carries no envelope body;
/// and the pin never silently rewrites itself when the basis moves later.
#[tokio::test]
async fn c_072_generated_envelope_shape_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let digest = sufficient_basis(&provider, &admin).await;

    let receipt = served(
        t3_generate(&provider, &grantee, &digest, "t4-c072").await,
        "the generation",
    );
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");
    assert_eq!(
        receipt["result"]
            .as_object()
            .map(|result| result.keys().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["cost", "reflection_id"]),
        "the receipt names the reflection and the cost, never a body: {receipt}"
    );

    let page = t3_list(&provider, &grantee, None, None).await;
    let envelope = &page["reflections"][0];
    assert_eq!(
        envelope["reflection_id"], receipt["result"]["reflection_id"],
        "the listing serves the generated envelope: {page}"
    );
    assert_eq!(
        envelope["selection"],
        json!({ "mode": "subject_all" }),
        "the selection is pinned verbatim: {page}"
    );
    assert!(
        envelope.get("filter").is_none(),
        "an absent filter is absent, never null: {page}"
    );
    assert!(
        envelope.get("cognition_filter").is_none(),
        "an absent cognition filter is absent, never null: {page}"
    );
    assert_eq!(envelope["basis"]["record_count"], json!(2), "{page}");
    assert_eq!(
        envelope["basis"]["profile_revisions"],
        json!({ "personality": 1 }),
        "the profile revisions are pinned: {page}"
    );
    assert_eq!(
        envelope["basis"]["state_digest"],
        json!(digest),
        "the pin matches the generating state's digest: {page}"
    );
    assert!(
        envelope["basis"]["source_set_digest"].is_string(),
        "the source set digest is pinned: {page}"
    );
    assert_eq!(envelope["body"]["format"], json!("markdown"), "{page}");
    assert!(
        !envelope["body"]["body"]
            .as_str()
            .expect("a body")
            .is_empty(),
        "the envelope carries a real body: {page}"
    );
    assert_eq!(
        envelope["generator"]["engine_policy_version"],
        served(
            provider.describe(&admin, dto(json!({}))).await,
            "the describe"
        )["engine_policy_version"],
        "the generator stamps the served policy version: {page}"
    );
    assert_eq!(
        envelope["cost"],
        json!({ "unit_kind": "reflection", "units": 1.0 }),
        "{page}"
    );
    assert_eq!(envelope["extensions"], json!({}), "{page}");
    assert_eq!(envelope["revision"], json!(1), "{page}");
    assert_eq!(envelope["schema_version"], json!(1), "{page}");
    assert_eq!(envelope["stale"], json!(false), "{page}");

    // The pin never silently rewrites: a later basis-affecting edit moves the
    // state digest, and the envelope still carries the basis it was generated
    // over — honestly stale, never rewritten.
    let (record_id, revision) = t3_records(&provider, &admin).await.remove(0);
    served(
        provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": record_id,
                    "content": { "text": "generation row, edited under the pin" },
                    "expected_revision": revision,
                    "idempotency_key": "t4-c072-edit",
                })),
            )
            .await,
        "the pinned-basis edit",
    );
    let after = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        after["reflections"][0]["basis"]["profile_revisions"],
        json!({ "personality": 1 }),
        "the pinned profile revisions never rewrite: {after}"
    );
    assert_eq!(
        after["reflections"][0]["basis"]["state_digest"],
        json!(digest),
        "the pinned state digest never rewrites: {after}"
    );
    assert_eq!(
        after["reflections"][0]["stale"],
        json!(true),
        "the pin is honestly stale: {after}"
    );
    let state = t3_state(&provider, &admin, json!({ "mode": "subject_all" }), None).await;
    assert_ne!(
        state["state_digest"],
        json!(digest),
        "the basis move is visible at the next read: {state}"
    );
    assert_eq!(
        state["stale"],
        json!(true),
        "the pin is honestly stale: {state}"
    );
}

/// C-078: a generation failure is honest and never charges — the one-shot
/// fault commits a `failed` receipt (retryable `INTERNAL_ERROR`, "interrupted
/// before any effect", no result, no cost) that both the call and
/// `receipts.get` report; the same key replays the recorded failure; a fresh
/// key runs exactly one clean generation whose committed envelope carries the
/// only cost, while the failed receipt stays failed and result-free.
#[tokio::test]
async fn c_078_generation_failure_honesty_and_no_double_charge_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let digest = sufficient_basis(&provider, &admin).await;

    provider.inject_generation_fault();
    let failed = expect_refusal(
        t3_generate(&provider, &grantee, &digest, "t4-c078-failed").await,
        SpokeRejectCode::InternalError,
        "retryable",
        "the interrupted generation",
    );
    assert!(
        failed.message.contains("interrupted before any effect"),
        "the refusal reports the honest interruption: {failed:?}"
    );
    // The durable rows name the failed receipt; the receipt channel serves it.
    let space = provider.space_id().to_owned();
    let rows = durable_generation_receipts(home.path(), &space).await;
    let receipt_id = rows
        .iter()
        .find(|(_, status)| status == "failed")
        .map(|(receipt_id, _)| receipt_id.clone());
    let Some(receipt_id) = receipt_id else {
        panic!("the interrupted generation recorded its failure: {rows:?}");
    };
    let stored = receipt_body_of(&provider, &grantee, &receipt_id).await;
    assert_eq!(stored["status"], json!("failed"), "{stored}");
    assert_eq!(stored["error"]["code"], json!("INTERNAL_ERROR"), "{stored}");
    assert_eq!(
        stored["error"]["details"]["retry_class"],
        json!("retryable"),
        "{stored}"
    );
    assert!(
        stored["error"]["message"]
            .as_str()
            .expect("a failure message")
            .contains("interrupted before any effect"),
        "{stored}"
    );
    assert!(
        stored.get("result").is_none(),
        "a failed receipt carries no result and no cost: {stored}"
    );

    // The same key replays the recorded failure — never a re-run.
    let replayed = expect_refusal(
        t3_generate(&provider, &grantee, &digest, "t4-c078-failed").await,
        SpokeRejectCode::InternalError,
        "retryable",
        "the failed replay",
    );
    assert_eq!(
        replayed.message, failed.message,
        "the replay serves the same recorded failure: {replayed:?}"
    );
    let page = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        page["reflections"],
        json!([]),
        "the failed generation wrote no effect: {page}"
    );

    // A fresh key: the fault is consumed, exactly one clean generation commits.
    let committed = served(
        t3_generate(&provider, &grantee, &digest, "t4-c078-fresh").await,
        "the fresh generation",
    );
    assert_eq!(committed["status"], json!("committed"), "{committed}");
    assert_eq!(
        committed["result"]["cost"],
        json!({ "unit_kind": "reflection", "units": 1.0 }),
        "the only charge rides the committed outcome: {committed}"
    );
    let settled = t3_list(&provider, &grantee, None, None).await;
    let envelopes = settled["reflections"].as_array().expect("envelope page");
    assert_eq!(
        envelopes.len(),
        1,
        "one fresh generation, one envelope: {settled}"
    );
    assert_eq!(
        envelopes[0]["cost"],
        json!({ "unit_kind": "reflection", "units": 1.0 }),
        "{settled}"
    );
    let still_failed = receipt_body_of(&provider, &grantee, &receipt_id).await;
    assert_eq!(still_failed["status"], json!("failed"), "{still_failed}");
    assert!(
        still_failed.get("result").is_none(),
        "the failed receipt was never charged: {still_failed}"
    );
}

/// C-079: the sufficiency verdict is the provider's own policy, reported by
/// `state` and enforced by `generate` — an empty basis refuses
/// `PROFILE_INCOMPLETE` (the required kind is missing), an authored profile
/// alone stays `INSUFFICIENT_DATA`, and records arriving flip the verdict to
/// `sufficient`, where the same flow commits.
#[tokio::test]
async fn c_079_insufficient_data_policy_outcomes_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let selection = json!({ "mode": "subject_all" });

    // 1. Empty basis: verdict insufficient, required kind missing; generate
    //    refuses `PROFILE_INCOMPLETE` naming the missing kinds.
    let empty = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(empty["data_sufficiency"], json!("insufficient"), "{empty}");
    assert_eq!(
        empty["missing_required_profile_kinds"],
        json!(["personality"]),
        "{empty}"
    );
    let empty_digest = expect_state_digest(&empty, "the empty state");
    let missing = expect_refusal(
        t3_generate(&provider, &grantee, &empty_digest, "t4-c079-missing").await,
        SpokeRejectCode::ProfileIncomplete,
        "terminal",
        "the empty-basis generation",
    );
    assert_eq!(
        missing
            .details
            .as_ref()
            .and_then(|details| details["missing"].as_array()),
        Some(&vec![json!("personality")]),
        "the refusal names the missing kinds: {missing:?}"
    );

    // 2. Authored profile, zero records: the verdict reads the records, so
    //    generation refuses `INSUFFICIENT_DATA`.
    author_required_profile(&provider, &admin).await;
    let profiled = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        profiled["data_sufficiency"],
        json!("insufficient"),
        "{profiled}"
    );
    assert_eq!(
        profiled["missing_required_profile_kinds"],
        json!([]),
        "{profiled}"
    );
    let profiled_digest = expect_state_digest(&profiled, "the profile-only state");
    expect_refusal(
        t3_generate(&provider, &grantee, &profiled_digest, "t4-c079-empty").await,
        SpokeRejectCode::InsufficientData,
        "terminal",
        "the profile-only generation",
    );

    // 3. Records arrive: sufficient, and the same flow commits.
    t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_A, "policy row a").await;
    t3_record(&provider, &admin, T3_CONTEXT_B, T3_SOURCE_B, "policy row b").await;
    let full = t3_state(&provider, &admin, selection, None).await;
    assert_eq!(full["data_sufficiency"], json!("sufficient"), "{full}");
    assert_eq!(full["missing_required_profile_kinds"], json!([]), "{full}");
    let full_digest = expect_state_digest(&full, "the sufficient state");
    let receipt = served(
        t3_generate(&provider, &grantee, &full_digest, "t4-c079-sufficient").await,
        "the sufficient generation",
    );
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");
}

/// C-015 (reflection half): the long-running generation publishes its pending
/// receipt before any effect — `memory.receipts.get` serves the §7.0.3 poll
/// answer (pending, no result, no terminal stamp) while the operation runs,
/// and the same receipt resolves to the committed terminal outcome afterwards.
#[tokio::test]
async fn c_015_pending_receipt_long_running_operation_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let digest = sufficient_basis(&provider, &admin).await;
    let space = provider.space_id().to_owned();
    let home_path = home.path().to_path_buf();

    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Barrier::new(2));
    provider.park_next_generation(Arc::clone(&entered), Arc::clone(&release));
    let generation = t3_generate(&provider, &grantee, &digest, "t4-c015");
    let window = async {
        entered.notified().await;
        let pending_id = parked_pending_receipt_id(&home_path, &space).await;
        let pending = receipt_body_of(&provider, &grantee, &pending_id).await;
        assert_eq!(pending["status"], json!("pending"), "{pending}");
        assert_eq!(
            pending["op"],
            json!("memory.reflections.generate"),
            "{pending}"
        );
        assert_eq!(
            pending["requester_principal_id"],
            json!(T3_GRANTEE),
            "{pending}"
        );
        assert!(pending["payload_digest"].is_string(), "{pending}");
        assert!(pending["created_at"].is_string(), "{pending}");
        assert!(
            pending.get("result").is_none(),
            "the poll answer carries no result yet: {pending}"
        );
        assert!(
            pending.get("completed_at").is_none(),
            "the poll answer carries no terminal stamp: {pending}"
        );
        release.wait().await;
        pending_id
    };
    let (generation, pending_id) = tokio::join!(generation, window);
    let receipt = served(generation, "the long-running generation");
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");
    assert_eq!(
        receipt["receipt_id"],
        json!(pending_id),
        "the parked receipt settles in place: {receipt}"
    );
    assert!(receipt["completed_at"].is_string(), "{receipt}");
    let terminal = receipt_body_of(&provider, &grantee, &pending_id).await;
    assert_eq!(terminal["status"], json!("committed"), "{terminal}");
    assert_eq!(
        terminal["result"]["reflection_id"], receipt["result"]["reflection_id"],
        "the receipt channel serves the recorded outcome: {terminal}"
    );
}

/// C-090: the `reflect` capability rides the existing grant surface with no
/// new DTO — minted through the unchanged issuance path, listed with its
/// coverage, serving inside its reach (`selected_contexts`), refusing outside
/// it (`shared_only` without shared access), revocable through the unchanged
/// revocation, while the unrestricted `admin` grant keeps implying `reflect`.
#[tokio::test]
async fn c_090_reflection_grant_surfaces_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;

    // Minted through the unchanged non-amplification path.
    let receipt = served(
        provider
            .grant(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "principal_id": T3_GRANTEE,
                    "capabilities": ["reflect"],
                    "context_ids": [T3_CONTEXT_A],
                    "shared_access": false,
                    "idempotency_key": "t4-c090-mint",
                })),
            )
            .await,
        "the reflect grant",
    );
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");
    assert_eq!(receipt["op"], json!("memory.grants.grant"), "{receipt}");

    // Listed with its coverage through the unchanged grants listing.
    let grants = served(
        provider
            .list_grants(&admin, dto(json!({ "space_id": provider.space_id() })))
            .await,
        "the grants listing",
    );
    let listed = grants["grants"]
        .as_array()
        .expect("grant page")
        .iter()
        .find(|grant| grant["principal_id"] == json!(T3_GRANTEE))
        .expect("the reflect grant lists")
        .clone();
    assert_eq!(listed["capabilities"], json!(["reflect"]), "{listed}");
    assert_eq!(listed["context_ids"], json!([T3_CONTEXT_A]), "{listed}");
    assert_eq!(listed["shared_access"], json!(false), "{listed}");

    // Inside the reach: the covered selection serves.
    let covered = t3_state(
        &provider,
        &grantee,
        json!({ "mode": "selected_contexts", "context_ids": [T3_CONTEXT_A] }),
        None,
    )
    .await;
    assert_eq!(
        covered["basis"]["record_count"],
        json!(0),
        "the covered selection serves: {covered}"
    );
    expect_state_digest(&covered, "the covered read");

    // Outside the reach: `shared_only` under a restricted grant without
    // shared access is the coverage refusal, terminal.
    expect_refusal(
        provider
            .get_reflection_state(
                &grantee,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "shared_only" },
                })),
            )
            .await,
        SpokeRejectCode::NotAuthorized,
        "terminal",
        "shared_only under a restricted grant",
    );

    // Revocable through the unchanged revocation: coverage is gone.
    let revoked = served(
        provider
            .revoke_grant(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "grant_id": listed["grant_id"],
                    "expected_revision": 1,
                    "idempotency_key": "t4-c090-revoke",
                })),
            )
            .await,
        "the revocation",
    );
    assert_eq!(revoked["status"], json!("committed"), "{revoked}");
    expect_refusal(
        provider
            .get_reflection_state(
                &grantee,
                dto(json!({
                    "space_id": provider.space_id(),
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "selected_contexts", "context_ids": [T3_CONTEXT_A] },
                })),
            )
            .await,
        SpokeRejectCode::NotAuthorized,
        "terminal",
        "the revoked selection",
    );

    // The unrestricted `admin` grant keeps implying `reflect`.
    let implied = t3_state(&provider, &admin, json!({ "mode": "subject_all" }), None).await;
    assert!(
        implied["state_digest"].is_string(),
        "the admin grant implies reflect: {implied}"
    );
}

/// C-100 (reflection walk): every refusal class the reflection family answers
/// carries its published envelope — the terminal classes
/// (`NOT_AUTHORIZED`, `INVALID_INPUT`, `PROFILE_INCOMPLETE`,
/// `REVISION_CONFLICT`, `IDEMPOTENCY_CONFLICT`, `SUBJECT_FENCED`,
/// `INSUFFICIENT_DATA`, `SPACE_FROZEN`), the pre-effect retryable
/// `INTERNAL_ERROR` of an interrupted generation, and the `uncertain` answer a
/// pending generation's same-key replay resolves — with the §8.2 reads staying
/// served through the freeze.
#[tokio::test]
async fn c_100_reflection_error_classes_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    let space = provider.space_id().to_owned();
    let home_path = home.path().to_path_buf();

    // 1. `NOT_AUTHORIZED` (terminal): no covering grant.
    expect_refusal(
        provider
            .get_reflection_state(
                &grantee,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "subject_all" },
                })),
            )
            .await,
        SpokeRejectCode::NotAuthorized,
        "terminal",
        "the ungranted state read",
    );

    // 2. `INVALID_INPUT` (terminal): a present cognition filter is refused
    //    outright — the honest D9 refusal, never a widened selection.
    let cognition = expect_refusal(
        provider
            .get_reflection_state(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "cognition_filter": {},
                })),
            )
            .await,
        SpokeRejectCode::InvalidInput,
        "terminal",
        "the cognition-filtered read",
    );
    assert_eq!(
        cognition
            .details
            .as_ref()
            .and_then(|details| details["field"].as_str()),
        Some("cognition_filter"),
        "the refusal names the filter: {cognition:?}"
    );

    // 3. Coverage, then the empty-basis `PROFILE_INCOMPLETE` (terminal).
    grant_subject_all_reflect(&provider, &admin).await;
    let empty_digest = expect_state_digest(
        &t3_state(&provider, &admin, json!({ "mode": "subject_all" }), None).await,
        "the empty state",
    );
    expect_refusal(
        t3_generate(&provider, &grantee, &empty_digest, "t4c100-missing").await,
        SpokeRejectCode::ProfileIncomplete,
        "terminal",
        "the empty-basis generation",
    );

    // 4. Seeded basis: `REVISION_CONFLICT` (terminal) names the current
    //    digest; the committed generation replays; a changed payload under the
    //    same key is `IDEMPOTENCY_CONFLICT` (terminal).
    let digest = sufficient_basis(&provider, &admin).await;
    let stale = expect_refusal(
        t3_generate(
            &provider,
            &grantee,
            "0000000000000000000000000000000000000000000000000000000000000000",
            "t4c100-stale",
        )
        .await,
        SpokeRejectCode::RevisionConflict,
        "terminal",
        "the stale-CAS generation",
    );
    assert_eq!(
        stale
            .details
            .as_ref()
            .and_then(|details| details["current_state_digest"].as_str()),
        Some(digest.as_str()),
        "the refusal names the current digest: {stale:?}"
    );
    let receipt = served(
        t3_generate(&provider, &grantee, &digest, "t4c100-gen").await,
        "the generation",
    );
    assert_eq!(receipt["status"], json!("committed"), "{receipt}");
    expect_refusal(
        provider
            .generate_reflection(
                &grantee,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "selected_contexts", "context_ids": [T3_CONTEXT_A] },
                    "expected_state_digest": digest,
                    "idempotency_key": "t4c100-gen",
                })),
            )
            .await,
        SpokeRejectCode::IdempotencyConflict,
        "terminal",
        "the changed-payload replay",
    );

    // 5. `SUBJECT_FENCED` (terminal): a fresh generation on a fenced subject.
    let fenced = served(
        provider
            .fence_subject(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "target": { "type": "subject" },
                    "action": "fence",
                    "expected_revision": 1,
                    "idempotency_key": "t4c100-fence",
                })),
            )
            .await,
        "the fence",
    );
    assert_eq!(fenced["status"], json!("committed"), "{fenced}");
    let fenced_refusal = expect_refusal(
        t3_generate(&provider, &grantee, &digest, "t4c100-fenced").await,
        SpokeRejectCode::SubjectFenced,
        "terminal",
        "the fenced generation",
    );
    assert_eq!(
        fenced_refusal
            .details
            .as_ref()
            .and_then(|details| details["subject_id"].as_str()),
        Some(T3_SUBJECT),
        "the refusal names the fenced subject: {fenced_refusal:?}"
    );
    let unfenced = served(
        provider
            .fence_subject(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "target": { "type": "subject" },
                    "action": "unfence",
                    "expected_revision": 2,
                    "idempotency_key": "t4c100-unfence",
                })),
            )
            .await,
        "the unfence",
    );
    assert_eq!(unfenced["status"], json!("committed"), "{unfenced}");

    // 6. `INSUFFICIENT_DATA` (terminal) on a second subject: authored profile,
    //    zero records — the records-only verdict.
    let empty_subject = "sbj_c100_empty";
    served(
        provider
            .ensure_subject(
                &admin,
                dto(json!({ "space_id": space, "subject_id": empty_subject })),
            )
            .await,
        "the empty subject ensure",
    );
    served(
        provider
            .update_profile(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": empty_subject,
                    "profile_kind": "personality",
                    "document": { "format": "markdown", "body": "# Empty subject" },
                    "expected_revision": null,
                    "idempotency_key": "t4c100-empty-profile",
                })),
            )
            .await,
        "the empty subject profile",
    );
    let empty_basis = expect_state_digest(
        &served(
            provider
                .get_reflection_state(
                    &admin,
                    dto(json!({
                        "space_id": space,
                        "subject_id": empty_subject,
                        "selection": { "mode": "subject_all" },
                    })),
                )
                .await,
            "the empty-subject state",
        ),
        "the empty-subject state",
    );
    expect_refusal(
        provider
            .generate_reflection(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": empty_subject,
                    "selection": { "mode": "subject_all" },
                    "expected_state_digest": empty_basis,
                    "idempotency_key": "t4c100-insufficient",
                })),
            )
            .await,
        SpokeRejectCode::InsufficientData,
        "terminal",
        "the recordless generation",
    );

    // 7. `SPACE_FROZEN` (terminal): generate is outside the §8.2 allowlist,
    //    while the reads stay served through the freeze.
    served(
        provider
            .create_record(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": empty_subject,
                    "kind": "note",
                    "content": { "text": "classes row" },
                    "scope": { "visibility": "context", "context_id": T3_CONTEXT_A },
                    "source_context_ids": [T3_SOURCE_A],
                    "idempotency_key": "t4c100-classes-row",
                })),
            )
            .await,
        "the empty-subject record",
    );
    let frozen_digest = expect_state_digest(
        &served(
            provider
                .get_reflection_state(
                    &admin,
                    dto(json!({
                        "space_id": space,
                        "subject_id": empty_subject,
                        "selection": { "mode": "subject_all" },
                    })),
                )
                .await,
            "the frozen-basis state",
        ),
        "the frozen-basis state",
    );
    let spaces = served(
        provider.list_spaces(&admin, dto(json!({}))).await,
        "the space list",
    );
    let space_revision = spaces["spaces"][0]["revision"]
        .as_i64()
        .expect("space revision");
    let frozen = served(
        provider
            .freeze_space(
                &admin,
                dto(json!({
                    "space_id": space,
                    "expected_revision": space_revision,
                    "idempotency_key": "t4c100-freeze",
                })),
            )
            .await,
        "the freeze",
    );
    assert_eq!(frozen["status"], json!("committed"), "{frozen}");
    expect_refusal(
        provider
            .generate_reflection(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": empty_subject,
                    "selection": { "mode": "subject_all" },
                    "expected_state_digest": frozen_digest,
                    "idempotency_key": "t4c100-frozen",
                })),
            )
            .await,
        SpokeRejectCode::SpaceFrozen,
        "terminal",
        "the frozen generation",
    );
    let frozen_read = t3_state(&provider, &admin, json!({ "mode": "subject_all" }), None).await;
    assert!(
        frozen_read["state_digest"].is_string(),
        "the read stays served through the freeze: {frozen_read}"
    );
    let unfrozen = served(
        provider
            .unfreeze_space(
                &admin,
                dto(json!({
                    "space_id": space,
                    "expected_revision": space_revision + 1,
                    "idempotency_key": "t4c100-unfreeze-space",
                })),
            )
            .await,
        "the unfreeze",
    );
    assert_eq!(unfrozen["status"], json!("committed"), "{unfrozen}");

    // 8. Pre-effect retryable `INTERNAL_ERROR`: the interrupted generation
    //    commits an honest failed receipt with no cost.
    provider.inject_generation_fault();
    let interrupted = expect_refusal(
        provider
            .generate_reflection(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": empty_subject,
                    "selection": { "mode": "subject_all" },
                    "expected_state_digest": frozen_digest,
                    "idempotency_key": "t4c100-fault",
                })),
            )
            .await,
        SpokeRejectCode::InternalError,
        "retryable",
        "the interrupted generation",
    );
    assert!(
        interrupted
            .message
            .contains("interrupted before any effect"),
        "the refusal reports the honest interruption: {interrupted:?}"
    );
    let failed_rows = durable_generation_receipts(&home_path, &space).await;
    let fault_receipt = failed_rows
        .iter()
        .find(|(_, status)| status == "failed")
        .map(|(receipt_id, _)| receipt_id.clone());
    let Some(fault_receipt) = fault_receipt else {
        panic!("the interruption recorded its failure: {failed_rows:?}");
    };
    let failed = receipt_body_of(&provider, &admin, &fault_receipt).await;
    assert_eq!(failed["status"], json!("failed"), "{failed}");
    assert!(
        failed.get("result").is_none(),
        "no cost on failure: {failed}"
    );

    // 9. `uncertain`: a same-key replay under a pending generation answers
    //    `OUTCOME_UNCERTAIN` until the terminal outcome commits.
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Barrier::new(2));
    provider.park_next_generation(Arc::clone(&entered), Arc::clone(&release));
    let generation = provider.generate_reflection(
        &admin,
        dto(json!({
            "space_id": space,
            "subject_id": empty_subject,
            "selection": { "mode": "subject_all" },
            "expected_state_digest": frozen_digest,
            "idempotency_key": "t4c100-parked",
        })),
    );
    let window = async {
        entered.notified().await;
        expect_refusal(
            provider
                .generate_reflection(
                    &admin,
                    dto(json!({
                        "space_id": space,
                        "subject_id": empty_subject,
                        "selection": { "mode": "subject_all" },
                        "expected_state_digest": frozen_digest,
                        "idempotency_key": "t4c100-parked",
                    })),
                )
                .await,
            SpokeRejectCode::OutcomeUncertain,
            "uncertain",
            "the same-key replay under a pending generation",
        );
        release.wait().await;
    };
    let (generation, ()) = tokio::join!(generation, window);
    let settled = served(generation, "the parked generation");
    assert_eq!(settled["status"], json!("committed"), "{settled}");
}

// ── Fix round: blocking-warning regressions (qc-consolidated B1–B4) ──────

/// One `subject_all` generate payload with a provenance filter pinned.
fn generate_payload_filtered(space: &str, digest: &str, key: &str, filter: &[&str]) -> Value {
    json!({
        "space_id": space,
        "subject_id": T3_SUBJECT,
        "selection": { "mode": "subject_all" },
        "filter": { "source_context_ids": filter },
        "expected_state_digest": digest,
        "idempotency_key": key,
    })
}

/// The reflection listing body for one request (filter/cursor/limit shape).
async fn t3_list_request(
    provider: &LocalMemoryProvider,
    caller: &schemas::Principal,
    request: Value,
) -> Value {
    served(
        provider.list_reflections(caller, dto(request)).await,
        "the reflection list",
    )
}

/// The one unsettled journal row's (`receipt_id`, first file target path), read
/// read-only from the provider's own state database — the durable-row
/// observation channel, not a provider reimplementation.
async fn unsettled_journal_target(user_home: &std::path::Path, space: &str) -> (String, String) {
    let db = nexus_home_layout::workspace_state_db_path(user_home, ADMIN, "default");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db)
        .read_only(true);
    let mut conn = sqlx::sqlite::SqliteConnection::connect_with(&options)
        .await
        .expect("one read-only connection to the provider's own file");
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT receipt_id, targets_json FROM memory_journal \
         WHERE space_id = ? AND state <> 'applied' LIMIT 1",
    )
    .bind(space)
    .fetch_optional(&mut conn)
    .await
    .expect("the unsettled journal row read");
    conn.close().await.expect("the read-only connection closes");
    let (receipt_id, targets) = row.expect("one unsettled journal row exists");
    let path = serde_json::from_str::<Value>(&targets)
        .expect("the journal targets are JSON")
        .as_array()
        .expect("the journal targets are a list")[0]["path"]
        .as_str()
        .expect("a journal target names its path")
        .to_owned();
    (receipt_id, path)
}

/// The state of one journal row, read read-only from the provider's own
/// state database.
async fn journal_state(user_home: &std::path::Path, space: &str, receipt_id: &str) -> String {
    let db = nexus_home_layout::workspace_state_db_path(user_home, ADMIN, "default");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db)
        .read_only(true);
    let mut conn = sqlx::sqlite::SqliteConnection::connect_with(&options)
        .await
        .expect("one read-only connection to the provider's own file");
    let (state,): (String,) =
        sqlx::query_as("SELECT state FROM memory_journal WHERE space_id = ? AND receipt_id = ?")
            .bind(space)
            .bind(receipt_id)
            .fetch_one(&mut conn)
            .await
            .expect("the journal row read");
    conn.close().await.expect("the read-only connection closes");
    state
}

/// B1: verdicts belong to each row's own pinned basis, not to a sibling row's.
/// R1 and R2 share the selection/filter pin, but R2 pins a moved basis: R2
/// answers fresh and R1 stale under the same pin, and the C-077 exclusion
/// stays per-row — tombstoning R2's added record leaves R2 tombstone-only
/// (excluded) while R1's pin is whole again (listed, fresh).
#[tokio::test]
async fn qc_b1_verdicts_are_per_row_under_one_pin_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let selection = json!({ "mode": "subject_all" });

    // R1 pins the two-record basis.
    let digest_one = sufficient_basis(&provider, &admin).await;
    let first = served(
        t3_generate(&provider, &grantee, &digest_one, "qc-b1-r1").await,
        "the first generation",
    );
    assert_eq!(first["status"], json!("committed"), "{first}");
    let first_id = first["result"]["reflection_id"]
        .as_str()
        .expect("reflection id");

    // The basis moves; R2 pins the moved basis under the same pin.
    let added = t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_A, "qc-b1 row c").await;
    let digest_two = expect_state_digest(
        &t3_state(&provider, &admin, selection.clone(), None).await,
        "the moved basis",
    );
    assert_ne!(digest_two, digest_one, "the added row moved the digest");
    let second = served(
        t3_generate(&provider, &grantee, &digest_two, "qc-b1-r2").await,
        "the second generation",
    );
    assert_eq!(second["status"], json!("committed"), "{second}");
    let second_id = second["result"]["reflection_id"]
        .as_str()
        .expect("reflection id");

    // Newest-first listing: each envelope carries its own verdict.
    let page = t3_list(&provider, &grantee, None, None).await;
    let rows = page["reflections"].as_array().expect("envelope rows");
    assert_eq!(rows.len(), 2, "both envelopes list: {page}");
    assert_eq!(rows[0]["reflection_id"], json!(second_id), "{page}");
    assert_eq!(
        rows[0]["stale"],
        json!(false),
        "R2 pins the current basis: {page}"
    );
    assert_eq!(rows[1]["reflection_id"], json!(first_id), "{page}");
    assert_eq!(
        rows[1]["stale"],
        json!(true),
        "R1 pins the superseded basis: {page}"
    );

    // Exclusion stays per-row: tombstone the record R2 added. R2's pin
    // becomes tombstone-only (excluded); R1's pin is whole again (fresh),
    // so the listing keeps exactly R1 and the state names R1 last.
    let revision = t3_records(&provider, &admin)
        .await
        .iter()
        .find(|(id, _)| *id == added)
        .expect("the added row lists before its tombstone")
        .1;
    served(
        provider
            .delete_record(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "record_id": added,
                    "expected_revision": revision,
                    "idempotency_key": "qc-b1-delete",
                })),
            )
            .await,
        "the added-row delete",
    );
    let after = t3_list(&provider, &grantee, None, None).await;
    let rows = after["reflections"].as_array().expect("envelope rows");
    assert_eq!(rows.len(), 1, "the tombstone-only pin is excluded: {after}");
    assert_eq!(rows[0]["reflection_id"], json!(first_id), "{after}");
    assert_eq!(
        rows[0]["stale"],
        json!(false),
        "R1's pin is whole again: {after}"
    );
    let state = t3_state(&provider, &admin, selection, None).await;
    assert_eq!(
        state.get("last_reflection_id"),
        Some(&json!(first_id)),
        "the excluded envelope stops surfacing last: {state}"
    );
    assert_eq!(state["stale"], json!(false), "{state}");
}

/// B2: the unsettled journal tuple reaches every consumer in one order. A
/// pending journal intent temporarily withholds its subject — the reads stay
/// admitted and serve empty honestly; once the same intent is quarantined
/// through the provider's own recovery (foreign bytes at the owed file
/// target), the same reads answer the terminal refusal. The reversed tuple
/// handed the receipt id to the state check, so a plain pending intent
/// already answered the terminal refusal.
#[tokio::test]
async fn qc_b2_pending_journal_withholds_then_answers_terminal_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let selection = json!({ "mode": "subject_all" });
    let space = provider.space_id().to_owned();

    // One committed envelope exists before the recovery window opens.
    let digest = sufficient_basis(&provider, &admin).await;
    let pin = served(
        t3_generate(&provider, &grantee, &digest, "qc-b2-pin").await,
        "the pinned generation",
    );
    assert_eq!(pin["status"], json!("committed"), "{pin}");

    // A crash after the journal COMMIT: the record's SQL effect is durable,
    // its file effect is owed, and the caller learns OUTCOME_UNCERTAIN.
    provider.inject_commit_fault(true);
    expect_refusal(
        provider
            .create_record(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "kind": "note",
                    "content": { "text": "qc-b2 recovering row" },
                    "scope": { "visibility": "context", "context_id": T3_CONTEXT_A },
                    "source_context_ids": [T3_SOURCE_A],
                    "idempotency_key": "qc-b2-recovering",
                })),
            )
            .await,
        SpokeRejectCode::OutcomeUncertain,
        "uncertain",
        "the lost record commit",
    );

    // Pending intent: the subject is withheld silently — every ordinary read
    // serves empty and stays admitted; the committed envelope does not leak.
    let listed = served(
        provider
            .list_records(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "subject_all" },
                })),
            )
            .await,
        "the withheld record list",
    );
    assert_eq!(listed["total_authorized"], json!(0), "{listed}");
    let state = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(state["basis"]["record_count"], json!(0), "{state}");
    assert!(
        state.get("last_reflection_id").is_none(),
        "a withheld subject surfaces no envelope: {state}"
    );
    let page = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(page["reflections"], json!([]), "{page}");

    // Durable window: exactly one unsettled journal row, still pending.
    let (receipt_id, path) = unsettled_journal_target(home.path(), &space).await;
    assert_eq!(
        journal_state(home.path(), &space, &receipt_id).await,
        "pending",
        "the recovering intent is pending"
    );

    // The provider's own recovery quarantines the intent once the owed file
    // target holds bytes this intent did not produce: reopen with foreign
    // bytes at the target path (relative to the provider's `memory-store`
    // file root).
    let target = nexus_home_layout::operational_workspace_dir(home.path(), ADMIN, "default")
        .join("memory-store")
        .join(&path);
    std::fs::create_dir_all(target.parent().expect("the target has a parent"))
        .expect("the target directory");
    std::fs::write(&target, "bytes no journal intent produced").expect("the foreign bytes");
    drop(provider);
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("reopen the local provider");
    assert_eq!(
        journal_state(home.path(), &space, &receipt_id).await,
        "quarantined",
        "the bounded pass quarantined the foreign target"
    );

    // Terminal: the same reads now answer the refusal and name the subject.
    let records = expect_refusal(
        provider
            .list_records(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "selection": { "mode": "subject_all" },
                })),
            )
            .await,
        SpokeRejectCode::InternalError,
        "terminal",
        "the quarantined record list",
    );
    assert_eq!(
        records
            .details
            .as_ref()
            .and_then(|d| d["subject_id"].as_str()),
        Some(T3_SUBJECT),
        "the refusal names the quarantined subject: {records:?}"
    );
    expect_refusal(
        provider
            .get_reflection_state(
                &admin,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                    "selection": selection,
                })),
            )
            .await,
        SpokeRejectCode::InternalError,
        "terminal",
        "the quarantined state read",
    );
    expect_refusal(
        provider
            .list_reflections(
                &grantee,
                dto(json!({
                    "space_id": space,
                    "subject_id": T3_SUBJECT,
                })),
            )
            .await,
        SpokeRejectCode::InternalError,
        "terminal",
        "the quarantined listing",
    );
}

/// B3: an authorized same-key replay outranks the §8.2 freeze gate. A
/// committed generation whose response was lost is recoverable through the
/// identical retry while the space is frozen; a fresh key still answers
/// `SPACE_FROZEN`. The facade checked freeze before the store's replay probe,
/// which made already-committed work unrecoverable during a freeze.
#[tokio::test]
async fn qc_b3_committed_generation_replays_while_frozen_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let digest = sufficient_basis(&provider, &admin).await;
    let spaces = served(
        provider.list_spaces(&admin, dto(json!({}))).await,
        "the space list",
    );
    let revision = spaces["spaces"][0]["revision"]
        .as_i64()
        .expect("space revision");

    // The generation commits; its response is the caller's only copy.
    let first = served(
        t3_generate(&provider, &grantee, &digest, "qc-b3-commit").await,
        "the committed generation",
    );
    assert_eq!(first["status"], json!("committed"), "{first}");

    // Freeze, then recover the committed outcome through the same key.
    served(
        provider
            .freeze_space(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "expected_revision": revision,
                    "idempotency_key": "qc-b3-freeze",
                })),
            )
            .await,
        "the freeze",
    );
    let replay = served(
        t3_generate(&provider, &grantee, &digest, "qc-b3-commit").await,
        "the same-key replay while frozen",
    );
    assert_eq!(
        replay["receipt_id"], first["receipt_id"],
        "the replay discloses the stored receipt: {replay}"
    );
    let replay_page = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        replay_page["reflections"].as_array().expect("rows").len(),
        1,
        "a replay never generates again: {replay_page}"
    );

    // Fresh work during the freeze is still refused.
    expect_refusal(
        t3_generate(&provider, &grantee, &digest, "qc-b3-fresh").await,
        SpokeRejectCode::SpaceFrozen,
        "terminal",
        "the fresh generation while frozen",
    );

    // Unfrozen, the same fresh key runs exactly one new generation.
    served(
        provider
            .unfreeze_space(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "expected_revision": revision + 1,
                    "idempotency_key": "qc-b3-unfreeze",
                })),
            )
            .await,
        "the unfreeze",
    );
    served(
        t3_generate(&provider, &grantee, &digest, "qc-b3-fresh").await,
        "the fresh generation after unfreeze",
    );
    let page = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        page["reflections"].as_array().expect("rows").len(),
        2,
        "the replay added no envelope; the fresh key added one: {page}"
    );
}

/// B4: a filtered list/state returns only the envelopes pinned to the
/// requested provenance, and the state's last-reflection pick honors the same
/// filter. Two distinct pins under one selection are listed and read through
/// each pin's filter and without one.
#[tokio::test]
async fn qc_b4_filtered_list_and_state_honor_provenance_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;

    // One record per provenance source; each generation pins its own filter.
    author_required_profile(&provider, &admin).await;
    t3_record(
        &provider,
        &admin,
        T3_CONTEXT_A,
        T3_SOURCE_A,
        "qc-b4 source a",
    )
    .await;
    t3_record(
        &provider,
        &admin,
        T3_CONTEXT_B,
        T3_SOURCE_B,
        "qc-b4 source b",
    )
    .await;
    let digest_a = expect_state_digest(
        &t3_state(
            &provider,
            &admin,
            json!({ "mode": "subject_all" }),
            Some(&[T3_SOURCE_A]),
        )
        .await,
        "the filtered basis a",
    );
    let pin_a = served(
        provider
            .generate_reflection(
                &grantee,
                dto(generate_payload_filtered(
                    provider.space_id(),
                    &digest_a,
                    "qc-b4-pin-a",
                    &[T3_SOURCE_A],
                )),
            )
            .await,
        "the filtered generation a",
    );
    let pin_first_id = pin_a["result"]["reflection_id"]
        .as_str()
        .expect("reflection id");
    let digest_b = expect_state_digest(
        &t3_state(
            &provider,
            &admin,
            json!({ "mode": "subject_all" }),
            Some(&[T3_SOURCE_B]),
        )
        .await,
        "the filtered basis b",
    );
    let pin_b = served(
        provider
            .generate_reflection(
                &grantee,
                dto(generate_payload_filtered(
                    provider.space_id(),
                    &digest_b,
                    "qc-b4-pin-b",
                    &[T3_SOURCE_B],
                )),
            )
            .await,
        "the filtered generation b",
    );
    let pin_second_id = pin_b["result"]["reflection_id"]
        .as_str()
        .expect("reflection id");

    // Unfiltered: both envelopes list.
    let unfiltered = t3_list(&provider, &grantee, None, None).await;
    let rows = unfiltered["reflections"].as_array().expect("rows");
    assert_eq!(
        rows.len(),
        2,
        "both pins list without a filter: {unfiltered}"
    );

    // Filtered list: only the envelope pinned to the requested provenance.
    let listed_a = t3_list_request(
        &provider,
        &grantee,
        json!({
            "space_id": provider.space_id(),
            "subject_id": T3_SUBJECT,
            "filter": { "source_context_ids": [T3_SOURCE_A] },
        }),
    )
    .await;
    let rows_a = listed_a["reflections"].as_array().expect("rows");
    assert_eq!(
        rows_a.len(),
        1,
        "the filter narrows the projection: {listed_a}"
    );
    assert_eq!(
        rows_a[0]["reflection_id"],
        json!(pin_first_id),
        "{listed_a}"
    );
    let listed_b = t3_list_request(
        &provider,
        &grantee,
        json!({
            "space_id": provider.space_id(),
            "subject_id": T3_SUBJECT,
            "filter": { "source_context_ids": [T3_SOURCE_B] },
        }),
    )
    .await;
    let rows_b = listed_b["reflections"].as_array().expect("rows");
    assert_eq!(rows_b.len(), 1, "{listed_b}");
    assert_eq!(
        rows_b[0]["reflection_id"],
        json!(pin_second_id),
        "{listed_b}"
    );

    // Filtered state: the last-reflection pick honors the same filter, and
    // the reported staleness is that envelope's own verdict.
    let state_a = t3_state(
        &provider,
        &admin,
        json!({ "mode": "subject_all" }),
        Some(&[T3_SOURCE_A]),
    )
    .await;
    assert_eq!(
        state_a.get("last_reflection_id"),
        Some(&json!(pin_first_id)),
        "the state's last pick honors the filter: {state_a}"
    );
    assert_eq!(state_a["stale"], json!(false), "{state_a}");
    let state_b = t3_state(
        &provider,
        &admin,
        json!({ "mode": "subject_all" }),
        Some(&[T3_SOURCE_B]),
    )
    .await;
    assert_eq!(
        state_b.get("last_reflection_id"),
        Some(&json!(pin_second_id)),
        "{state_b}"
    );
}

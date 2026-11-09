//! N4-R `_local` reflection selectors (`v230-n4r-local-reflection`).
//!
//! Rows land one per owning task; Task 6 completes the file with the full
//! `## Verification` selector set and the matrix plumbing. Until that round,
//! the rows drive the local provider's own [`MemoryPort`] admission directly:
//! the Connect dispatch gate still refuses the unnegotiated
//! `spoke-memory-reflection` family on every matrix session this round's
//! harness can open, so a signed-wire invoke would exercise the dispatch gate
//! (C-003/C-073's rows), never the admission rule a Task 2–5 row owns.
//!
//! C-075 row (this task): a context-restricted `reflect` grant cannot call the
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
//! C-077 — a tombstoned row leaves the authorized basis at read time. The
//! pinned-envelope halves of C-042/C-071/C-076/C-077 (staleness against a
//! pinned basis, envelope paging, tombstoned-basis envelope exclusion) need
//! reflection rows, which only `generate` writes (Task 4); Task 6 completes
//! those halves with the full selector set.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

use serde_json::{json, Value};
use spoke_operations::{spoke_schemas as schemas, MemoryPort, SpokeReject, SpokeRejectCode, SpokeResult};

use nexus_core::memory_provider::LocalMemoryProvider;

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
    // the grant's reach, so it must not answer the C-075 refusal. (In this
    // round the admitted request still falls through to the not-served
    // refusal; Task 3's read flow replaces that with the success shape.)
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
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
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
        receipt["status"], json!("committed"),
        "the reflect grant commits: {receipt}"
    );
}

/// Ensure the row's subject exists, then return the principal pair.
async fn t3_setup(
    provider: &LocalMemoryProvider,
) -> (schemas::Principal, schemas::Principal) {
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
            ).await,
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
            ).await,
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
            ).await,
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
    assert_eq!(state["basis"], json!({ "profile_kinds": [], "record_count": 0 }), "{state}");
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
        admin_state["basis"]["record_count"], json!(0),
        "the admin reads the same empty basis: {admin_state}"
    );

    // Both reads stay served through a freeze (§8.2 read-only allowlist).
    let spaces = served(
        provider.list_spaces(&admin, dto(json!({}))).await,
        "the space list",
    );
    let revision = spaces["spaces"][0]["revision"].as_i64().expect("space revision");
    let frozen = served(
        provider
            .freeze_space(
                &admin,
                dto(json!({
                    "space_id": provider.space_id(),
                    "expected_revision": revision,
                    "idempotency_key": "t3-freeze",
                })),
            ).await,
            "the freeze",
    );
    assert_eq!(frozen["status"], json!("committed"), "{frozen}");
    let frozen_state =
        t3_state(&provider, &grantee, json!({ "mode": "subject_all" }), None).await;
    assert_eq!(
        frozen_state["state_digest"], state["state_digest"],
        "a frozen space answers the same basis: {frozen_state}"
    );
    let frozen_list = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(
        frozen_list["reflections"], json!([]),
        "the frozen listing is served empty: {frozen_list}"
    );
}

/// C-042: every basis-affecting fact class — record create, edit, delete,
/// scope-promote, profile update, and an authorization change that alters the
/// authorized set — moves the published `state_digest` at the next read, with
/// no eager invalidation writes and `stale` honestly false while nothing is
/// pinned.
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
            ).await,
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
            ).await,
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
            ).await,
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
            ).await,
            "the profile update",
    );
    assert_eq!(profiled["status"], json!("committed"), "{profiled}");
    let profile_state = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        profile_state["basis"]["profile_kinds"], json!(["personality"]),
        "the authored kind publishes: {profile_state}"
    );
    assert_eq!(
        profile_state["missing_required_profile_kinds"], json!([]),
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
            ).await,
            "the context-scoped reflect grant",
    );
    let peer_state = t3_state(&provider, &grantee, scoped_selection, None).await;
    digests.push(expect_state_digest(&peer_state, "the peer's scoped basis"));
    assert_eq!(
        peer_state["basis"]["record_count"], json!(0),
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
            ).await,
            "the profile update",
    );
    assert_eq!(profiled["status"], json!("committed"), "{profiled}");
    let profile_only = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        profile_only["basis"],
        json!({ "profile_kinds": ["personality"], "record_count": 0 }),
        "{profile_only}"
    );
    assert_eq!(profile_only["data_sufficiency"], json!("insufficient"), "{profile_only}");
    assert_eq!(
        profile_only["missing_required_profile_kinds"], json!([]),
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
    let filtered =
        t3_state(&provider, &admin, selection.clone(), Some(&[T3_SOURCE_A])).await;
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
            ).await,
            "the record edit",
    );
    assert_eq!(edited["status"], json!("committed"), "{edited}");
    let after_edit = t3_state(&provider, &admin, selection, None).await;
    assert_ne!(
        after_edit["state_digest"], digest,
        "the edit moved the digest: {after_edit}"
    );
    assert_eq!(after_edit["stale"], json!(false), "{after_edit}");
}

/// C-071 (read half): the listing contract — an empty page names no
/// end-of-results cursor, the limit bound is honored, the cursor is opaque and
/// bound to the op that minted it, and a foreign or malformed cursor answers
/// `CURSOR_INVALID` (terminal). The terminal-receipt half needs `generate`
/// (Task 4); Task 6 completes it.
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
            ).await,
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
}

/// C-077 (read half): a tombstoned row leaves the authorized basis at read
/// time — the published count drops, the digest moves, and the same exclusion
/// governs the record listing. The envelope-exclusion half (reflections whose
/// pinned source set went tombstone-only are excluded from list/get) needs
/// pinned rows from `generate` (Task 4); Task 6 completes it.
#[tokio::test]
async fn c_077_tombstoned_bases_excluded_from_list_get_local() {
    let home = tempfile::TempDir::new().expect("temporary provider home");
    let provider = LocalMemoryProvider::open(home.path().to_path_buf(), ADMIN, "default")
        .await
        .expect("open the local provider");
    let (admin, grantee) = t3_setup(&provider).await;
    grant_subject_all_reflect(&provider, &admin).await;
    let selection = json!({ "mode": "subject_all" });

    let record_a = t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_A, "tombstone row a").await;
    t3_record(&provider, &admin, T3_CONTEXT_A, T3_SOURCE_A, "tombstone row b").await;
    let before = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(before["basis"]["record_count"], json!(2), "{before}");
    let digest = expect_state_digest(&before, "the two-row state");

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
            ).await,
            "the record delete",
    );

    let after = t3_state(&provider, &admin, selection.clone(), None).await;
    assert_eq!(
        after["basis"]["record_count"], json!(1),
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
    // The listing still serves; with no pinned envelope there is nothing to
    // exclude from it yet.
    let page = t3_list(&provider, &grantee, None, None).await;
    assert_eq!(page["reflections"], json!([]), "{page}");
}

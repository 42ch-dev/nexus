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

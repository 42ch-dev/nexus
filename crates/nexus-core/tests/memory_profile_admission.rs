//! MEM-R005 / C-062 profile admission: the three outcomes stay distinct.
//!
//! A replacement provider fails exactly one of three ways, and they are never
//! collapsed (spoke-memory §4.2, adoption plan D-12):
//!
//! * a **required** authored content kind is absent ⇒ `PROFILE_INCOMPLETE`
//!   (fail-closed, `details.missing`), never an empty success;
//! * an **optional** authored kind is absent ⇒ honest-empty success (§7.23) —
//!   the kind is simply not returned and nothing is invented;
//! * the **capability** is absent ⇒ `CAPABILITY_PORT_MISSING` at provider
//!   selection, never a profile-admission verdict.
//!
//! Profiles carry content kinds only: SOUL.md is the authored `personality`
//! profile, and no governance/transfer/reflection document is ever fabricated.

use std::path::PathBuf;

use nexus_core::memory_provider::profile::SOUL_PROFILE_KIND;
use nexus_core::memory_provider::LocalMemoryProvider;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{MemoryPort, SpokeRejectCode, SpokeResult};
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SPACE: &str = "spc_local_default";
const SUBJECT: &str = "sbj_mira";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
}

fn seed_env() -> Env {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    std::fs::create_dir_all(user_home.join(".nexus42")).unwrap();
    Env {
        _tmp: tmp,
        user_home,
    }
}

async fn open_provider(env: &Env) -> LocalMemoryProvider {
    LocalMemoryProvider::open(env.user_home.clone(), CREATOR, "default")
        .await
        .expect("provider open")
}

fn admin() -> schemas::Principal {
    dto(json!({ "principal_id": CREATOR, "extensions": {} }))
}

fn dto<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid payload")
}

/// Project a port result onto one assertion shape: the payload, or the refusal
/// as the wire carries it (§14.1 — a refusal is never a success value).
fn wire<T: Serialize>(result: &SpokeResult<T>) -> Value {
    match result {
        SpokeResult::Ok(value) => serde_json::to_value(value).expect("serializable response"),
        SpokeResult::Reject(reject) => json!({
            "error": {
                "code": reject.code.as_str(),
                "details": reject.details.clone().map_or(Value::Null, Value::Object),
                "message": reject.message,
            }
        }),
    }
}

fn ok<T>(result: SpokeResult<T>) -> T {
    match result {
        SpokeResult::Ok(value) => value,
        SpokeResult::Reject(reject) => {
            panic!(
                "expected success, got {}: {}",
                reject.code.as_str(),
                reject.message
            )
        }
    }
}

fn error_code(body: &Value) -> &str {
    body["error"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("expected a memory error, got {body}"))
}

fn error_details(body: &Value) -> &Value {
    &body["error"]["details"]
}

async fn ensure(provider: &LocalMemoryProvider) {
    wire(
        &provider
            .ensure_subject(
                &admin(),
                dto(json!({ "space_id": SPACE, "subject_id": SUBJECT })),
            )
            .await,
    );
}

/// Author one profile kind; the document is the authored content, never policy.
async fn author(provider: &LocalMemoryProvider, kind: &str, body: &str, key: &str) -> Value {
    wire(
        &provider
            .update_profile(
                &admin(),
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "profile_kind": kind,
                    "expected_revision": null,
                    "document": { "format": "markdown", "body": body },
                    "idempotency_key": key,
                })),
            )
            .await,
    )
}

async fn read_profiles(provider: &LocalMemoryProvider, kinds: Option<&[&str]>) -> Value {
    let mut request = json!({ "space_id": SPACE, "subject_id": SUBJECT });
    if let Some(kinds) = kinds {
        request["profile_kinds"] = json!(kinds);
    }
    wire(&provider.get_profile(&admin(), dto(request)).await)
}

/// The reject the admission gate raises, as a typed port value.
async fn admission_rejection(provider: &LocalMemoryProvider) -> spoke_operations::SpokeReject {
    match provider.profile_admission(&admin(), SPACE, SUBJECT).await {
        SpokeResult::Reject(reject) => reject,
        SpokeResult::Ok(admission) => {
            panic!("a missing required profile must never be a success value: {admission:?}")
        }
    }
}

/// Outcome 1: a required authored kind is absent ⇒ `PROFILE_INCOMPLETE` with
/// `details.missing`, terminal, and a named recovery. A present *optional* kind
/// never substitutes for the required one.
#[tokio::test]
async fn required_profile_missing_fails_closed_with_profile_incomplete() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;

    let reject = admission_rejection(&provider).await;
    assert_eq!(reject.code, SpokeRejectCode::ProfileIncomplete);
    assert_eq!(reject.code.as_str(), "PROFILE_INCOMPLETE");
    let details = reject.details.as_ref().expect("details are carried");
    assert_eq!(details["missing"], json!([SOUL_PROFILE_KIND]));
    assert_eq!(details["retry_class"], "terminal");
    assert!(
        reject.message.contains("memory.profile.update"),
        "the refusal names its recovery: {}",
        reject.message
    );
    assert_ne!(
        reject.code,
        SpokeRejectCode::CapabilityPortMissing,
        "a missing authored profile is not a missing capability (MEM-R005)"
    );

    // Only the optional `preferences` kind is authored: the required one is
    // still missing, so the gate still fails closed.
    let authored = author(
        &provider,
        "preferences",
        "- concise replies",
        "k-preferences",
    )
    .await;
    assert_eq!(authored["status"], "committed");

    let reject = admission_rejection(&provider).await;
    assert_eq!(reject.code, SpokeRejectCode::ProfileIncomplete);
    assert_eq!(
        reject.details.as_ref().unwrap()["missing"],
        json!([SOUL_PROFILE_KIND]),
        "an optional kind never substitutes for the required one"
    );
}

/// Outcome 2: the required kind is authored and the optional kind is absent ⇒
/// honest-empty success. Nothing is invented for the absent kind.
#[tokio::test]
async fn optional_kind_absent_is_honest_empty_success() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;

    // D-12: the Creator SOUL document is the authored `personality` profile.
    let authored = author(
        &provider,
        SOUL_PROFILE_KIND,
        "# Mira\n\nPatient, watchful.",
        "k-personality",
    )
    .await;
    assert_eq!(authored["status"], "committed");

    let admitted = ok(provider.profile_admission(&admin(), SPACE, SUBJECT).await);
    assert_eq!(admitted.present, vec![SOUL_PROFILE_KIND.to_string()]);

    let read = read_profiles(&provider, Some(&[SOUL_PROFILE_KIND, "preferences"])).await;
    let profiles = read["profiles"].as_array().unwrap();
    assert_eq!(
        profiles.len(),
        1,
        "the absent optional kind is honest-empty, not fabricated: {read}"
    );
    assert_eq!(profiles[0]["profile_kind"], SOUL_PROFILE_KIND);
    assert_eq!(
        profiles[0]["document"]["body"],
        "# Mira\n\nPatient, watchful."
    );

    let optional_only = read_profiles(&provider, Some(&["preferences"])).await;
    assert_eq!(
        optional_only["profiles"],
        json!([]),
        "an optional absence is an empty result, not an error: {optional_only}"
    );
    assert!(
        optional_only.get("error").is_none(),
        "an optional absence is never a refusal: {optional_only}"
    );
}

/// Outcome 3: the capability is absent ⇒ `CAPABILITY_PORT_MISSING` at
/// selection, and neither code ever stands in for the other.
#[tokio::test]
async fn capability_mismatch_is_distinct_from_profile_admission() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;

    let describe = wire(&provider.describe(&admin(), dto(json!({}))).await);
    assert_eq!(describe["capabilities"], json!(["spoke-memory"]));
    assert_eq!(
        describe["required_profile_kinds"],
        json!([SOUL_PROFILE_KIND]),
        "the declared required kind is the SOUL.md content kind (D-12): {describe}"
    );

    let reflection = wire(
        &provider
            .generate_reflection(
                &admin(),
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "expected_state_digest": "0".repeat(64),
                    "idempotency_key": "k-reflection-gated",
                })),
            )
            .await,
    );
    assert_eq!(
        error_code(&reflection),
        "CAPABILITY_PORT_MISSING",
        "an unserved family is a selection failure, not an admission verdict: {reflection}"
    );
    assert_eq!(
        error_details(&reflection)["op"],
        "memory.reflections.generate"
    );
    assert_eq!(error_details(&reflection)["retry_class"], "terminal");

    let transfer = wire(
        &provider
            .get_export(
                &admin(),
                dto(json!({
                    "space_id": SPACE,
                    "export_id": "exp_1",
                    "chunk_ordinal": 0,
                })),
            )
            .await,
    );
    assert_eq!(error_code(&transfer), "CAPABILITY_PORT_MISSING");

    // Non-collapse, both directions.
    assert_ne!(
        error_code(&reflection),
        "PROFILE_INCOMPLETE",
        "capability absence must never be reported as missing authored content"
    );
    let reject = admission_rejection(&provider).await;
    assert_ne!(
        reject.code.as_str(),
        "CAPABILITY_PORT_MISSING",
        "missing authored content must never be reported as a capability failure"
    );
}

/// Content-kind discipline: the provider fabricates no governance/transfer/
/// reflection profile documents, before or after admission is refused.
#[tokio::test]
async fn no_fake_profile_documents_are_fabricated() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;

    let empty = read_profiles(&provider, None).await;
    assert_eq!(
        empty["profiles"],
        json!([]),
        "an unpublished subject has no profiles, invented or otherwise: {empty}"
    );
    let _ = admission_rejection(&provider).await;
    let _ = wire(
        &provider
            .generate_reflection(
                &admin(),
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "expected_state_digest": "0".repeat(64),
                    "idempotency_key": "k-reflection-no-fake",
                })),
            )
            .await,
    );

    let authored = author(
        &provider,
        SOUL_PROFILE_KIND,
        "# Mira\n\nPatient, watchful.",
        "k-personality-no-fake",
    )
    .await;
    assert_eq!(authored["status"], "committed");

    let read = read_profiles(&provider, None).await;
    let kinds: Vec<&str> = read["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|profile| profile["profile_kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec![SOUL_PROFILE_KIND], "{read}");
    for fabricated in ["governance", "transfer", "reflection", "soul"] {
        assert!(
            !kinds.contains(&fabricated),
            "no fake `{fabricated}` profile document may exist: {read}"
        );
    }
}

//! `LocalMemoryProvider` (NXM-1) protocol contract tests.
//!
//! Covers the whole served surface: `memory.describe`, `memory.spaces.*`,
//! `memory.subjects.*`, `memory.grants.*`, and the round-B content families
//! (`memory.capture`, `memory.candidates.*`, `memory.records.*`,
//! `memory.recall`, `memory.profile.*`) end to end on a temp nexus home. The
//! sub-capability gated families (reflection, transfer) and the NXM-2 receipt
//! read must answer the dispatch-level denial — `CAPABILITY_PORT_MISSING`, the
//! code upstream's responder serializes for an unimplemented op — never a
//! success value and never an empty success.
//!
//! Every scenario runs on a temp nexus home. Import-boundary note: the shared
//! `MemoryPort` vocabulary comes from `spoke_operations`, whose generated wire
//! types are reachable through its `spoke_schemas` re-export, so this crate
//! keeps exactly one spoke dependency edge.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::SubsecRound;
use nexus_core::memory_provider::LocalMemoryProvider;
use nexus_creator_memory::bearer::MemoryBearerRef;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{MemoryPort, SpokeRejectCode, SpokeResult};
use sqlx::Connection as _;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const THIRD: &str = "ctr_cccccccccccccccccccccccccccccccc";
const SPACE: &str = "spc_local_default";
const SUBJECT: &str = "sbj_mira";
const OTHER_SUBJECT: &str = "sbj_other";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
    db_path: PathBuf,
}

fn seed_env() -> Env {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, "default");
    Env {
        _tmp: tmp,
        user_home,
        db_path,
    }
}

async fn open_provider(env: &Env) -> LocalMemoryProvider {
    LocalMemoryProvider::open(env.user_home.clone(), CREATOR, "default")
        .await
        .expect("provider open")
}

fn principal(id: &str) -> schemas::Principal {
    dto(json!({ "principal_id": id, "extensions": {} }))
}

fn dto<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid payload")
}

/// Project a port result onto ONE assertion shape: the success payload, or the
/// refusal as the wire carries it. Refusals must arrive as `SpokeResult::Reject`
/// (§14.1), so both channels are readable here without encoding either as a
/// success.
fn wire<T: serde::Serialize>(result: &SpokeResult<T>) -> Value {
    match result {
        SpokeResult::Ok(value) => serde_json::to_value(value).expect("serializable response"),
        SpokeResult::Reject(reject) => json!({
            "error": {
                "code": reject.code.as_str(),
                "details": reject
                    .details
                    .clone()
                    .map_or(Value::Null, Value::Object),
                "message": reject.message,
            }
        }),
    }
}

/// The success payload of a typed port result; a refusal panics with its code.
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

fn retry_class(body: &Value) -> &str {
    error_details(body)["retry_class"]
        .as_str()
        .unwrap_or_else(|| panic!("expected details.retry_class, got {body}"))
}

/// A committed receipt, as the wire sees it (§6.10).
fn receipt(body: &Value) -> &Value {
    assert_eq!(
        body["status"], "committed",
        "expected a committed receipt, got {body}"
    );
    body
}

fn grant_request(key: &str, capability: &str, grantee: &str) -> schemas::MemoryGrantsGrantRequest {
    dto(json!({
        "space_id": SPACE,
        "principal_id": grantee,
        "capabilities": [capability],
        "idempotency_key": key,
    }))
}

/// §6.5 unrestricted grant scoped to one subject.
fn subject_grant(
    key: &str,
    capability: &str,
    grantee: &str,
    subject: &str,
) -> schemas::MemoryGrantsGrantRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": subject,
        "principal_id": grantee,
        "capabilities": [capability],
        "idempotency_key": key,
    }))
}

/// §6.5 context-restricted grant (the shape `context_ids` selects).
fn restricted_grant(
    key: &str,
    capability: &str,
    grantee: &str,
    subject: &str,
    contexts: &[&str],
) -> schemas::MemoryGrantsGrantRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": subject,
        "principal_id": grantee,
        "capabilities": [capability],
        "context_ids": contexts,
        "shared_access": false,
        "idempotency_key": key,
    }))
}

fn fence_at_subject(
    subject: &str,
    action: &str,
    revision: u64,
    key: &str,
) -> schemas::MemorySubjectsFenceRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": subject,
        "target": { "type": "subject" },
        "action": action,
        "expected_revision": revision,
        "idempotency_key": key,
    }))
}

fn fence_at_context(
    subject: &str,
    context_id: &str,
    action: &str,
    revision: u64,
    key: &str,
) -> schemas::MemorySubjectsFenceRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": subject,
        "target": { "type": "context", "context_id": context_id },
        "action": action,
        "expected_revision": revision,
        "idempotency_key": key,
    }))
}

fn ensure_request(subject: &str) -> schemas::MemorySubjectsEnsureRequest {
    dto(json!({ "space_id": SPACE, "subject_id": subject }))
}

fn freeze_request(key: &str, revision: u64) -> schemas::MemorySpacesFreezeRequest {
    dto(json!({ "space_id": SPACE, "expected_revision": revision, "idempotency_key": key }))
}

fn list_grants(
    subject: Option<&str>,
    principal_filter: Option<&str>,
    limit: Option<u64>,
    cursor: Option<&str>,
) -> schemas::MemoryGrantsListRequest {
    let mut request = json!({ "space_id": SPACE });
    if let Some(subject) = subject {
        request["subject_id"] = json!(subject);
    }
    if let Some(principal_id) = principal_filter {
        request["principal_id"] = json!(principal_id);
    }
    if let Some(limit) = limit {
        request["limit"] = json!(limit);
    }
    if let Some(cursor) = cursor {
        request["cursor"] = json!(cursor);
    }
    dto(request)
}

/// §7.1: the provider describes exactly what it serves, carries no space
/// catalogue, and every advertised limit is a positive bound.
#[tokio::test]
async fn describe_reports_the_served_surface() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let body = wire(&provider.describe(&principal(CREATOR), dto(json!({}))).await);

    assert_eq!(body["memory_protocol_version"], 1);
    assert!(
        body["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|flag| flag == "spoke-memory"),
        "the served family is advertised: {body}"
    );
    assert!(
        !body["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|flag| flag == "spoke-memory-reflection" || flag == "spoke-memory-transfer"),
        "the gated sub-capabilities are not advertised: {body}"
    );
    assert!(
        body.get("spaces").is_none(),
        "describe carries no space catalogue (§7.1): {body}"
    );
    assert_eq!(body["required_profile_kinds"], json!(["personality"]));
    assert!(
        !body["engine_policy_version"].as_str().unwrap().is_empty(),
        "a non-empty engine policy version is stamped: {body}"
    );

    let limits = &body["limits"];
    for limit in [
        "max_content_bytes",
        "max_candidates_per_capture",
        "max_page_size",
        "export_chunk_bytes",
        "idempotency_retention_seconds",
        "receipt_retention_seconds",
        "dedup_retention_seconds",
        "staged_import_ttl_seconds",
        "export_retention_seconds",
    ] {
        assert!(
            limits[limit].as_u64().unwrap_or(0) > 0,
            "limit {limit} must be positive: {body}"
        );
    }
    assert!(
        limits["dedup_retention_seconds"].as_u64().unwrap()
            >= limits["idempotency_retention_seconds"].as_u64().unwrap(),
        "dedup retention must cover receipt retention (MEM-R021): {body}"
    );
}

/// §7.2: the operator-provisioned local space is the only catalogue entry, and
/// it reports the computed drain barrier.
#[tokio::test]
async fn spaces_list_shows_the_provisioned_space() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let body = wire(
        &provider
            .list_spaces(&principal(CREATOR), dto(json!({})))
            .await,
    );

    let spaces = body["spaces"].as_array().expect("spaces array");
    assert_eq!(spaces.len(), 1, "one provisioned space: {body}");
    assert_eq!(spaces[0]["space_id"], SPACE);
    assert_eq!(spaces[0]["status"], "active");
    assert_eq!(spaces[0]["revision"], 1);
    assert_eq!(spaces[0]["pending_receipts"], 0);
    assert!(
        !spaces[0]["authority_host_id"].as_str().unwrap().is_empty(),
        "the space names its authoritative provider: {body}"
    );
    assert_eq!(body.get("next_cursor"), None);
}

/// §7.3/§7.4/§7.0.5/§7.0.4: freeze is revision-checked, replaying the same
/// idempotency key + payload returns the ORIGINAL receipt without
/// re-executing, a different payload under the same key is
/// `IDEMPOTENCY_CONFLICT`, a stale revision is `REVISION_CONFLICT`, a frozen
/// space refuses non-allowlisted mutations, and unfreeze stays admissible.
#[tokio::test]
async fn space_freeze_is_revision_checked_and_key_idempotent() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);

    let issued = wire(
        &provider
            .grant(&admin, grant_request("k-pre-freeze", "capture", OTHER))
            .await,
    );
    let grant_id = receipt(&issued)["result"]["grant_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let first = wire(
        &provider
            .freeze_space(&admin, freeze_request("k-freeze", 1))
            .await,
    );
    let first = receipt(&first);
    assert_eq!(first["op"], "memory.spaces.freeze");
    assert_eq!(first["result"]["status"], "frozen");
    assert_eq!(first["result"]["revision"], 2);
    assert_eq!(first["result"]["pending_receipts"], 0);
    assert_eq!(first["requester_principal_id"], CREATOR);
    let receipt_id = first["receipt_id"].as_str().unwrap().to_owned();

    let replay = wire(
        &provider
            .freeze_space(&admin, freeze_request("k-freeze", 1))
            .await,
    );
    assert_eq!(
        receipt(&replay)["receipt_id"],
        receipt_id,
        "the same key + payload replays the stored receipt (§7.0.5)"
    );

    let conflicting = wire(
        &provider
            .freeze_space(&admin, freeze_request("k-freeze", 2))
            .await,
    );
    assert_eq!(error_code(&conflicting), "IDEMPOTENCY_CONFLICT");
    assert_eq!(retry_class(&conflicting), "terminal");

    let stale = wire(
        &provider
            .freeze_space(&admin, freeze_request("k-stale", 1))
            .await,
    );
    assert_eq!(error_code(&stale), "REVISION_CONFLICT");
    assert_eq!(retry_class(&stale), "terminal");

    let refused = wire(
        &provider
            .grant(&admin, grant_request("k-grant-frozen", "capture", OTHER))
            .await,
    );
    assert_eq!(
        error_code(&refused),
        "SPACE_FROZEN",
        "grants.grant is outside the §8.2 allowlist"
    );

    // §8.2: revoke stays admissible while frozen, so an incident can be contained.
    let revoked = wire(
        &provider
            .revoke_grant(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "grant_id": grant_id,
                    "expected_revision": 1,
                    "idempotency_key": "k-revoke-frozen",
                })),
            )
            .await,
    );
    assert_eq!(receipt(&revoked)["result"]["revoked"], true);

    let unfrozen = wire(
        &provider
            .unfreeze_space(
                &admin,
                dto(json!({ "space_id": SPACE, "expected_revision": 2, "idempotency_key": "k-unfreeze" })),
            )
            .await,
    );
    assert_eq!(receipt(&unfrozen)["result"]["status"], "active");
    assert_eq!(receipt(&unfrozen)["result"]["revision"], 3);
}

/// §7.5/§7.6/§7.7: get-or-create keys off the subject id (kind is creation-only
/// metadata), and the fence state machine covers subject, context and the
/// terminal archive.
#[tokio::test]
async fn subjects_ensure_get_and_fence_state_machine() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let ensure = |kind: Value| -> schemas::MemorySubjectsEnsureRequest {
        dto(json!({ "space_id": SPACE, "subject_id": SUBJECT, "kind": kind }))
    };

    let created = wire(
        &provider
            .ensure_subject(&admin, ensure(json!({ "ns": "nexus", "id": "character" })))
            .await,
    );
    let created = receipt(&created);
    assert_eq!(created["result"]["created"], true);
    assert_eq!(created["result"]["status"], "active");
    assert_eq!(created["result"]["revision"], 1);
    assert_eq!(
        created.get("idempotency_key"),
        None,
        "the sole key exemption echoes no key (§7.0.5)"
    );

    let existing = wire(
        &provider
            .ensure_subject(&admin, ensure(json!({ "ns": "other", "id": "x" })))
            .await,
    );
    let existing = receipt(&existing);
    assert_eq!(existing["result"]["created"], false);
    assert_eq!(
        existing["result"]["revision"], 1,
        "kind on an existing subject is not authority and never rewrites it"
    );

    let read = wire(
        &provider
            .get_subject(
                &admin,
                dto(json!({ "space_id": SPACE, "subject_id": SUBJECT })),
            )
            .await,
    );
    assert_eq!(read["subject"]["subject_id"], SUBJECT);
    assert_eq!(read["subject"]["status"], "active");
    assert_eq!(read["subject"]["kind"]["ns"], "nexus");
    assert_eq!(read["subject"]["kind"]["id"], "character");

    let archived_on_context = wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_context(SUBJECT, "bnd_1", "archive", 1, "k-arch-ctx"),
            )
            .await,
    );
    assert_eq!(error_code(&archived_on_context), "INVALID_INPUT");

    let fenced = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "fence", 1, "k-fence"))
            .await,
    );
    assert_eq!(receipt(&fenced)["result"]["status"], "fenced");
    assert_eq!(receipt(&fenced)["result"]["revision"], 2);

    let unfenced = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "unfence", 2, "k-unfence"))
            .await,
    );
    assert_eq!(receipt(&unfenced)["result"]["status"], "active");
    assert_eq!(receipt(&unfenced)["result"]["revision"], 3);

    let context_fenced = wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_context(SUBJECT, "bnd_1", "fence", 3, "k-fence-ctx"),
            )
            .await,
    );
    assert_eq!(receipt(&context_fenced)["result"]["status"], "active");
    assert_eq!(
        receipt(&context_fenced)["result"]["fenced_contexts"],
        json!(["bnd_1"])
    );

    let archived = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "archive", 4, "k-archive"))
            .await,
    );
    assert_eq!(receipt(&archived)["result"]["status"], "archived");

    let resurrect = wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_subject(SUBJECT, "unfence", 5, "k-unarchive"),
            )
            .await,
    );
    assert_eq!(error_code(&resurrect), "INVALID_STATUS_TRANSITION");
}

/// §7.8/§7.9/§7.10: admin grants flow through non-amplification, self-view
/// exposes exactly the caller's own grants, and revoke is revision-checked and
/// terminal.
#[tokio::test]
async fn grants_grant_list_revoke_and_non_amplification() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let grantee = principal(OTHER);

    let issued = wire(
        &provider
            .grant(&admin, grant_request("k-grant-1", "capture", OTHER))
            .await,
    );
    let issued = receipt(&issued);
    let grant_id = issued["result"]["grant_id"].as_str().unwrap().to_owned();
    assert_eq!(issued["result"]["revision"], 1);

    let self_view = wire(
        &provider
            .list_grants(&grantee, dto(json!({ "space_id": SPACE })))
            .await,
    );
    let own = self_view["grants"].as_array().unwrap();
    assert_eq!(own.len(), 1, "self-view lists exactly the caller's grants");
    assert_eq!(own[0]["grant_id"], grant_id);
    assert_eq!(own[0]["principal_id"], OTHER);
    assert_eq!(own[0]["capabilities"], json!(["capture"]));

    let amplified = wire(
        &provider
            .grant(&grantee, grant_request("k-grant-2", "capture", CREATOR))
            .await,
    );
    assert_eq!(
        error_code(&amplified),
        "NOT_AUTHORIZED",
        "a capture-only grantee cannot issue grants"
    );

    let admin_view = wire(
        &provider
            .list_grants(
                &admin,
                dto(json!({ "space_id": SPACE, "principal_id": OTHER })),
            )
            .await,
    );
    assert_eq!(admin_view["grants"].as_array().unwrap().len(), 1);

    let revoked = wire(
        &provider
            .revoke_grant(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "grant_id": grant_id,
                    "expected_revision": 1,
                    "idempotency_key": "k-revoke",
                })),
            )
            .await,
    );
    let revoked = receipt(&revoked);
    assert_eq!(revoked["result"]["grant_id"], grant_id);
    assert_eq!(revoked["result"]["revoked"], true);
    assert_eq!(revoked["result"]["revision"], 2);

    let stale = wire(
        &provider
            .revoke_grant(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "grant_id": grant_id,
                    "expected_revision": 1,
                    "idempotency_key": "k-revoke-stale",
                })),
            )
            .await,
    );
    assert_eq!(error_code(&stale), "REVISION_CONFLICT");

    let after_revoke = wire(
        &provider
            .list_grants(&grantee, dto(json!({ "space_id": SPACE })))
            .await,
    );
    let revoked_row = &after_revoke["grants"].as_array().unwrap()[0];
    assert!(
        !revoked_row["revoked_at"].is_null(),
        "the revoked grant keeps its audit record with revoked_at: {after_revoke}"
    );

    // Revocation is terminal for coverage, not for the ledger: the grantee can
    // no longer run a `capture`-gated op.
    let denied = wire(
        &provider
            .ensure_subject(&grantee, ensure_request("sbj_after_revoke"))
            .await,
    );
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED");
}

/// §14.1/§14.2: refusals travel `SpokeResult::Reject` — a domain refusal keeps
/// its memory code, and an unserved op is the dispatch-deny code upstream's
/// responder serializes (`CAPABILITY_PORT_MISSING`), never a success payload.
#[tokio::test]
async fn refusals_travel_the_reject_channel() {
    let env = seed_env();
    let provider = open_provider(&env).await;

    let denied = provider
        .grant(
            &principal(OTHER),
            grant_request("k-deny", "capture", CREATOR),
        )
        .await;
    match &denied {
        SpokeResult::Reject(reject) => {
            assert_eq!(reject.code, SpokeRejectCode::NotAuthorized);
            let details = reject.details.as_ref().expect("retry class is carried");
            assert_eq!(details["retry_class"], "terminal");
        }
        SpokeResult::Ok(_) => panic!("a refusal must never be a success value: {denied:?}"),
    }

    let unsupported = provider
        .generate_reflection(
            &principal(CREATOR),
            dto(json!({
                "space_id": SPACE,
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
                "expected_state_digest": "0".repeat(64),
                "idempotency_key": "k-reflection-reject",
            })),
        )
        .await;
    match &unsupported {
        SpokeResult::Reject(reject) => {
            assert_eq!(reject.code, SpokeRejectCode::CapabilityPortMissing);
            let details = reject.details.as_ref().expect("retry class is carried");
            assert_eq!(details["retry_class"], "terminal");
        }
        SpokeResult::Ok(_) => panic!("an unserved op must be rejected: {unsupported:?}"),
    }
}

/// The sub-capability gated families and the NXM-2 receipt read answer the
/// dispatch-level denial instead of any success.
#[tokio::test]
async fn gated_families_refuse_with_the_unsupported_code() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let p = principal(CREATOR);

    let reflections = wire(
        &provider
            .get_reflection_state(
                &p,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                })),
            )
            .await,
    );
    assert_eq!(error_code(&reflections), "CAPABILITY_PORT_MISSING");

    let export = wire(
        &provider
            .get_export(
                &p,
                dto(json!({
                    "space_id": SPACE,
                    "export_id": "exp_1",
                    "chunk_ordinal": 0,
                })),
            )
            .await,
    );
    assert_eq!(error_code(&export), "CAPABILITY_PORT_MISSING");
}

/// R1: an unserved op emits exactly the code string a responder puts on the
/// wire. `send_reverse_error_envelope(doc, reject.code.as_str(), ..)` copies the
/// reject code verbatim (`responder.rs:1506-1522`), and the port vocabulary has
/// no `op_unsupported` variant (`result.rs:8-70, 76-129`) — upstream's own
/// responder answers an unimplemented port op with `CapabilityPortMissing`
/// (`responder.rs:367-371`). `op_unsupported` is only ever an *inbound* marker
/// that `map_error_envelope` folds into `CapabilityPortMissing`
/// (`remote_adapter.rs:296-299`).
#[tokio::test]
async fn unserved_ops_serialize_the_upstream_dispatch_code() {
    let env = seed_env();
    let provider = open_provider(&env).await;

    let result = provider
        .generate_reflection(
            &principal(CREATOR),
            dto(json!({
                "space_id": SPACE,
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
                "expected_state_digest": "0".repeat(64),
                "idempotency_key": "k-wire-code",
            })),
        )
        .await;
    let SpokeResult::Reject(reject) = &result else {
        panic!("an unserved op must be rejected: {result:?}");
    };

    let serialized = reject.code.as_str();
    assert_eq!(serialized, SpokeRejectCode::CapabilityPortMissing.as_str());
    assert_eq!(
        SpokeRejectCode::try_from_str(serialized),
        Some(SpokeRejectCode::CapabilityPortMissing),
        "the emitted code must be a protocol code a peer can parse"
    );
    assert_eq!(
        SpokeRejectCode::try_from_str("op_unsupported"),
        None,
        "`op_unsupported` is a transport dispatch-deny string, never a reject code"
    );

    // The envelope a responder builds from this reject carries the code as-is.
    let envelope = wire(&result);
    assert_eq!(error_code(&envelope), serialized);
    assert!(
        error_details(&envelope).get("wire_code").is_none(),
        "details must not restate a mapping that belongs to the receiving side: {envelope}"
    );
}

/// §6.5/§7.7: fence authorization is bound to the target — a context fence needs
/// coverage of THAT context, and a whole-subject fence/archive needs unrestricted
/// coverage of the subject.
#[tokio::test]
async fn fence_authorization_is_bound_to_the_fenced_target() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let scoped = principal(OTHER);

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(OTHER_SUBJECT))
            .await,
    );
    let granted = wire(
        &provider
            .grant(
                &admin,
                restricted_grant("k-scoped-admin", "admin", OTHER, SUBJECT, &["bnd_1"]),
            )
            .await,
    );
    assert_eq!(receipt(&granted)["result"]["revision"], 1);

    let outside_context = wire(
        &provider
            .fence_subject(
                &scoped,
                fence_at_context(SUBJECT, "bnd_2", "fence", 1, "k-fence-outside"),
            )
            .await,
    );
    assert_eq!(
        error_code(&outside_context),
        "NOT_AUTHORIZED",
        "an admin restricted to bnd_1 cannot fence bnd_2"
    );

    let other_subject = wire(
        &provider
            .fence_subject(
                &scoped,
                fence_at_context(OTHER_SUBJECT, "bnd_1", "fence", 1, "k-fence-other-subject"),
            )
            .await,
    );
    assert_eq!(
        error_code(&other_subject),
        "NOT_AUTHORIZED",
        "subject-scoped coverage never reaches another subject"
    );

    let whole_subject = wire(
        &provider
            .fence_subject(
                &scoped,
                fence_at_subject(SUBJECT, "fence", 1, "k-fence-whole"),
            )
            .await,
    );
    assert_eq!(
        error_code(&whole_subject),
        "NOT_AUTHORIZED",
        "a whole-subject fence needs unrestricted grant coverage"
    );

    let archive = wire(
        &provider
            .fence_subject(
                &scoped,
                fence_at_subject(SUBJECT, "archive", 1, "k-archive-whole"),
            )
            .await,
    );
    assert_eq!(error_code(&archive), "NOT_AUTHORIZED");

    let own_context = wire(
        &provider
            .fence_subject(
                &scoped,
                fence_at_context(SUBJECT, "bnd_1", "fence", 1, "k-fence-inside"),
            )
            .await,
    );
    assert_eq!(
        receipt(&own_context)["result"]["fenced_contexts"],
        json!(["bnd_1"])
    );

    let admin_archive = wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_subject(SUBJECT, "archive", 2, "k-archive-admin"),
            )
            .await,
    );
    assert_eq!(receipt(&admin_archive)["result"]["status"], "archived");
}

/// §7.5/§7.7/§7.9: a fenced or archived subject rejects its writes — get-or-create
/// and subject-scoped grants — while space-wide writes stay unaffected.
#[tokio::test]
async fn fenced_and_archived_subjects_reject_writes() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let fenced = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "fence", 1, "k-fence"))
            .await,
    );
    assert_eq!(receipt(&fenced)["result"]["status"], "fenced");

    let ensure_while_fenced = wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    assert_eq!(error_code(&ensure_while_fenced), "SUBJECT_FENCED");
    assert_eq!(error_details(&ensure_while_fenced)["status"], "fenced");
    assert_eq!(retry_class(&ensure_while_fenced), "terminal");

    let grant_while_fenced = wire(
        &provider
            .grant(
                &admin,
                subject_grant("k-grant-fenced", "capture", OTHER, SUBJECT),
            )
            .await,
    );
    assert_eq!(error_code(&grant_while_fenced), "SUBJECT_FENCED");

    let unfenced = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "unfence", 2, "k-unfence"))
            .await,
    );
    assert_eq!(receipt(&unfenced)["result"]["status"], "active");

    let ensure_after_unfence = wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    assert_eq!(
        receipt(&ensure_after_unfence)["result"]["created"],
        false,
        "unfencing restores the subject's writes"
    );

    let archived = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "archive", 3, "k-archive"))
            .await,
    );
    assert_eq!(receipt(&archived)["result"]["status"], "archived");

    let ensure_while_archived = wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    assert_eq!(error_code(&ensure_while_archived), "SUBJECT_FENCED");
    assert_eq!(error_details(&ensure_while_archived)["status"], "archived");

    let grant_while_archived = wire(
        &provider
            .grant(
                &admin,
                subject_grant("k-grant-archived", "capture", OTHER, SUBJECT),
            )
            .await,
    );
    assert_eq!(error_code(&grant_while_archived), "SUBJECT_FENCED");

    // Space-wide issuance names no gated subject, so it still commits.
    let space_wide = wire(
        &provider
            .grant(&admin, grant_request("k-space-wide", "capture", OTHER))
            .await,
    );
    assert_eq!(receipt(&space_wide)["result"]["revision"], 1);
}

/// §7.9 non-amplification with the ISSUED scope: a subject-scoped admin delegates
/// within its own subject only.
#[tokio::test]
async fn subject_scoped_admin_delegates_within_its_subject_only() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let scoped = principal(OTHER);

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(OTHER_SUBJECT))
            .await,
    );
    let scoped_admin = wire(
        &provider
            .grant(&admin, subject_grant("k-scoped", "admin", OTHER, SUBJECT))
            .await,
    );
    assert_eq!(receipt(&scoped_admin)["result"]["revision"], 1);

    let delegated = wire(
        &provider
            .grant(
                &scoped,
                subject_grant("k-delegate", "capture", THIRD, SUBJECT),
            )
            .await,
    );
    assert_eq!(
        receipt(&delegated)["result"]["revision"],
        1,
        "a same-subject admin may delegate within its own subject"
    );

    let other_subject = wire(
        &provider
            .grant(
                &scoped,
                subject_grant("k-delegate-other", "capture", THIRD, OTHER_SUBJECT),
            )
            .await,
    );
    assert_eq!(error_code(&other_subject), "NOT_AUTHORIZED");

    let space_wide = wire(
        &provider
            .grant(&scoped, grant_request("k-delegate-wide", "capture", THIRD))
            .await,
    );
    assert_eq!(
        error_code(&space_wide),
        "NOT_AUTHORIZED",
        "subject-scoped coverage cannot issue a space-wide grant"
    );
}

/// §7.0.5/MEM-R021: a same-key replay returns the stored receipt without
/// re-evaluating the freeze the original op preceded; a different payload under
/// the same key is still `IDEMPOTENCY_CONFLICT`.
#[tokio::test]
async fn replay_is_checked_before_fresh_effect_lifecycle_gates() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);

    let granted = wire(
        &provider
            .grant(&admin, grant_request("k-replay", "capture", OTHER))
            .await,
    );
    let granted_receipt = receipt(&granted)["receipt_id"].as_str().unwrap().to_owned();

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let fenced = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "fence", 1, "k-fence"))
            .await,
    );
    let fence_receipt = receipt(&fenced)["receipt_id"].as_str().unwrap().to_owned();

    let frozen = wire(
        &provider
            .freeze_space(&admin, freeze_request("k-freeze", 1))
            .await,
    );
    assert_eq!(receipt(&frozen)["result"]["status"], "frozen");

    let replay = wire(
        &provider
            .grant(&admin, grant_request("k-replay", "capture", OTHER))
            .await,
    );
    assert_eq!(
        receipt(&replay)["receipt_id"],
        granted_receipt,
        "the retry after freeze replays the original grant receipt, not SPACE_FROZEN"
    );

    let fence_replay = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "fence", 1, "k-fence"))
            .await,
    );
    assert_eq!(
        receipt(&fence_replay)["receipt_id"],
        fence_receipt,
        "the retry after freeze replays the original fence receipt"
    );

    let changed_payload = wire(
        &provider
            .grant(&admin, grant_request("k-replay", "recall", OTHER))
            .await,
    );
    assert_eq!(
        error_code(&changed_payload),
        "IDEMPOTENCY_CONFLICT",
        "the same key with a different payload is never replayed"
    );
}

/// R3 (§7.0.5/MEM-R021): a replay is admitted by *current* coverage and then
/// answered from its receipt — it never re-runs the §7.9 non-amplification the
/// original issuance was checked against. An applicable admin grant acquired
/// later tightens the expiry bound over that same set, which must not turn a
/// committed issuance into a refusal.
#[tokio::test]
async fn a_committed_grant_replay_survives_a_tightened_issuance_bound() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);

    // The provisioned admin grant never expires, so this non-expiring issuance
    // is admitted and committed.
    let issued = wire(
        &provider
            .grant(&admin, grant_request("k-bound-replay", "capture", OTHER))
            .await,
    );
    let receipt_id = receipt(&issued)["receipt_id"].as_str().unwrap().to_owned();

    // An additional, expiring admin grant for the same principal: the §7.9
    // applicable set now carries a finite earliest expiry.
    let expiring_admin = dto::<schemas::MemoryGrantsGrantRequest>(json!({
        "space_id": SPACE,
        "principal_id": CREATOR,
        "capabilities": ["admin"],
        "expires_at": (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
        "idempotency_key": "k-bound-admin",
    }));
    let tightened = wire(&provider.grant(&admin, expiring_admin).await);
    assert_eq!(receipt(&tightened)["result"]["revision"], 1);

    // A *fresh* non-expiring issuance is now refused under that bound ...
    let fresh = wire(
        &provider
            .grant(&admin, grant_request("k-bound-fresh", "capture", OTHER))
            .await,
    );
    assert_eq!(
        error_code(&fresh),
        "NOT_AUTHORIZED",
        "the bound really did tighten: {fresh}"
    );

    // ... while the identical retry still returns its committed receipt.
    let replay = wire(
        &provider
            .grant(&admin, grant_request("k-bound-replay", "capture", OTHER))
            .await,
    );
    assert_eq!(
        receipt(&replay)["receipt_id"],
        receipt_id,
        "a replay is authorized by current coverage, never re-validated as a fresh issuance"
    );
}

/// R2 (§6.5): the authorization clock is read *after* the write lock. A grant
/// that expires while the write waits for `BEGIN IMMEDIATE` cannot authorize it,
/// even though it was valid when the request arrived.
#[tokio::test]
async fn a_grant_expiring_during_the_lock_wait_does_not_authorize_the_write() {
    let env = seed_env();
    let provider = Arc::new(open_provider(&env).await);
    let pool = nexus_local_db::open_pool(&env.db_path).await.unwrap();

    // `expires_at` is stored second-truncated (`storage::timestamp`), so a
    // sub-second TTL would land in the past and prove nothing. Place it on a
    // whole second 400–1400ms ahead: the request clock the op captures when it
    // starts is then still valid, while the post-lock clock is not.
    let mut expires_at = chrono::Utc::now().trunc_subsecs(0) + chrono::Duration::seconds(1);
    if expires_at - chrono::Utc::now() < chrono::Duration::milliseconds(400) {
        expires_at += chrono::Duration::seconds(1);
    }
    let expiring = dto::<schemas::MemoryGrantsGrantRequest>(json!({
        "space_id": SPACE,
        "subject_id": OTHER_SUBJECT,
        "principal_id": OTHER,
        "capabilities": ["capture"],
        "expires_at": expires_at.to_rfc3339(),
        "idempotency_key": "k-expiring-capture",
    }));
    let issued = wire(&provider.grant(&principal(CREATOR), expiring).await);
    assert_eq!(receipt(&issued)["result"]["revision"], 1);

    // A competing writer parks the op on the write lock across that expiry.
    let held = nexus_local_db::begin_immediate(&pool).await.unwrap();
    let writer = Arc::clone(&provider);
    let pending = tokio::spawn(async move {
        writer
            .ensure_subject(&principal(OTHER), ensure_request(OTHER_SUBJECT))
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !pending.is_finished(),
        "the write is parked on the competing writer's lock"
    );
    // Release only after the grant has expired, and still inside the 2s
    // busy_timeout the connection sets.
    let until_expired = (expires_at + chrono::Duration::milliseconds(100) - chrono::Utc::now())
        .to_std()
        .unwrap_or_default();
    tokio::time::sleep(until_expired).await;

    held.rollback().await.unwrap();
    let refused = wire(&pending.await.unwrap());
    assert_eq!(
        error_code(&refused),
        "NOT_AUTHORIZED",
        "coverage is evaluated at the post-lock clock, not the request clock"
    );

    let absent = wire(
        &provider
            .get_subject(
                &principal(CREATOR),
                dto(json!({ "space_id": SPACE, "subject_id": OTHER_SUBJECT })),
            )
            .await,
    );
    assert_eq!(
        error_code(&absent),
        "NOT_FOUND",
        "the expired grant authorizes no effect: {absent}"
    );
}

/// §7.0.7/MEM-R026: a cursor is bound to principal + space + subject +
/// principal filter + policy version + snapshot marker, and a grant write
/// between pages invalidates it instead of silently re-filtering.
#[tokio::test]
async fn cursors_bind_the_query_and_invalidate_on_change() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(OTHER_SUBJECT))
            .await,
    );
    wire(
        &provider
            .grant(&admin, subject_grant("k-a", "capture", OTHER, SUBJECT))
            .await,
    );
    wire(
        &provider
            .grant(&admin, subject_grant("k-b", "recall", OTHER, SUBJECT))
            .await,
    );
    wire(
        &provider
            .grant(
                &admin,
                subject_grant("k-c", "capture", OTHER, OTHER_SUBJECT),
            )
            .await,
    );

    let first = wire(
        &provider
            .list_grants(&admin, list_grants(Some(SUBJECT), None, Some(1), None))
            .await,
    );
    assert_eq!(first["grants"].as_array().unwrap().len(), 1);
    let cursor = first["next_cursor"]
        .as_str()
        .expect("a filtered page of two grants issues a cursor")
        .to_owned();

    let second = wire(
        &provider
            .list_grants(
                &admin,
                list_grants(Some(SUBJECT), None, Some(1), Some(&cursor)),
            )
            .await,
    );
    assert_eq!(second["grants"].as_array().unwrap().len(), 1);
    assert_eq!(
        second.get("next_cursor"),
        None,
        "the second page ends the bound listing"
    );

    let cross_filter = wire(
        &provider
            .list_grants(
                &admin,
                list_grants(Some(OTHER_SUBJECT), None, Some(1), Some(&cursor)),
            )
            .await,
    );
    assert_eq!(
        error_code(&cross_filter),
        "CURSOR_INVALID",
        "a cursor never carries over to another filter"
    );

    let cross_principal = wire(
        &provider
            .list_grants(
                &principal(OTHER),
                list_grants(Some(SUBJECT), None, Some(1), Some(&cursor)),
            )
            .await,
    );
    assert_eq!(
        error_code(&cross_principal),
        "CURSOR_INVALID",
        "a cursor is bound to the principal that minted it"
    );

    // A grant write moves the marker, so the page-one cursor dies (§7.0.7).
    wire(
        &provider
            .grant(&admin, subject_grant("k-d", "capture", THIRD, SUBJECT))
            .await,
    );
    let invalidated = wire(
        &provider
            .list_grants(
                &admin,
                list_grants(Some(SUBJECT), None, Some(1), Some(&cursor)),
            )
            .await,
    );
    assert_eq!(
        error_code(&invalidated),
        "CURSOR_INVALID",
        "a grant change between pages invalidates the cursor"
    );
}

/// §6.1 + plan Global Constraints: `payload_digest` is the SHA-256 of the JCS
/// canonicalization of `{op, payload}`. The vector below was computed
/// independently (Python `json.dumps(..., sort_keys=True, separators=(",", ":"))`
/// over `{"op": "memory.spaces.freeze", "payload": {...}}`) rather than read back
/// from the provider.
#[tokio::test]
async fn payload_digest_binds_the_operation_and_the_canonical_payload() {
    const FREEZE_DIGEST_VECTOR: &str =
        "0fb2daf7b8909d3a73c5d9961a8deb8549aee99387b86d894b39eeccdd08619d";

    let env = seed_env();
    let provider = open_provider(&env).await;
    let body = wire(
        &provider
            .freeze_space(&principal(CREATOR), freeze_request("k-digest", 1))
            .await,
    );
    let receipt = receipt(&body);
    let digest = receipt["payload_digest"].as_str().expect("digest present");
    assert_eq!(digest.len(), 64, "64 lowercase hex chars (§6.1)");
    assert!(
        digest
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "lowercase hex only: {digest}"
    );
    assert_eq!(
        digest, FREEZE_DIGEST_VECTOR,
        "the op-bound JCS digest matches the pinned cross-component vector"
    );
}

/// §14.2/§14.3: a failure at or after `COMMIT` is `OUTCOME_UNCERTAIN`, never a
/// clean retryable failure, and same-key replay/settlement resolves it.
#[tokio::test]
async fn commit_boundary_failures_are_outcome_uncertain() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);

    // The COMMIT succeeded but the response was lost: the effect is committed.
    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .grant(&admin, grant_request("k-uncertain", "capture", OTHER))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class(&lost), "uncertain");

    let resolved = wire(
        &provider
            .grant(&admin, grant_request("k-uncertain", "capture", OTHER))
            .await,
    );
    let resolved = receipt(&resolved);
    assert_eq!(resolved["op"], "memory.grants.grant");
    assert_eq!(
        resolved["status"], "committed",
        "the lost response is recovered by same-key replay (§14.3)"
    );
    let view = wire(
        &provider
            .list_grants(&principal(OTHER), dto(json!({ "space_id": SPACE })))
            .await,
    );
    assert_eq!(
        view["grants"].as_array().unwrap().len(),
        1,
        "the committed effect exists exactly once"
    );

    // The COMMIT itself failed: the outcome is unknown, so it is not reported as
    // a clean no-effect retryable either.
    provider.inject_commit_fault(false);
    let unknown = wire(
        &provider
            .grant(&admin, grant_request("k-unknown", "capture", THIRD))
            .await,
    );
    assert_eq!(error_code(&unknown), "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class(&unknown), "uncertain");

    let retried = wire(
        &provider
            .grant(&admin, grant_request("k-unknown", "capture", THIRD))
            .await,
    );
    assert_eq!(receipt(&retried)["result"]["revision"], 1);
}

/// §14.2: a storage failure raised strictly BEFORE `COMMIT` asserts no effect and
/// stays a retryable `INTERNAL_ERROR`.
#[tokio::test]
async fn pre_commit_storage_failures_are_retryable_and_leave_no_effect() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let pool = nexus_local_db::open_pool(&env.db_path).await.unwrap();

    // A real storage error before COMMIT: the receipt insert always aborts.
    sqlx::query(
        "CREATE TRIGGER test_fail_receipt BEFORE INSERT ON memory_receipts \
         BEGIN SELECT RAISE(ABORT, 'injected receipt failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let failed = wire(
        &provider
            .grant(&admin, grant_request("k-precommit", "capture", OTHER))
            .await,
    );
    assert_eq!(error_code(&failed), "INTERNAL_ERROR");
    assert_eq!(retry_class(&failed), "retryable");

    let view = wire(
        &provider
            .list_grants(
                &admin,
                dto(json!({ "space_id": SPACE, "principal_id": OTHER })),
            )
            .await,
    );
    assert!(
        view["grants"].as_array().unwrap().is_empty(),
        "a pre-commit failure leaves no effect: {view}"
    );

    sqlx::query("DROP TRIGGER test_fail_receipt")
        .execute(&pool)
        .await
        .unwrap();
    let retried = wire(
        &provider
            .grant(&admin, grant_request("k-precommit", "capture", OTHER))
            .await,
    );
    assert_eq!(
        receipt(&retried)["result"]["revision"],
        1,
        "the retry with the same key re-executes and commits"
    );
}

/// §7.3/§11: admission and effect are serialized with the freeze transition. A
/// freeze transaction holding the write lock blocks the op's admission, so after
/// it commits the op sees the frozen space and is refused — never admitted on the
/// pre-freeze state and committed after the barrier.
#[tokio::test]
async fn freeze_serializes_with_admitted_writes() {
    let env = seed_env();
    let provider = Arc::new(open_provider(&env).await);
    let pool = nexus_local_db::open_pool(&env.db_path).await.unwrap();

    // The uncommitted freeze holds the write lock for this whole block.
    let mut freeze = nexus_local_db::begin_immediate(&pool).await.unwrap();
    sqlx::query(
        "UPDATE memory_spaces SET status = 'frozen', revision = revision + 1 WHERE space_id = ?",
    )
    .bind(SPACE)
    .execute(&mut *freeze)
    .await
    .unwrap();

    let writer = Arc::clone(&provider);
    let pending = tokio::spawn(async move {
        writer
            .grant(
                &principal(CREATOR),
                grant_request("k-race", "capture", OTHER),
            )
            .await
    });

    // Deterministic: the op cannot begin its transaction (hence cannot admit)
    // while the freeze transaction is open; 100ms is far inside busy_timeout.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !pending.is_finished(),
        "the op must wait for the freeze transaction (§7.3/§11)"
    );

    freeze.commit().await.unwrap();
    let refused = wire(&pending.await.unwrap());
    assert_eq!(
        error_code(&refused),
        "SPACE_FROZEN",
        "an op that begins after the freeze commit is refused"
    );

    let view = wire(
        &provider
            .list_grants(
                &principal(CREATOR),
                dto(json!({ "space_id": SPACE, "principal_id": OTHER })),
            )
            .await,
    );
    assert!(
        view["grants"].as_array().unwrap().is_empty(),
        "no write is committed after the freeze barrier: {view}"
    );
}

/// §11: an op that is admitted before a competing writer performs its work is not
/// lost — it completes and keeps its receipt once the writer releases the lock.
#[tokio::test]
async fn admitted_write_completes_after_a_competing_writer_releases() {
    let env = seed_env();
    let provider = Arc::new(open_provider(&env).await);
    let pool = nexus_local_db::open_pool(&env.db_path).await.unwrap();

    let competing = nexus_local_db::begin_immediate(&pool).await.unwrap();

    let writer = Arc::clone(&provider);
    let pending = tokio::spawn(async move {
        writer
            .grant(
                &principal(CREATOR),
                grant_request("k-admitted", "capture", OTHER),
            )
            .await
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !pending.is_finished(),
        "the op waits for the competing writer's transaction"
    );

    // The competing writer never takes effect; the op then commits in full.
    competing.rollback().await.unwrap();
    let committed = wire(&pending.await.unwrap());
    assert_eq!(receipt(&committed)["result"]["revision"], 1);

    let view = wire(
        &provider
            .list_grants(&principal(OTHER), dto(json!({ "space_id": SPACE })))
            .await,
    );
    assert_eq!(
        view["grants"].as_array().unwrap().len(),
        1,
        "the admitted write is neither lost nor half-applied"
    );
}

// C-117: both memory migrations advanced the schema version line (25 → 26 → 27).
const _: () = assert!(nexus_local_db::DB_SCHEMA_VERSION >= 27);

/// C-117: the control- and content-metadata migrations land once, re-run as
/// no-ops, are writer-guard registered (guards + outbox on every control and
/// content table), and the `DB_SCHEMA_VERSION` line advanced with them.
#[tokio::test]
async fn control_metadata_migration_is_idempotent_and_writer_registered() {
    let env = seed_env();

    let provider = open_provider(&env).await;
    let pool = nexus_local_db::open_pool(&env.db_path).await.unwrap();

    for migration in ["memory_control_metadata", "memory_content_metadata"] {
        let (applied,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations WHERE description LIKE ?")
                .bind(format!("%{migration}%"))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(applied, 1, "migration {migration} applies exactly once");
    }

    let tables = [
        "memory_spaces",
        "memory_subjects",
        "memory_grants",
        "memory_receipts",
        "memory_candidates",
        "memory_records",
        "memory_profiles",
    ];
    for table in tables {
        let (found,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
                .bind(table)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(found, 1, "missing memory table: {table}");
    }

    for table in tables {
        for (kind, verb) in [
            ("guard", "insert"),
            ("guard", "update"),
            ("guard", "delete"),
            ("outbox", "insert"),
            ("outbox", "update"),
            ("outbox", "delete"),
        ] {
            let name = format!("{kind}_{table}_{verb}");
            let (found,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name=?",
            )
            .bind(&name)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(found, 1, "missing writer-protocol trigger: {name}");
        }
    }

    let (registered,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM core_writer_registration r \
         INNER JOIN core_workspace_gate g ON g.pk = 1 \
         WHERE r.mode = 'direct' AND r.migration_epoch = g.migration_epoch",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        registered, 1,
        "the provider's writer is registered and epoch-matched"
    );

    let versions = nexus_local_db::read_versions(&pool).await.unwrap();
    assert_eq!(
        versions.db_schema_version,
        nexus_local_db::DB_SCHEMA_VERSION,
        "the seeded version line is current"
    );

    drop(provider);
    let reopened = open_provider(&env).await;
    let still_here = wire(
        &reopened
            .list_spaces(&principal(CREATOR), dto(json!({})))
            .await,
    );
    assert_eq!(
        still_here["spaces"].as_array().unwrap().len(),
        1,
        "re-running the migration set neither duplicates nor drops the space"
    );
}

/// C-110: a write outside the writer guard trips `WRITER_FENCED` — the
/// control tables are admitted through the core writer protocol only.
#[tokio::test]
async fn out_of_guard_writes_are_writer_fenced() {
    let env = seed_env();
    let _provider = open_provider(&env).await;

    // A connection with the protocol scalar functions installed but NO
    // `core_writer_registration` row: exactly the "outside the guard" writer the
    // C-110 fence exists for.
    let mut raw = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&env.db_path),
    )
    .await
    .unwrap();
    nexus_storage_guard::install_writer_functions(
        &mut raw,
        nexus_storage_guard::WriterConnectionContext {
            writer_id: "unregistered-test-writer".to_string(),
            protocol_version: 1,
            mode: nexus_storage_guard::WriterMode::Direct,
            migration_epoch: 0,
            engine_epoch: None,
        },
    )
    .await
    .unwrap();

    let err = sqlx::query(
        "INSERT INTO memory_spaces \
         (space_id, display_name, authority_host_id, authority_generation, status, revision, created_at) \
         VALUES ('spc_rogue', NULL, 'rogue', 1, 'active', 1, '2026-11-01T00:00:00Z')",
    )
    .execute(&mut raw)
    .await
    .expect_err("an unregistered connection must be fenced");
    assert!(
        err.to_string().contains("WRITER_FENCED"),
        "expected WRITER_FENCED, got: {err}"
    );

    let err = sqlx::query("UPDATE memory_spaces SET status = 'frozen' WHERE space_id = ?")
        .bind(SPACE)
        .execute(&mut raw)
        .await
        .expect_err("an unregistered pool must be fenced for updates too");
    assert!(
        err.to_string().contains("WRITER_FENCED"),
        "expected WRITER_FENCED, got: {err}"
    );

    // The round-B content tables carry the same admission: a record cannot be
    // written outside the writer guard either.
    let err = sqlx::query(
        "INSERT INTO memory_records \
         (record_id, space_id, subject_id, kind, content_json, scope_visibility, scope_context_id, \
          provenance_json, import_ref_json, occurred_at, tombstone_json, revision, created_at, updated_at) \
         VALUES ('rec_rogue', ?, 'sbj_mira', 'note', '{\"text\":\"rogue\"}', 'shared', NULL, \
                 '{}', NULL, NULL, NULL, 1, '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
    )
    .bind(SPACE)
    .execute(&mut raw)
    .await
    .expect_err("a content write outside the guard must be fenced");
    assert!(
        err.to_string().contains("WRITER_FENCED"),
        "expected WRITER_FENCED for a record insert, got: {err}"
    );
}

/// Compass AC #3: bearer path validation is untouched by the migration — the
/// same `ctr_` prefix and path-safety rules accept and reject exactly what
/// they did before it.
#[tokio::test]
async fn bearer_path_validation_is_unchanged() {
    assert!(MemoryBearerRef::Creator(CREATOR).validate().is_ok());
    for rejected in [
        "",
        "creator_1",
        "ctr_",
        "ctr_../escape",
        "ctr_aa/bb",
        "ctr_aa\\bb",
    ] {
        assert!(
            MemoryBearerRef::Creator(rejected).validate().is_err(),
            "expected {rejected:?} to stay rejected"
        );
    }

    let env = seed_env();
    let dir = MemoryBearerRef::Creator(CREATOR)
        .validated_long_term_memory_dir(env.user_home.as_path())
        .expect("validated layout");
    assert_eq!(
        dir,
        creator_memory_dir(env.user_home.as_path()),
        "the Creator bearer layout is byte-identical"
    );
}

fn creator_memory_dir(user_home: &Path) -> PathBuf {
    user_home
        .join(".nexus42")
        .join("creators")
        .join(CREATOR)
        .join("memory")
        .join("long-term")
}

// ── The content families ───────────────────────────────────────────────

const TEXT: &str = "Mira promised to return before the northern gate opens at dawn.";

fn capture_request(
    key: &str,
    context_id: Option<&str>,
    text: &str,
) -> schemas::MemoryCaptureRequest {
    let capture_scope = context_id.map_or_else(
        || json!({ "visibility": "shared" }),
        |context_id| json!({ "visibility": "context", "context_id": context_id }),
    );
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "content": { "text": text },
        "capture_scope": capture_scope,
        "source_context_ids": if context_id.is_some() { json!([context_id]) } else { json!([]) },
        "idempotency_key": key,
    }))
}

fn candidates_list_request(status: Option<&str>) -> schemas::MemoryCandidatesListRequest {
    status.map_or_else(
        || dto(json!({ "space_id": SPACE, "subject_id": SUBJECT })),
        |status| dto(json!({ "space_id": SPACE, "subject_id": SUBJECT, "status": status })),
    )
}

fn candidate_get_request(candidate_id: &str) -> schemas::MemoryCandidatesGetRequest {
    dto(json!({ "space_id": SPACE, "candidate_id": candidate_id }))
}

fn review_request(
    key: &str,
    candidate_id: &str,
    revision: u64,
) -> schemas::MemoryCandidatesReviewRequest {
    dto(json!({
        "space_id": SPACE,
        "candidate_id": candidate_id,
        "expected_revision": revision,
        "decision": { "action": "promote" },
        "idempotency_key": key,
    }))
}

fn record_create_request(
    key: &str,
    kind: &str,
    scope: &Value,
    text: &str,
) -> schemas::MemoryRecordsCreateRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "kind": kind,
        "content": { "text": text },
        "scope": scope,
        "idempotency_key": key,
    }))
}

fn record_get_request(record_id: &str) -> schemas::MemoryRecordsGetRequest {
    dto(json!({ "space_id": SPACE, "record_id": record_id }))
}

fn recall_request(max_items: u64) -> schemas::MemoryRecallRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "selection": { "mode": "subject_all" },
        "budget": { "max_items": max_items },
    }))
}

/// §7.22 recall with the ranking and paging inputs the cursor binds.
fn recall_page(
    max_items: u64,
    query: Option<&str>,
    cursor: Option<&str>,
) -> schemas::MemoryRecallRequest {
    let mut request = json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "selection": { "mode": "subject_all" },
        "budget": { "max_items": max_items },
    });
    if let Some(query) = query {
        request["query"] = json!(query);
    }
    if let Some(cursor) = cursor {
        request["cursor"] = json!(cursor);
    }
    dto(request)
}

/// §7.11→§7.15→§7.22 end to end: capture produces a pending candidate, recall
/// never returns it (MEM-R004), review consumes it into a record with the
/// candidate's placement while scrubbing the body and keeping its digest
/// (MEM-R009/§6.7), and recall then serves the record under current
/// authorization.
#[tokio::test]
async fn capture_review_promote_and_recall_round_trip() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );

    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-capture", None, TEXT))
            .await,
    );
    let captured = receipt(&captured);
    assert_eq!(captured["op"], "memory.capture");
    assert_eq!(captured["result"]["classified"], 1);
    let candidate_id = captured["result"]["candidate_ids"][0]
        .as_str()
        .expect("a committed capture names its candidate")
        .to_owned();
    assert!(
        !captured["result"].to_string().contains("Mira"),
        "receipts are content-free (MEM-R022): {captured}"
    );

    // §7.12: the pending candidate is listed with its body and advisory verdict.
    let listed = wire(
        &provider
            .list_candidates(&admin, candidates_list_request(Some("pending")))
            .await,
    );
    let prompt = &listed["candidates"][0];
    assert_eq!(prompt["candidate_id"], candidate_id);
    assert_eq!(prompt["status"], "pending");
    assert_eq!(prompt["proposed"]["content"]["text"], TEXT);
    assert_eq!(prompt["proposed"]["kind"], "nexus.custom");
    assert_eq!(prompt["classification"]["disposition"], "promote");
    assert_eq!(prompt["capture_scope"]["visibility"], "shared");

    // MEM-R004: a candidate is never a recall result, and nothing is retained yet.
    let before_review = wire(&provider.recall(&admin, recall_request(10)).await);
    assert_eq!(before_review["total_authorized"], 0);
    assert!(before_review["records"].as_array().unwrap().is_empty());

    let reviewed = wire(
        &provider
            .review_candidate(&admin, review_request("k-review", &candidate_id, 1))
            .await,
    );
    let reviewed = receipt(&reviewed);
    assert_eq!(reviewed["result"]["outcome"], "promoted");
    let record_id = reviewed["result"]["record_ids"][0]
        .as_str()
        .expect("a promoted candidate names its record")
        .to_owned();

    // §6.7 scrub: the shell keeps the digest and the outcome, never the body.
    let shell = wire(
        &provider
            .get_candidate(&admin, candidate_get_request(&candidate_id))
            .await,
    );
    let shell = &shell["candidate"];
    assert_eq!(shell["status"], "consumed");
    assert!(
        shell["proposed"].get("content").is_none(),
        "consumption removes the proposed body: {shell}"
    );
    let digest = shell["content_digest"].as_str().expect("digest is set");
    assert_eq!(digest.len(), 64, "§6.1 64 lowercase hex chars");
    assert_eq!(shell["consumed_by"]["outcome"], "promoted");
    assert_eq!(shell["consumed_by"]["receipt_id"], reviewed["receipt_id"]);
    assert_eq!(shell["consumed_by"]["record_ids"][0], record_id);

    // §7.17: the record exists with the placement the candidate was captured under.
    let record = wire(
        &provider
            .get_record(&admin, record_get_request(&record_id))
            .await,
    );
    assert_eq!(record["record"]["scope"]["visibility"], "shared");
    assert_eq!(record["record"]["content"]["text"], TEXT);
    assert_eq!(record["record"]["kind"], "nexus.custom");
    assert_eq!(record["record"]["provenance"]["captured_by"], CREATOR);

    let recalled = wire(&provider.recall(&admin, recall_request(10)).await);
    assert_eq!(recalled["total_authorized"], 1);
    assert_eq!(recalled["records"][0]["record_id"], record_id);
    assert_eq!(recalled["truncated"], false);
    assert!(
        !recalled["engine_policy_version"]
            .as_str()
            .unwrap()
            .is_empty(),
        "recall stamps the policy version its ordering came from: {recalled}"
    );
}

/// §7.0.5/MEM-R008: a same-key same-payload capture replays its committed
/// receipt without inserting a second candidate; a different payload under the
/// same key is `IDEMPOTENCY_CONFLICT`.
#[tokio::test]
async fn duplicate_capture_replays_the_receipt() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );

    let first = wire(
        &provider
            .capture(&admin, capture_request("k-replay-capture", None, TEXT))
            .await,
    );
    let first = receipt(&first);
    let candidate_id = first["result"]["candidate_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();

    let replay = wire(
        &provider
            .capture(&admin, capture_request("k-replay-capture", None, TEXT))
            .await,
    );
    assert_eq!(
        receipt(&replay)["receipt_id"],
        first["receipt_id"],
        "the retry returns the stored receipt without re-executing"
    );
    assert_eq!(receipt(&replay)["result"]["candidate_ids"][0], candidate_id);

    let listed = wire(
        &provider
            .list_candidates(&admin, candidates_list_request(None))
            .await,
    );
    assert_eq!(
        listed["candidates"].as_array().unwrap().len(),
        1,
        "a replay never inserts a second candidate: {listed}"
    );

    let conflict = wire(
        &provider
            .capture(
                &admin,
                capture_request(
                    "k-replay-capture",
                    None,
                    "A different promise under the same idempotency key.",
                ),
            )
            .await,
    );
    assert_eq!(error_code(&conflict), "IDEMPOTENCY_CONFLICT");
    assert_eq!(retry_class(&conflict), "terminal");
}

/// §8.1/MEM-R007: review is revision-checked, and consumption is terminal —
/// a second decision on the shell is `CANDIDATE_CONSUMED`, not a second record.
#[tokio::test]
async fn review_is_revision_checked_and_consumption_is_terminal() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-capture-cas", None, TEXT))
            .await,
    );
    let candidate_id = receipt(&captured)["result"]["candidate_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();

    let stale = wire(
        &provider
            .review_candidate(&admin, review_request("k-review-stale", &candidate_id, 2))
            .await,
    );
    assert_eq!(error_code(&stale), "REVISION_CONFLICT");
    assert_eq!(error_details(&stale)["current_revision"], 1);

    let promoted = wire(
        &provider
            .review_candidate(&admin, review_request("k-review-once", &candidate_id, 1))
            .await,
    );
    assert_eq!(receipt(&promoted)["result"]["outcome"], "promoted");

    let again = wire(
        &provider
            .review_candidate(&admin, review_request("k-review-twice", &candidate_id, 2))
            .await,
    );
    assert_eq!(error_code(&again), "CANDIDATE_CONSUMED");
    assert_eq!(retry_class(&again), "terminal");

    let recalled = wire(&provider.recall(&admin, recall_request(10)).await);
    assert_eq!(
        recalled["total_authorized"], 1,
        "the consumed candidate produced exactly one record: {recalled}"
    );
}

/// §7.14: dismissal consumes the candidate without producing a record, and the
/// outcome is atomic with the scrub (MEM-R009).
#[tokio::test]
async fn dismissing_a_candidate_consumes_it_without_a_record() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-capture-dismiss", None, TEXT))
            .await,
    );
    let candidate_id = receipt(&captured)["result"]["candidate_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();

    let dismissed = wire(
        &provider
            .dismiss_candidate(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "candidate_id": candidate_id,
                    "expected_revision": 1,
                    "reason": "duplicate of an existing note",
                    "idempotency_key": "k-dismiss",
                })),
            )
            .await,
    );
    let dismissed = receipt(&dismissed);
    assert_eq!(dismissed["result"]["outcome"], "dismissed");
    assert_eq!(dismissed["result"]["candidate_id"], candidate_id);

    let shell = wire(
        &provider
            .get_candidate(&admin, candidate_get_request(&candidate_id))
            .await,
    );
    assert_eq!(shell["candidate"]["status"], "consumed");
    assert_eq!(shell["candidate"]["consumed_by"]["outcome"], "dismissed");
    assert_eq!(shell["candidate"]["consumed_by"]["record_ids"], json!([]));

    let recalled = wire(&provider.recall(&admin, recall_request(10)).await);
    assert_eq!(recalled["total_authorized"], 0);
}

/// §7.18/§7.19/§7.20 + MEM-R007/R014: update is CAS-checked, delete keeps a
/// tombstone with no content, the tombstone is admin-only, and a tombstoned
/// record leaves recall.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn record_update_and_delete_tombstone_the_content() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let reader = principal(OTHER);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    wire(
        &provider
            .grant(
                &admin,
                subject_grant("k-reader", "records_read", OTHER, SUBJECT),
            )
            .await,
    );
    let created = wire(
        &provider
            .create_record(
                &admin,
                record_create_request(
                    "k-record",
                    "nexus.story_summary",
                    &json!({ "visibility": "shared" }),
                    TEXT,
                ),
            )
            .await,
    );
    let record_id = receipt(&created)["result"]["record_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(receipt(&created)["result"]["revision"], 1);

    let visible = wire(
        &provider
            .get_record(&reader, record_get_request(&record_id))
            .await,
    );
    assert_eq!(visible["record"]["record_id"], record_id);

    let stale = wire(
        &provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": record_id,
                    "expected_revision": 5,
                    "content": { "text": "rewritten" },
                    "idempotency_key": "k-update-stale",
                })),
            )
            .await,
    );
    assert_eq!(error_code(&stale), "REVISION_CONFLICT");
    assert_eq!(error_details(&stale)["current_revision"], 1);

    let updated = wire(
        &provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": record_id,
                    "expected_revision": 1,
                    "kind": "nexus.character_note",
                    "idempotency_key": "k-update",
                })),
            )
            .await,
    );
    assert_eq!(receipt(&updated)["result"]["revision"], 2);

    let deleted = wire(
        &provider
            .delete_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": record_id,
                    "expected_revision": 2,
                    "reason": "superseded",
                    "idempotency_key": "k-delete",
                })),
            )
            .await,
    );
    assert_eq!(receipt(&deleted)["result"]["tombstoned"], true);

    let admin_view = wire(
        &provider
            .get_record(&admin, record_get_request(&record_id))
            .await,
    );
    let tombstone = &admin_view["record"];
    assert_eq!(
        tombstone["content"]["text"], "",
        "deletion removes the content (MEM-R014): {admin_view}"
    );
    assert_eq!(tombstone["kind"], "nexus.character_note");
    assert_eq!(tombstone["tombstone"]["deleted_by"], CREATOR);
    assert_eq!(tombstone["tombstone"]["reason"], "superseded");

    let reader_view = wire(
        &provider
            .get_record(&reader, record_get_request(&record_id))
            .await,
    );
    assert_eq!(
        error_code(&reader_view),
        "NOT_FOUND",
        "a tombstone is admin-only (§7.17)"
    );

    let recalled = wire(&provider.recall(&admin, recall_request(10)).await);
    assert_eq!(recalled["total_authorized"], 0);

    let with_tombstones = wire(
        &provider
            .list_records(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "include_tombstoned": true,
                })),
            )
            .await,
    );
    assert_eq!(with_tombstones["total_authorized"], 1);

    let reader_tombstones = wire(
        &provider
            .list_records(
                &reader,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "include_tombstoned": true,
                })),
            )
            .await,
    );
    assert_eq!(
        error_code(&reader_tombstones),
        "NOT_AUTHORIZED",
        "include_tombstoned is admin-only (§7.20)"
    );
}

/// §7.21: promotion moves placement explicitly, and the new placement is what
/// authorization applies — a context-limited reader loses the record once it
/// becomes shared, and regains it when it is promoted back.
#[tokio::test]
async fn promote_moves_placement_and_authorization_follows_it() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let scoped = principal(OTHER);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    wire(
        &provider
            .grant(
                &admin,
                restricted_grant("k-scoped-read", "records_read", OTHER, SUBJECT, &["bnd_1"]),
            )
            .await,
    );
    let created = wire(
        &provider
            .create_record(
                &admin,
                record_create_request(
                    "k-context-record",
                    "nexus.character_note",
                    &json!({ "visibility": "context", "context_id": "bnd_1" }),
                    TEXT,
                ),
            )
            .await,
    );
    let record_id = receipt(&created)["result"]["record_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let scoped_view = wire(
        &provider
            .get_record(&scoped, record_get_request(&record_id))
            .await,
    );
    assert_eq!(scoped_view["record"]["scope"]["context_id"], "bnd_1");

    let promoted = wire(
        &provider
            .promote_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": record_id,
                    "expected_revision": 1,
                    "target_scope": { "visibility": "shared" },
                    "idempotency_key": "k-promote-shared",
                })),
            )
            .await,
    );
    let promoted = receipt(&promoted);
    assert_eq!(promoted["result"]["revision"], 2);
    assert_eq!(
        promoted["result"]["scope"],
        json!({ "visibility": "shared" })
    );

    let lost = wire(
        &provider
            .get_record(&scoped, record_get_request(&record_id))
            .await,
    );
    assert_eq!(
        error_code(&lost),
        "NOT_FOUND",
        "a context-limited grant never covers a shared placement"
    );

    let demoted = wire(
        &provider
            .promote_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": record_id,
                    "expected_revision": 2,
                    "target_scope": { "visibility": "context", "context_id": "bnd_1" },
                    "idempotency_key": "k-promote-context",
                })),
            )
            .await,
    );
    assert_eq!(receipt(&demoted)["result"]["revision"], 3);
    let regained = wire(
        &provider
            .get_record(&scoped, record_get_request(&record_id))
            .await,
    );
    assert_eq!(regained["record"]["record_id"], record_id);
}

/// MEM-R006: candidate listing filters by coverage *before* it pages, and review
/// needs both `review` and `records_write` over the candidate's capture scope.
#[tokio::test]
async fn candidate_reads_and_reviews_are_limited_to_covered_scopes() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let scoped = principal(OTHER);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let covered = wire(
        &provider
            .capture(&admin, capture_request("k-ctx-1", Some("bnd_1"), TEXT))
            .await,
    );
    let covered_id = receipt(&covered)["result"]["candidate_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let outside = wire(
        &provider
            .capture(&admin, capture_request("k-ctx-2", Some("bnd_2"), TEXT))
            .await,
    );
    let outside_id = receipt(&outside)["result"]["candidate_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();

    // Only `review`, restricted to bnd_1.
    wire(
        &provider
            .grant(
                &admin,
                restricted_grant("k-scoped-review", "review", OTHER, SUBJECT, &["bnd_1"]),
            )
            .await,
    );

    let listed = wire(
        &provider
            .list_candidates(&scoped, candidates_list_request(None))
            .await,
    );
    let visible = listed["candidates"].as_array().unwrap();
    assert_eq!(
        visible.len(),
        1,
        "only the covered candidate is listed: {listed}"
    );
    assert_eq!(visible[0]["candidate_id"], covered_id);

    let hidden = wire(
        &provider
            .get_candidate(&scoped, candidate_get_request(&outside_id))
            .await,
    );
    assert_eq!(
        error_code(&hidden),
        "NOT_FOUND",
        "an uncovered candidate reads as absent"
    );

    // §7.15 needs `records_write` over the same scope; this principal lacks it.
    let refused = wire(
        &provider
            .review_candidate(&scoped, review_request("k-review-scoped", &covered_id, 1))
            .await,
    );
    assert_eq!(error_code(&refused), "NOT_AUTHORIZED");

    // A reader with no review grant sees nothing at all rather than an empty page.
    let unprivileged = wire(
        &provider
            .list_candidates(&principal(THIRD), candidates_list_request(None))
            .await,
    );
    assert_eq!(error_code(&unprivileged), "NOT_AUTHORIZED");
}

/// §9: recall ranks deterministically, applies the byte budget as a hard limit,
/// and surfaces an oversized first item instead of dropping it.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn recall_ranks_by_query_and_surfaces_budget_overflow() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    for (key, text) in [
        (
            "k-rec-1",
            "The northern gate opens at dawn for the caravan.",
        ),
        ("k-rec-2", "The southern harbour closes on the first storm."),
        ("k-rec-3", "Mira keeps the ledger of every debt she owes."),
    ] {
        wire(
            &provider
                .create_record(
                    &admin,
                    record_create_request(
                        key,
                        "nexus.world_building",
                        &json!({ "visibility": "shared" }),
                        text,
                    ),
                )
                .await,
        );
    }

    let ranked = wire(
        &provider
            .recall(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "query": "northern gate",
                    "budget": { "max_items": 10 },
                })),
            )
            .await,
    );
    assert_eq!(ranked["total_authorized"], 3);
    assert!(
        ranked["records"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("northern"),
        "the query moves the matching record to the front: {ranked}"
    );

    let stable = wire(
        &provider
            .recall(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "query": "northern gate",
                    "budget": { "max_items": 10 },
                })),
            )
            .await,
    );
    assert_eq!(
        ranked["records"], stable["records"],
        "identical state + request + policy yields identical ordering (§9.2)"
    );

    // One byte below the first item: the page is empty, so it is returned whole
    // and named as the overflow (§9.3).
    let oversized = wire(
        &provider
            .recall(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "budget": { "max_items": 10, "max_bytes": 1 },
                })),
            )
            .await,
    );
    assert_eq!(oversized["records"].as_array().unwrap().len(), 1);
    assert_eq!(
        oversized["overflow_record_id"],
        oversized["records"][0]["record_id"]
    );
    assert_eq!(oversized["truncated"], true);

    let capped = wire(&provider.recall(&admin, recall_request(2)).await);
    assert_eq!(capped["records"].as_array().unwrap().len(), 2);
    assert_eq!(capped["truncated"], true);
    let cursor = capped["next_cursor"]
        .as_str()
        .expect("a third record remains");
    let page_two = wire(
        &provider
            .recall(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "budget": { "max_items": 2 },
                    "cursor": cursor,
                })),
            )
            .await,
    );
    assert_eq!(page_two["records"].as_array().unwrap().len(), 1);
    assert_eq!(page_two["truncated"], false);
    assert_eq!(page_two.get("next_cursor"), None);
}

/// §7.23/§7.24 + MEM-R005/D-12: profile reads are honest-empty, writes are
/// CAS-checked with `null` meaning create-only, and the admission seam reports
/// a missing *required* authored kind without inventing a document.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn profile_admission_and_honest_empty_reads() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(OTHER_SUBJECT))
            .await,
    );

    let empty = wire(
        &provider
            .get_profile(
                &admin,
                dto(json!({ "space_id": SPACE, "subject_id": SUBJECT })),
            )
            .await,
    );
    assert_eq!(
        empty["profiles"],
        json!([]),
        "an absent profile is honest-empty, never fabricated (§7.23)"
    );

    let before = ok(provider.profile_admission(&admin, SPACE, SUBJECT).await);
    assert!(!before.is_admitted(), "no authored profile yet");
    assert_eq!(before.missing_required, vec!["personality".to_string()]);
    assert!(before.present.is_empty());

    let created = wire(
        &provider
            .update_profile(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "profile_kind": "personality",
                    "expected_revision": null,
                    "document": { "format": "markdown", "body": "# Mira\n\nPatient, watchful." },
                    "idempotency_key": "k-profile-create",
                })),
            )
            .await,
    );
    assert_eq!(receipt(&created)["result"]["revision"], 1);

    let create_only = wire(
        &provider
            .update_profile(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "profile_kind": "personality",
                    "expected_revision": null,
                    "document": { "format": "markdown", "body": "replaced" },
                    "idempotency_key": "k-profile-create-again",
                })),
            )
            .await,
    );
    assert_eq!(
        error_code(&create_only),
        "REVISION_CONFLICT",
        "null expected_revision is create-only (§7.24)"
    );

    let updated = wire(
        &provider
            .update_profile(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "profile_kind": "personality",
                    "expected_revision": 1,
                    "document": { "format": "markdown", "body": "# Mira\n\nPatient, watchful, kind." },
                    "idempotency_key": "k-profile-update",
                })),
            )
            .await,
    );
    assert_eq!(receipt(&updated)["result"]["revision"], 2);

    let read = wire(
        &provider
            .get_profile(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "profile_kinds": ["personality", "preferences"],
                })),
            )
            .await,
    );
    let profiles = read["profiles"].as_array().unwrap();
    assert_eq!(
        profiles.len(),
        1,
        "the absent optional kind is honest-empty, not fabricated: {read}"
    );
    assert_eq!(profiles[0]["profile_kind"], "personality");
    assert_eq!(profiles[0]["updated_by"], CREATOR);
    assert_eq!(profiles[0]["revision"], 2);

    let admitted = ok(provider.profile_admission(&admin, SPACE, SUBJECT).await);
    assert!(admitted.is_admitted());
    assert_eq!(admitted.present, vec!["personality".to_string()]);
    assert!(admitted.missing_required.is_empty());

    // The other subject still lacks the required kind, and nothing was copied.
    let missing = ok(provider
        .profile_admission(&admin, SPACE, OTHER_SUBJECT)
        .await);
    assert!(!missing.is_admitted());
    assert_eq!(missing.missing_required, vec!["personality".to_string()]);
}

/// D-7: the nine-value Nexus taxonomy survives the protocol's open kind
/// vocabulary losslessly — both directions, and nothing else is claimed.
#[tokio::test]
async fn nexus_kinds_round_trip_through_the_namespaced_mapping() {
    use nexus_core::memory_provider::policy;

    let mut seen = Vec::new();
    for kind in policy::NEXUS_MEMORY_KINDS {
        let record_kind = policy::namespaced_record_kind(kind);
        assert_eq!(record_kind, format!("nexus.{kind}"));
        assert_eq!(
            policy::memory_kind_of(&record_kind),
            Some(kind),
            "the mapping is a bijection (D-7)"
        );
        seen.push(record_kind);
    }
    assert_eq!(seen.len(), 9, "nine authored kinds, never collapsed");
    assert_eq!(policy::NEXUS_MEMORY_KINDS.len(), 9);

    assert_eq!(policy::memory_kind_of("event"), None);
    assert_eq!(policy::memory_kind_of("nexus.unknown_kind"), None);
}

// ── Round B conformance regressions (C1–C5) ────────────────────────────

/// §7.22 (C1): execution-time recall demands the `recall` capability itself. A
/// principal holding only `records_read` may list the very same records and is
/// still refused at recall.
#[tokio::test]
async fn recall_demands_the_recall_capability() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let reader = principal(OTHER);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    wire(
        &provider
            .grant(
                &admin,
                subject_grant("k-reader", "records_read", OTHER, SUBJECT),
            )
            .await,
    );
    let created = wire(
        &provider
            .create_record(
                &admin,
                record_create_request(
                    "k-rec",
                    "nexus.world_building",
                    &json!({ "visibility": "shared" }),
                    TEXT,
                ),
            )
            .await,
    );
    let record_id = receipt(&created)["result"]["record_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let listed = wire(
        &provider
            .list_records(
                &reader,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                })),
            )
            .await,
    );
    assert_eq!(
        listed["total_authorized"], 1,
        "records_read authorizes the listing: {listed}"
    );

    let refused = wire(&provider.recall(&reader, recall_request(10)).await);
    assert_eq!(
        error_code(&refused),
        "NOT_AUTHORIZED",
        "records_read is not recall (§7.22): {refused}"
    );

    wire(
        &provider
            .grant(
                &admin,
                subject_grant("k-recalled", "recall", OTHER, SUBJECT),
            )
            .await,
    );
    let recalled = wire(&provider.recall(&reader, recall_request(10)).await);
    assert_eq!(recalled["total_authorized"], 1);
    assert_eq!(recalled["records"][0]["record_id"], record_id);
}

/// §7.7/MEM-R024 (C2): a fenced context refuses the capture and authored-create
/// placements into it, names the context on the wire, leaves sibling contexts
/// and shared placement usable, and unfencing restores the write.
#[tokio::test]
async fn context_fences_refuse_capture_and_create() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let fenced = wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_context(SUBJECT, "bnd_1", "fence", 1, "k-fence-context"),
            )
            .await,
    );
    assert_eq!(
        receipt(&fenced)["result"]["fenced_contexts"],
        json!(["bnd_1"])
    );
    assert_eq!(receipt(&fenced)["result"]["status"], "active");

    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-cap-fenced", Some("bnd_1"), TEXT))
            .await,
    );
    assert_eq!(error_code(&captured), "SUBJECT_FENCED");
    assert_eq!(error_details(&captured)["scope"], "context");
    assert_eq!(error_details(&captured)["context_id"], "bnd_1");
    assert_eq!(retry_class(&captured), "terminal");

    let listed = wire(
        &provider
            .list_candidates(&admin, candidates_list_request(None))
            .await,
    );
    assert!(
        listed["candidates"].as_array().unwrap().is_empty(),
        "a refused capture leaves no candidate: {listed}"
    );

    let created = wire(
        &provider
            .create_record(
                &admin,
                record_create_request(
                    "k-create-fenced",
                    "nexus.world_building",
                    &json!({ "visibility": "context", "context_id": "bnd_1" }),
                    TEXT,
                ),
            )
            .await,
    );
    assert_eq!(error_code(&created), "SUBJECT_FENCED");
    assert_eq!(error_details(&created)["scope"], "context");

    // MEM-R024: no collateral fencing — the sibling context and shared
    // placement stay writable.
    let sibling = wire(
        &provider
            .create_record(
                &admin,
                record_create_request(
                    "k-create-sibling",
                    "nexus.world_building",
                    &json!({ "visibility": "context", "context_id": "bnd_2" }),
                    TEXT,
                ),
            )
            .await,
    );
    assert_eq!(receipt(&sibling)["result"]["revision"], 1);
    let shared = wire(
        &provider
            .capture(&admin, capture_request("k-cap-shared", None, TEXT))
            .await,
    );
    assert_eq!(receipt(&shared)["result"]["classified"], 1);

    wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_context(SUBJECT, "bnd_1", "unfence", 2, "k-unfence-context"),
            )
            .await,
    );
    let restored = wire(
        &provider
            .capture(
                &admin,
                capture_request("k-cap-restored", Some("bnd_1"), TEXT),
            )
            .await,
    );
    assert_eq!(
        receipt(&restored)["result"]["classified"],
        1,
        "unfencing the context restores the write"
    );
}

/// §7.7/MEM-R024 (C2): row-scoped content writes resolve their placement from
/// the row, so review, update and promotion are refused when that placement —
/// or, for promotion, either side of the move — is fenced.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn context_fences_refuse_review_update_and_promotion() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-cap-review", Some("bnd_1"), TEXT))
            .await,
    );
    let candidate_id = receipt(&captured)["result"]["candidate_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let mut records = Vec::new();
    for (key, context_id) in [("k-rec-1", "bnd_1"), ("k-rec-2", "bnd_2")] {
        let created = wire(
            &provider
                .create_record(
                    &admin,
                    record_create_request(
                        key,
                        "nexus.world_building",
                        &json!({ "visibility": "context", "context_id": context_id }),
                        TEXT,
                    ),
                )
                .await,
        );
        records.push(
            receipt(&created)["result"]["record_id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }

    wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_context(SUBJECT, "bnd_1", "fence", 1, "k-fence-context"),
            )
            .await,
    );

    let reviewed = wire(
        &provider
            .review_candidate(&admin, review_request("k-review-fenced", &candidate_id, 1))
            .await,
    );
    assert_eq!(
        error_code(&reviewed),
        "SUBJECT_FENCED",
        "review creates a record in the candidate's fenced capture scope: {reviewed}"
    );
    assert_eq!(error_details(&reviewed)["scope"], "context");

    let updated = wire(
        &provider
            .update_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": records[0],
                    "expected_revision": 1,
                    "content": { "text": "rewritten inside a fenced context" },
                    "idempotency_key": "k-update-fenced",
                })),
            )
            .await,
    );
    assert_eq!(error_code(&updated), "SUBJECT_FENCED");

    // A placement move writes both sides: out of the fenced context…
    let moved_out = wire(
        &provider
            .promote_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": records[0],
                    "expected_revision": 1,
                    "target_scope": { "visibility": "shared" },
                    "idempotency_key": "k-promote-out",
                })),
            )
            .await,
    );
    assert_eq!(error_code(&moved_out), "SUBJECT_FENCED");

    // …and into it.
    let moved_in = wire(
        &provider
            .promote_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": records[1],
                    "expected_revision": 1,
                    "target_scope": { "visibility": "context", "context_id": "bnd_1" },
                    "idempotency_key": "k-promote-in",
                })),
            )
            .await,
    );
    assert_eq!(error_code(&moved_in), "SUBJECT_FENCED");
    assert_eq!(error_details(&moved_in)["context_id"], "bnd_1");

    // The sibling context keeps moving freely.
    let sibling_move = wire(
        &provider
            .promote_record(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "record_id": records[1],
                    "expected_revision": 1,
                    "target_scope": { "visibility": "shared" },
                    "idempotency_key": "k-promote-sibling",
                })),
            )
            .await,
    );
    assert_eq!(receipt(&sibling_move)["result"]["revision"], 2);
}

/// §7.7 (C3): an archived subject leaves recall and the default candidate list,
/// while its records, candidates and receipts stay readable through the audit
/// routes the protocol still permits.
#[tokio::test]
async fn archived_subjects_leave_recall_and_default_candidate_lists() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let created = wire(
        &provider
            .create_record(
                &admin,
                record_create_request(
                    "k-rec",
                    "nexus.world_building",
                    &json!({ "visibility": "shared" }),
                    TEXT,
                ),
            )
            .await,
    );
    let record_id = receipt(&created)["result"]["record_id"]
        .as_str()
        .unwrap()
        .to_owned();
    wire(
        &provider
            .capture(&admin, capture_request("k-cap", None, TEXT))
            .await,
    );

    let recalled = wire(&provider.recall(&admin, recall_request(10)).await);
    assert_eq!(recalled["total_authorized"], 1);
    let listed = wire(
        &provider
            .list_candidates(&admin, candidates_list_request(None))
            .await,
    );
    assert_eq!(listed["candidates"].as_array().unwrap().len(), 1);

    let archived = wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "archive", 1, "k-archive"))
            .await,
    );
    assert_eq!(receipt(&archived)["result"]["status"], "archived");

    let recalled_after = wire(&provider.recall(&admin, recall_request(10)).await);
    assert_eq!(
        recalled_after["total_authorized"], 0,
        "an archived subject's records leave recall (§7.7): {recalled_after}"
    );
    assert!(recalled_after["records"].as_array().unwrap().is_empty());

    let default_listing = wire(
        &provider
            .list_candidates(&admin, candidates_list_request(None))
            .await,
    );
    assert!(
        default_listing["candidates"].as_array().unwrap().is_empty(),
        "archive hides the subject from the default candidate listing (§7.7): {default_listing}"
    );

    // §7.7 keeps the retained state readable: an explicit status filter is an
    // audit read, and by-id/listing reads are unchanged.
    let audit_listing = wire(
        &provider
            .list_candidates(&admin, candidates_list_request(Some("pending")))
            .await,
    );
    assert_eq!(audit_listing["candidates"].as_array().unwrap().len(), 1);

    let by_id = wire(
        &provider
            .get_record(&admin, record_get_request(&record_id))
            .await,
    );
    assert_eq!(by_id["record"]["record_id"], record_id);
    assert_eq!(by_id["record"]["content"]["text"], TEXT);

    let listing = wire(
        &provider
            .list_records(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                })),
            )
            .await,
    );
    assert_eq!(listing["total_authorized"], 1);
}

/// §7.0.7/MEM-R026 (C4): the recall cursor binds the snapshot *and* the ranking
/// input. Losing a redundant grant, swapping the visible rows for different
/// rows at the same revision sum, and changing the query all make it
/// `CURSOR_INVALID` instead of silently re-paging a different set.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn recall_cursors_bind_snapshot_and_query() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let reader = principal(OTHER);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let mut record_ids = Vec::new();
    for (key, text) in [
        (
            "k-rec-1",
            "The northern gate opens at dawn for the caravan.",
        ),
        ("k-rec-2", "The southern harbour closes on the first storm."),
    ] {
        let created = wire(
            &provider
                .create_record(
                    &admin,
                    record_create_request(
                        key,
                        "nexus.world_building",
                        &json!({ "visibility": "shared" }),
                        text,
                    ),
                )
                .await,
        );
        record_ids.push(
            receipt(&created)["result"]["record_id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    wire(
        &provider
            .grant(
                &admin,
                subject_grant("k-recall-1", "recall", OTHER, SUBJECT),
            )
            .await,
    );
    let duplicate = wire(
        &provider
            .grant(
                &admin,
                subject_grant("k-recall-2", "recall", OTHER, SUBJECT),
            )
            .await,
    );
    let duplicate_grant = receipt(&duplicate)["result"]["grant_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // (a) A grant change that leaves every visible row untouched still changes
    //     the authorization the page was cut under.
    let page_one = wire(&provider.recall(&reader, recall_page(1, None, None)).await);
    assert_eq!(page_one["records"].as_array().unwrap().len(), 1);
    let cursor = page_one["next_cursor"]
        .as_str()
        .expect("a second record remains")
        .to_owned();
    let page_two = wire(
        &provider
            .recall(&reader, recall_page(1, None, Some(&cursor)))
            .await,
    );
    assert_eq!(
        page_two["records"].as_array().unwrap().len(),
        1,
        "the cursor continues its own snapshot"
    );
    wire(
        &provider
            .revoke_grant(
                &admin,
                dto(json!({
                    "space_id": SPACE,
                    "grant_id": duplicate_grant,
                    "expected_revision": 1,
                    "idempotency_key": "k-revoke-duplicate",
                })),
            )
            .await,
    );
    let redundant_change = wire(
        &provider
            .recall(&reader, recall_page(1, None, Some(&cursor)))
            .await,
    );
    assert_eq!(
        error_code(&redundant_change),
        "CURSOR_INVALID",
        "a redundant grant revoke invalidates the cursor even though the rows are unchanged"
    );

    // (b) Two revision-1 rows out, two revision-1 rows in: the visible revision
    //     sum stays 2 while the page would silently change identity.
    let page_one = wire(&provider.recall(&reader, recall_page(1, None, None)).await);
    let stale = page_one["next_cursor"]
        .as_str()
        .expect("a second record remains")
        .to_owned();
    for (index, record_id) in record_ids.iter().enumerate() {
        wire(
            &provider
                .delete_record(
                    &admin,
                    dto(json!({
                        "space_id": SPACE,
                        "record_id": record_id,
                        "expected_revision": 1,
                        "idempotency_key": format!("k-delete-{index}"),
                    })),
                )
                .await,
        );
    }
    for (key, text) in [
        ("k-rec-3", "The eastern road floods after the thaw."),
        ("k-rec-4", "The western mill grinds until the frost."),
    ] {
        wire(
            &provider
                .create_record(
                    &admin,
                    record_create_request(
                        key,
                        "nexus.world_building",
                        &json!({ "visibility": "shared" }),
                        text,
                    ),
                )
                .await,
        );
    }
    let swapped = wire(
        &provider
            .recall(&reader, recall_page(1, None, Some(&stale)))
            .await,
    );
    assert_eq!(
        error_code(&swapped),
        "CURSOR_INVALID",
        "equal visible revision sums do not identify the same page"
    );

    // (c) The query orders the page, so it is part of the bound request.
    let ranked = wire(
        &provider
            .recall(&reader, recall_page(1, Some("eastern road"), None))
            .await,
    );
    let ranked_cursor = ranked["next_cursor"]
        .as_str()
        .expect("a second record remains")
        .to_owned();
    let same_query = wire(
        &provider
            .recall(
                &reader,
                recall_page(1, Some("eastern road"), Some(&ranked_cursor)),
            )
            .await,
    );
    assert_eq!(same_query["records"].as_array().unwrap().len(), 1);
    let changed_query = wire(
        &provider
            .recall(
                &reader,
                recall_page(1, Some("western mill"), Some(&ranked_cursor)),
            )
            .await,
    );
    assert_eq!(
        error_code(&changed_query),
        "CURSOR_INVALID",
        "a cursor never carries over to another ordering (§9.2/MEM-R026)"
    );
}

/// §7.19/§7.7 (C5): deletion runs the same subject and placement gates as update
/// and promote, so a fenced or archived subject cannot have its content
/// irreversibly scrubbed.
#[tokio::test]
async fn delete_is_gated_by_the_subject_lifecycle() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let created = wire(
        &provider
            .create_record(
                &admin,
                record_create_request(
                    "k-rec",
                    "nexus.world_building",
                    &json!({ "visibility": "context", "context_id": "bnd_1" }),
                    TEXT,
                ),
            )
            .await,
    );
    let record_id = receipt(&created)["result"]["record_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let delete = |key: &str| {
        dto::<schemas::MemoryRecordsDeleteRequest>(json!({
            "space_id": SPACE,
            "record_id": record_id,
            "expected_revision": 1,
            "idempotency_key": key,
        }))
    };

    wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_context(SUBJECT, "bnd_1", "fence", 1, "k-fence-context"),
            )
            .await,
    );
    let context_fenced = wire(
        &provider
            .delete_record(&admin, delete("k-delete-context"))
            .await,
    );
    assert_eq!(error_code(&context_fenced), "SUBJECT_FENCED");
    assert_eq!(error_details(&context_fenced)["scope"], "context");

    wire(
        &provider
            .fence_subject(
                &admin,
                fence_at_context(SUBJECT, "bnd_1", "unfence", 2, "k-unfence-context"),
            )
            .await,
    );
    wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "fence", 3, "k-fence"))
            .await,
    );
    let subject_fenced = wire(
        &provider
            .delete_record(&admin, delete("k-delete-fenced"))
            .await,
    );
    assert_eq!(error_code(&subject_fenced), "SUBJECT_FENCED");
    assert_eq!(error_details(&subject_fenced)["status"], "fenced");

    wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "unfence", 4, "k-unfence"))
            .await,
    );
    wire(
        &provider
            .fence_subject(&admin, fence_at_subject(SUBJECT, "archive", 5, "k-archive"))
            .await,
    );
    let archived = wire(
        &provider
            .delete_record(&admin, delete("k-delete-archived"))
            .await,
    );
    assert_eq!(error_code(&archived), "SUBJECT_FENCED");
    assert_eq!(error_details(&archived)["status"], "archived");

    // Every refusal left the content untouched: no scrub, no tombstone.
    let record = wire(
        &provider
            .get_record(&admin, record_get_request(&record_id))
            .await,
    );
    assert_eq!(record["record"]["content"]["text"], TEXT);
    assert_eq!(record["record"]["revision"], 1);
    assert!(
        record["record"].get("tombstone").is_none(),
        "a refused delete never tombstones: {record}"
    );
}

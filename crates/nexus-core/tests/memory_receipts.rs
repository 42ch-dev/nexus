//! NXM-2 memory-domain receipt tests: §6.10, §7.0.5, §7.28; MEM-R008/R010/R021.
//!
//! The receipt channel is the memory domain's sole uncertainty-resolution path
//! (MEM-R010): a same-key retry replays the stored receipt instead of re-running
//! the effect, `memory.receipts.get` serves that receipt content-free under
//! **current** authorization, and a receipt whose body has been evicted answers
//! `RECEIPT_EXPIRED` (terminal, `details.last_known_status`) rather than silently
//! re-executing. Past `dedup_retention_seconds` the namespace is new again.
//!
//! The compute consumer of the durable operation-receipt store is pinned by its
//! own existing tests (`crates/nexus-core/tests/capability_compute.rs`); this
//! target drives the memory consumer only.
//!
//! Every scenario runs on a temp nexus home.

use std::path::PathBuf;

use nexus_core::memory_provider::LocalMemoryProvider;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{MemoryPort, SpokeResult};
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SPACE: &str = "spc_local_default";
const SUBJECT: &str = "sbj_mira";
const TEXT: &str = "Mira promised to return before the northern gate opens at dawn.";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
}

fn seed_env() -> Env {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
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

fn principal(id: &str) -> schemas::Principal {
    dto(json!({ "principal_id": id, "extensions": {} }))
}

fn dto<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("wire-valid payload")
}

/// One assertion shape for both port channels: the success payload, or the
/// refusal as the wire carries it (§14.1).
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

/// The receipt payload of `memory.receipts.get`.
fn read_receipt(body: &Value) -> &Value {
    let receipt = &body["receipt"];
    assert!(
        receipt.is_object(),
        "memory.receipts.get answers {{receipt: …}}, got {body}"
    );
    receipt
}

fn ensure_request(subject: &str) -> schemas::MemorySubjectsEnsureRequest {
    dto(json!({ "space_id": SPACE, "subject_id": subject }))
}

fn capture_request(key: &str, text: &str) -> schemas::MemoryCaptureRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "content": { "text": text },
        "capture_scope": { "visibility": "shared" },
        "source_context_ids": [],
        "idempotency_key": key,
    }))
}

fn candidates_list_request() -> schemas::MemoryCandidatesListRequest {
    dto(json!({ "space_id": SPACE, "subject_id": SUBJECT }))
}

fn receipts_get_request(receipt_id: &str) -> schemas::MemoryReceiptsGetRequest {
    dto(json!({ "space_id": SPACE, "receipt_id": receipt_id }))
}

fn grant_request(key: &str, capability: &str, grantee: &str) -> schemas::MemoryGrantsGrantRequest {
    dto(json!({
        "space_id": SPACE,
        "principal_id": grantee,
        "capabilities": [capability],
        "idempotency_key": key,
    }))
}

fn revoke_request(key: &str, grant_id: &str, revision: u64) -> schemas::MemoryGrantsRevokeRequest {
    dto(json!({
        "space_id": SPACE,
        "grant_id": grant_id,
        "expected_revision": revision,
        "idempotency_key": key,
    }))
}

/// §7.1 `limits` as the provider advertises them: the retention windows the
/// receipt channel must actually enforce.
async fn retention_limits(
    provider: &LocalMemoryProvider,
    admin: &schemas::Principal,
) -> (i64, i64) {
    let described = wire(&provider.describe(admin, dto(json!({}))).await);
    let limits = &described["limits"];
    let read = |field: &str| {
        i64::try_from(
            limits[field]
                .as_u64()
                .unwrap_or_else(|| panic!("missing limits.{field}: {described}")),
        )
        .unwrap()
    };
    (
        read("receipt_retention_seconds"),
        read("dedup_retention_seconds"),
    )
}

async fn pending_candidates(provider: &LocalMemoryProvider, admin: &schemas::Principal) -> usize {
    wire(
        &provider
            .list_candidates(admin, candidates_list_request())
            .await,
    )["candidates"]
        .as_array()
        .expect("candidates page")
        .len()
}

/// §7.0.5/MEM-R008: a same-key same-payload capture replays the stored receipt
/// verbatim — no `IDEMPOTENCY_CONFLICT`, no second candidate.
#[tokio::test]
async fn duplicate_capture_replays_the_identical_receipt() {
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
            .capture(&admin, capture_request("k-receipt-replay", TEXT))
            .await,
    );
    let first = receipt(&first).clone();

    let replay = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-replay", TEXT))
            .await,
    );
    assert_eq!(
        receipt(&replay),
        &first,
        "the retry returns the identical stored receipt"
    );
    assert_eq!(
        pending_candidates(&provider, &admin).await,
        1,
        "a replay never inserts a second candidate"
    );
}

/// §14.3/MEM-R010: a commit whose response was lost is `OUTCOME_UNCERTAIN`, and
/// the same-key replay plus `memory.receipts.get` resolve it — the effect exists
/// exactly once, and the receipt is content-free (MEM-R022).
#[tokio::test]
async fn uncertain_outcome_resolves_through_the_receipt_channel() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );

    // The COMMIT succeeded; the response never reached the caller.
    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-uncertain", TEXT))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class(&lost), "uncertain");

    // Resolution 1 — same-key replay returns the committed receipt (§7.0.5).
    let resolved = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-uncertain", TEXT))
            .await,
    );
    let resolved = receipt(&resolved).clone();
    assert_eq!(resolved["op"], "memory.capture");
    let receipt_id = resolved["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    assert_eq!(
        pending_candidates(&provider, &admin).await,
        1,
        "the uncertain effect committed exactly once"
    );

    // Resolution 2 — the receipt channel serves the same receipt (§7.28).
    let read = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        read_receipt(&read),
        &resolved,
        "memory.receipts.get serves the stored receipt"
    );
    assert!(
        !read_receipt(&read)["result"].to_string().contains(TEXT),
        "receipts are content-free (MEM-R022): {read}"
    );

    // MEM-R011: the read channel mutates nothing — the same read answers the same
    // bytes and the candidate set is untouched.
    let again = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(read, again);
    assert_eq!(pending_candidates(&provider, &admin).await, 1);

    // §7.28/MEM-R021: disclosure needs *current* authorization.
    let stranger = wire(
        &provider
            .get_receipt(&principal(OTHER), receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(error_code(&stranger), "NOT_AUTHORIZED");
    assert_eq!(retry_class(&stranger), "terminal");

    let unknown = wire(
        &provider
            .get_receipt(
                &admin,
                receipts_get_request("rcp_00000000000000000000000000000000"),
            )
            .await,
    );
    assert_eq!(error_code(&unknown), "NOT_FOUND");
}

/// §7.28/MEM-R018 + C-104: a requester whose only grant was revoked cannot read
/// even their own receipt; a space admin still can.
#[tokio::test]
async fn revoked_requester_cannot_read_their_receipt() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let other = principal(OTHER);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );

    let granted = wire(
        &provider
            .grant(&admin, grant_request("k-receipt-grant", "capture", OTHER))
            .await,
    );
    let granted = receipt(&granted);
    let grant_id = granted["result"]["grant_id"]
        .as_str()
        .expect("grant id")
        .to_owned();

    let captured = wire(
        &provider
            .capture(&other, capture_request("k-receipt-revoked", TEXT))
            .await,
    );
    let captured = receipt(&captured).clone();
    assert_eq!(captured["requester_principal_id"], OTHER);
    let receipt_id = captured["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();

    // Before revocation the requester reads their own receipt.
    let before = wire(
        &provider
            .get_receipt(&other, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(read_receipt(&before), &captured);

    wire(
        &provider
            .revoke_grant(&admin, revoke_request("k-receipt-revoke", &grant_id, 1))
            .await,
    );

    let after = wire(
        &provider
            .get_receipt(&other, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        error_code(&after),
        "NOT_AUTHORIZED",
        "a revoked principal cannot read receipts (MEM-R018)"
    );
    assert_eq!(retry_class(&after), "terminal");

    // The space admin's authorization is unaffected by the revocation.
    let admin_read = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(read_receipt(&admin_read), &captured);
}

/// §6.10/MEM-R021 + C-028: inside `dedup_retention_seconds` an evicted body is
/// terminal `RECEIPT_EXPIRED` (never a silent re-run); past it the key is new.
#[tokio::test]
async fn replay_after_receipt_eviction_is_receipt_expired_until_the_dedup_window_closes() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, dedup_retention) = retention_limits(&provider, &admin).await;
    assert!(
        dedup_retention >= receipt_retention,
        "the dedup window covers the receipt window (MEM-R021)"
    );
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );

    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-evicted", TEXT))
            .await,
    );
    let captured = receipt(&captured).clone();
    let receipt_id = captured["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();

    // One second past the receipt window: the body is evicted, the tombstone lives.
    provider.advance_receipt_clock(receipt_retention + 1);
    let replay = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-evicted", TEXT))
            .await,
    );
    assert_eq!(error_code(&replay), "RECEIPT_EXPIRED");
    assert_eq!(retry_class(&replay), "terminal");
    assert_eq!(error_details(&replay)["last_known_status"], "committed");
    assert!(
        error_details(&replay)["last_known_status"].is_string(),
        "the tombstone reports the last known status: {replay}"
    );

    let read = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(error_code(&read), "RECEIPT_EXPIRED");
    assert_eq!(error_details(&read)["last_known_status"], "committed");

    // Past the dedup window the namespace is new: the same key runs a fresh effect.
    provider.advance_receipt_clock(dedup_retention - receipt_retention + 1);
    let fresh = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-evicted", TEXT))
            .await,
    );
    let fresh = receipt(&fresh).clone();
    assert_ne!(
        fresh["receipt_id"].as_str().expect("receipt id"),
        receipt_id.as_str(),
        "the expired namespace is re-executed under a new receipt"
    );
    assert_eq!(fresh["op"], "memory.capture");

    // The expired receipt is gone with its tombstone.
    let gone = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(error_code(&gone), "NOT_FOUND");
}

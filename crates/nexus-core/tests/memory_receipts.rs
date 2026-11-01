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
use std::sync::Arc;

use nexus_core::memory_provider::LocalMemoryProvider;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{MemoryPort, SpokeResult};
use sqlx::SqlitePool;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const STRANGER: &str = "ctr_cccccccccccccccccccccccccccccccc";
const UNKNOWN_RECEIPT_ID: &str = "rcp_00000000000000000000000000000000";
const SPACE: &str = "spc_local_default";
const SUBJECT: &str = "sbj_mira";
const TEXT: &str = "Mira promised to return before the northern gate opens at dawn.";

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

/// A grant that stops authorizing at `expires_at` (RFC 3339, whole seconds).
fn grant_request_until(
    key: &str,
    capability: &str,
    grantee: &str,
    expires_at: &str,
) -> schemas::MemoryGrantsGrantRequest {
    dto(json!({
        "space_id": SPACE,
        "principal_id": grantee,
        "capabilities": [capability],
        "expires_at": expires_at,
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

/// One stored receipt row as retention left it — `(result_json, error_json,
/// status)` straight from `memory_receipts`. A settled row with no payload is the
/// content-free §6.10 tombstone.
async fn receipt_payloads(
    pool: &SqlitePool,
    receipt_id: &str,
) -> (Option<String>, Option<String>, String) {
    sqlx::query_as(
        "SELECT result_json, error_json, status FROM memory_receipts WHERE receipt_id = ?",
    )
    .bind(receipt_id)
    .fetch_one(pool)
    .await
    .expect("memory_receipts row")
}

/// How many rows the namespace still holds for that id: `1` while only the read
/// path has looked at it, `0` once a sweep purged an expired tombstone.
async fn receipt_rows(pool: &SqlitePool, receipt_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM memory_receipts WHERE receipt_id = ?")
        .bind(receipt_id)
        .fetch_one(pool)
        .await
        .expect("memory_receipts row count")
}

/// The durable §6.10 retention clock floor, as the provider persisted it.
async fn persisted_clock_floor(pool: &SqlitePool) -> i64 {
    let stored: String = sqlx::query_scalar(
        "SELECT value FROM memory_provider_meta WHERE key = 'clock_floor_micros'",
    )
    .fetch_one(pool)
    .await
    .expect("memory_provider_meta clock floor");
    stored.parse().expect("decimal clock floor")
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
    provider.advance_clock(receipt_retention + 1);
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
    provider.advance_clock(dedup_retention - receipt_retention + 1);
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

/// §7.0.5: the dedup namespace binds the key to its `payload_digest`, so once the
/// body is evicted the same payload is still a terminal replay of the recorded
/// outcome while a different payload is a conflict — never "expired", and never
/// the prior operation's status reported for a different request.
#[tokio::test]
async fn evicted_receipt_separates_a_replay_from_a_different_payload() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, _) = retention_limits(&provider, &admin).await;
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );

    let original = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-digest", TEXT))
            .await,
    );
    assert_eq!(receipt(&original)["op"], "memory.capture");

    provider.advance_clock(receipt_retention + 1);

    let replay = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-digest", TEXT))
            .await,
    );
    assert_eq!(
        error_code(&replay),
        "RECEIPT_EXPIRED",
        "the same payload is a true replay of the recorded outcome: {replay}"
    );
    assert_eq!(error_details(&replay)["last_known_status"], "committed");

    let conflict = wire(
        &provider
            .capture(
                &admin,
                capture_request("k-receipt-digest", "A different promise entirely."),
            )
            .await,
    );
    assert_eq!(
        error_code(&conflict),
        "IDEMPOTENCY_CONFLICT",
        "a different payload under the same key is a conflict, not an expired replay: {conflict}"
    );
    assert_ne!(
        conflict["error"]["code"].as_str().unwrap_or_default(),
        "RECEIPT_EXPIRED"
    );
    assert_eq!(
        pending_candidates(&provider, &admin).await,
        1,
        "neither request re-executed the capture"
    );
}

/// §6.10/MEM-R021: retention is physical. A mutating transaction evicts every
/// expired body — keyless `memory.subjects.ensure` receipts included — and later
/// drops the tombstone rows, without waiting for a probe of the same key. An
/// evicted body is never restored, even if the provider clock moves backwards.
#[tokio::test]
async fn receipt_retention_evicts_bodies_and_purges_tombstones() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, dedup_retention) = retention_limits(&provider, &admin).await;

    let ensured = wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let keyless_id = receipt(&ensured)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-storage", TEXT))
            .await,
    );
    let receipt_id = receipt(&captured)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();

    let pool = nexus_local_db::open_pool_read_only(&env.db_path)
        .await
        .unwrap();

    // Inside the receipt window both bodies are stored as committed receipts.
    for id in [&keyless_id, &receipt_id] {
        let (result, error, status) = receipt_payloads(&pool, id).await;
        assert!(result.is_some(), "a retained receipt serves its body: {id}");
        assert!(error.is_none());
        assert_eq!(status, "committed");
    }

    // One second past the receipt window, an *unrelated* mutation reclaims both
    // bodies: eviction does not need a probe of the same key, and a keyless
    // ensure receipt is reclaimed like any other. The receipt that mutation
    // writes itself stays intact — the sweep only touches expired rows.
    provider.advance_clock(receipt_retention + 1);
    let unrelated = wire(
        &provider
            .ensure_subject(&admin, ensure_request("sbj_other"))
            .await,
    );
    let unrelated_id = receipt(&unrelated)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    for id in [&keyless_id, &receipt_id] {
        let (result, error, status) = receipt_payloads(&pool, id).await;
        assert!(
            result.is_none() && error.is_none(),
            "the body is removed from the row, not merely hidden: {id}"
        );
        assert_eq!(
            status, "committed",
            "the tombstone keeps the terminal status"
        );
    }
    let (unrelated_body, _, _) = receipt_payloads(&pool, &unrelated_id).await;
    assert!(
        unrelated_body.is_some(),
        "a receipt inside its window keeps its body"
    );

    // The namespace still binds the key, and the evicted body stays evicted when
    // the clock is moved back to the receipt's own creation time. (The provider
    // clock is monotone, so this move cannot lower the verdict; the read-only
    // path that has no swept tombstone to rely on is
    // `receipt_read_expiry_is_one_way_under_a_backward_clock`.)
    provider.advance_clock(-(receipt_retention + 1));
    let read = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        error_code(&read),
        "RECEIPT_EXPIRED",
        "a scrubbed body is never served again, whatever the clock says: {read}"
    );
    assert_eq!(error_details(&read)["last_known_status"], "committed");

    // Past the dedup window the tombstone rows themselves are dropped.
    provider.advance_clock(dedup_retention + 1);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request("sbj_fresh"))
            .await,
    );
    for id in [&keyless_id, &receipt_id] {
        let rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memory_receipts WHERE receipt_id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rows, 0, "the expired tombstone is purged: {id}");
    }

    let fresh = wire(
        &provider
            .capture(&admin, capture_request("k-receipt-storage", TEXT))
            .await,
    );
    assert_ne!(
        receipt(&fresh)["receipt_id"].as_str().expect("receipt id"),
        receipt_id,
        "the closed dedup window frees the key for a new operation"
    );
}

/// §7.28/MEM-R018: the authorization clock is read after the read holds its
/// connection, so a grant that expires while `memory.receipts.get` waits in the
/// pool queue cannot authorize disclosure.
#[tokio::test]
async fn receipt_authorization_is_evaluated_after_the_connection_wait() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let other = principal(OTHER);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );

    let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(60))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    wire(
        &provider
            .grant(
                &admin,
                grant_request_until("k-receipt-wait-grant", "capture", OTHER, &expires_at),
            )
            .await,
    );
    let captured = wire(
        &provider
            .capture(&other, capture_request("k-receipt-wait", TEXT))
            .await,
    );
    let captured = receipt(&captured).clone();
    let receipt_id = captured["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();

    // While the grant is live the requester reads their own receipt.
    let live = wire(
        &provider
            .get_receipt(&other, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(read_receipt(&live), &captured);

    // Park the read once it holds its connection; the grant expires while it
    // waits. A clock sampled before the wait would still admit the disclosure.
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    provider.park_next_read(Arc::clone(&barrier));
    let read = provider.get_receipt(&other, receipts_get_request(&receipt_id));
    let expire_while_queued = async {
        provider.advance_clock(120);
        barrier.wait().await;
    };
    let (read, ()) = tokio::join!(read, expire_while_queued);
    let refused = wire(&read);
    assert_eq!(
        error_code(&refused),
        "NOT_AUTHORIZED",
        "a grant that expired during the wait cannot authorize disclosure: {refused}"
    );
    assert_eq!(retry_class(&refused), "terminal");

    // The body is still inside its retention window: the refusal is current
    // authorization, not retention.
    let admin_read = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(read_receipt(&admin_read), &captured);
}

/// §7.28/MEM-R018: receipt existence is not enumerable without current space
/// participation — an unrelated or revoked principal gets one refusal shape for
/// an unknown id and for an existing one.
#[tokio::test]
async fn receipt_existence_is_not_enumerable_without_current_participation() {
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
            .grant(
                &admin,
                grant_request("k-receipt-enum-grant", "capture", OTHER),
            )
            .await,
    );
    let grant_id = receipt(&granted)["result"]["grant_id"]
        .as_str()
        .expect("grant id")
        .to_owned();
    let captured = wire(
        &provider
            .capture(&other, capture_request("k-receipt-enum", TEXT))
            .await,
    );
    let existing_id = receipt(&captured)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();

    // A principal with no grant in the space.
    for id in [existing_id.as_str(), UNKNOWN_RECEIPT_ID] {
        let refused = wire(
            &provider
                .get_receipt(&principal(STRANGER), receipts_get_request(id))
                .await,
        );
        assert_eq!(
            error_code(&refused),
            "NOT_AUTHORIZED",
            "an unrelated principal cannot tell whether receipt {id} exists: {refused}"
        );
        assert_eq!(retry_class(&refused), "terminal");
    }

    // The requester after their only grant was revoked.
    wire(
        &provider
            .revoke_grant(
                &admin,
                revoke_request("k-receipt-enum-revoke", &grant_id, 1),
            )
            .await,
    );
    for id in [existing_id.as_str(), UNKNOWN_RECEIPT_ID] {
        let refused = wire(&provider.get_receipt(&other, receipts_get_request(id)).await);
        assert_eq!(
            error_code(&refused),
            "NOT_AUTHORIZED",
            "a revoked requester cannot tell whether receipt {id} exists: {refused}"
        );
        assert_eq!(retry_class(&refused), "terminal");
    }
}

/// §6.10/MEM-R021: reclamation is autonomous. The sweep commits in a transaction
/// of its own *before* the operation runs, so a replay refusal or an
/// authorization failure — both of which drop the effect transaction — cannot
/// roll the eviction back and leave an aged body on disk.
#[tokio::test]
async fn receipt_reclamation_survives_a_refused_operation() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, _) = retention_limits(&provider, &admin).await;

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-reclaim-refused", TEXT))
            .await,
    );
    let receipt_id = receipt(&captured)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    let pool = nexus_local_db::open_pool_read_only(&env.db_path)
        .await
        .unwrap();
    let (result, error, _) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_some() && error.is_none(),
        "the body starts on disk"
    );

    // Age the body, then refuse the operation that reaches the sweep: the
    // same-key replay is terminal (`RECEIPT_EXPIRED`), so the effect transaction
    // that produced it never commits.
    provider.advance_clock(receipt_retention + 1);
    let replay = wire(
        &provider
            .capture(&admin, capture_request("k-reclaim-refused", TEXT))
            .await,
    );
    assert_eq!(error_code(&replay), "RECEIPT_EXPIRED");
    let (result, error, status) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_none() && error.is_none(),
        "a replay refusal still leaves the sweep committed"
    );
    assert_eq!(
        status, "committed",
        "the tombstone keeps the terminal status"
    );

    // The same holds when the refusal is authorization: age a fresh body past
    // its window, then have a principal with no grant in the space attempt a
    // mutation.
    let second = wire(
        &provider
            .capture(&admin, capture_request("k-reclaim-refused-2", TEXT))
            .await,
    );
    let second_id = receipt(&second)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    provider.advance_clock(receipt_retention + 1);
    let refused = wire(
        &provider
            .capture(
                &principal(STRANGER),
                capture_request("k-reclaim-stranger", TEXT),
            )
            .await,
    );
    assert_eq!(error_code(&refused), "NOT_AUTHORIZED");
    let (result, error, _) = receipt_payloads(&pool, &second_id).await;
    assert!(
        result.is_none() && error.is_none(),
        "an authorization failure still leaves the sweep committed"
    );
}

/// §6.10/MEM-R021: the read path stays read-only (MEM-R011) and its verdict is
/// still one-way. A body past its retention window is reported
/// `RECEIPT_EXPIRED` even when no sweep has run yet, moving the provider clock
/// backwards cannot make a later read serve it, and the next mutating entry
/// point turns the observed eviction into a physical one.
#[tokio::test]
async fn receipt_read_expiry_is_one_way_under_a_backward_clock() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, _) = retention_limits(&provider, &admin).await;

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-read-one-way", TEXT))
            .await,
    );
    let receipt_id = receipt(&captured)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    let pool = nexus_local_db::open_pool_read_only(&env.db_path)
        .await
        .unwrap();

    // Age the body with reads only: nothing sweeps, so the body is still stored
    // and the verdict below is the read path's own.
    provider.advance_clock(receipt_retention + 1);
    let (result, _, _) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_some(),
        "no sweep has run yet, so the aged body is still on disk"
    );
    let read = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        error_code(&read),
        "RECEIPT_EXPIRED",
        "an aged body is not served: {read}"
    );
    assert_eq!(error_details(&read)["last_known_status"], "committed");

    // Back to the receipt's creation instant: the expiry already reported stands.
    provider.advance_clock(-(receipt_retention + 1));
    let again = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        error_code(&again),
        "RECEIPT_EXPIRED",
        "a reported expiry is not re-derived from the clock: {again}"
    );
    assert_eq!(error_details(&again)["last_known_status"], "committed");

    // The next mutating entry point makes the eviction physical.
    wire(
        &provider
            .ensure_subject(&admin, ensure_request("sbj_after_expiry"))
            .await,
    );
    let (result, error, _) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_none() && error.is_none(),
        "the observed eviction becomes durable at the next mutation"
    );
}

/// §6.10/MEM-R021: the dedup window, not the persisted tombstone, decides the
/// answer. A tombstone left by an earlier sweep answers `RECEIPT_EXPIRED` only
/// inside `dedup_retention_seconds`; past it the key is new again, so a
/// read-only deployment — no sweep since — gets `NOT_FOUND` (the `RECEIPT_EXPIRED`
/// cause is the evicted body, and there is no surviving dedup namespace left).
#[tokio::test]
async fn receipt_tombstone_past_the_dedup_window_is_not_found() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, dedup_retention) = retention_limits(&provider, &admin).await;

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-dedup-read-only", TEXT))
            .await,
    );
    let receipt_id = receipt(&captured)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    let pool = nexus_local_db::open_pool_read_only(&env.db_path)
        .await
        .unwrap();

    // A mutation just past the receipt window evicts the body and leaves the
    // content-free tombstone behind.
    provider.advance_clock(receipt_retention + 1);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request("sbj_other"))
            .await,
    );
    let (result, error, status) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_none() && error.is_none(),
        "the body is evicted, the tombstone stays"
    );
    assert_eq!(status, "committed");

    // Inside the dedup window the tombstone still answers for the evicted body.
    let read = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(error_code(&read), "RECEIPT_EXPIRED");
    assert_eq!(error_details(&read)["last_known_status"], "committed");

    // Past the dedup window, reads only: the row is still there (no sweep ran)
    // and the key is new again.
    provider.advance_clock(dedup_retention + 1);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_receipts WHERE receipt_id = ?")
        .bind(&receipt_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        rows, 1,
        "no sweep has run since the tombstone was written; the read path must not purge it"
    );
    let gone = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        error_code(&gone),
        "NOT_FOUND",
        "the dedup window, not the tombstone's presence, decides: {gone}"
    );
    // Both sides of the final read: the row it declined to answer for is still
    // there, so the `NOT_FOUND` came from the window, not from a read-path purge.
    assert_eq!(
        receipt_rows(&pool, &receipt_id).await,
        1,
        "the read that answered NOT_FOUND left the tombstone row untouched"
    );
}

/// §6.10/MEM-R021: an expiry the provider **committed** is durable. A body
/// observed past its window is evicted by the next mutating entry point; the
/// eviction and the clock reading that decided it commit together, so a restart
/// into a wall clock that moved backwards neither restores the body nor
/// re-executes the namespace: the reopened provider answers `RECEIPT_EXPIRED` to
/// the read and to a replay alike, and the key stays bound.
///
/// Only the committed-eviction case is claimed here. The read-only expiry that
/// no sweep committed is the open D13 residual, documented by
/// `read_only_expiry_is_served_again_after_a_restart_under_a_backward_clock`.
#[tokio::test]
async fn committed_expiry_eviction_survives_a_restart_under_a_backward_clock() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, _) = retention_limits(&provider, &admin).await;

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-restart-floor", TEXT))
            .await,
    );
    let receipt_id = receipt(&captured)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    let pool = nexus_local_db::open_pool_read_only(&env.db_path)
        .await
        .unwrap();
    let (result, error, _) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_some() && error.is_none(),
        "the body starts on disk"
    );

    // Age the body and observe the expiry on the read path only: reads stay
    // read-only (MEM-R011), so nothing has been recorded yet and the aged body
    // is still physically there.
    provider.advance_clock(receipt_retention + 1);
    let read = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        error_code(&read),
        "RECEIPT_EXPIRED",
        "an aged body is not served: {read}"
    );
    let (result, _, _) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_some(),
        "the read path persisted nothing, so no sweep has evicted the body yet"
    );

    // The next mutating entry point commits the eviction in a transaction of its
    // own, so it outlives the operation and the process.
    wire(
        &provider
            .ensure_subject(&admin, ensure_request("sbj_after_expiry"))
            .await,
    );
    let (result, error, _) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_none() && error.is_none(),
        "the sweep evicted the aged body"
    );

    // Restart, with the wall clock back where this process started — i.e. before
    // the observation above.
    drop(provider);
    let reopened = open_provider(&env).await;

    let refused = wire(
        &reopened
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        error_code(&refused),
        "RECEIPT_EXPIRED",
        "the reopened provider never re-serves the expiry it committed: {refused}"
    );
    assert_eq!(error_details(&refused)["last_known_status"], "committed");
    let (result, error, _) = receipt_payloads(&pool, &receipt_id).await;
    assert!(
        result.is_none() && error.is_none(),
        "the restart restored neither the body nor an error payload"
    );
    let replay = wire(
        &reopened
            .capture(&admin, capture_request("k-restart-floor", TEXT))
            .await,
    );
    assert_eq!(
        error_code(&replay),
        "RECEIPT_EXPIRED",
        "a replay across the restart does not re-execute: {replay}"
    );
    assert_eq!(
        receipt_rows(&pool, &receipt_id).await,
        1,
        "the namespace still binds the key; only the body is gone"
    );
}

/// §6.10/MEM-R021 / compass D13 residual — **this test documents un-closed
/// behavior, not a guarantee**. The sequence is read → drop → backward clock →
/// reopen: the read path observes the expiry without persisting it (MEM-R011
/// keeps it read-only) and no mutating entry point runs between that observation
/// and the drop, so the floor the reopened provider seeds from is still the one
/// committed by the last sweep before the body aged — the sweep `capture`'s own
/// admission ran, not the provider-open sweep. The reopened provider therefore
/// evaluates at that earlier reading and serves the aged body again.
///
/// Ignored so the suite never presents this as desired behavior. Closing it
/// needs a read-path persistence contract change, which is the owner's decision
/// (compass D13, open); once that lands this test flips to expecting
/// `RECEIPT_EXPIRED`.
#[ignore = "documents the D13 residual — owner adjudication pending"]
#[tokio::test]
async fn read_only_expiry_is_served_again_after_a_restart_under_a_backward_clock() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, _) = retention_limits(&provider, &admin).await;

    wire(
        &provider
            .ensure_subject(&admin, ensure_request(SUBJECT))
            .await,
    );
    let captured = wire(
        &provider
            .capture(&admin, capture_request("k-read-only-expiry", TEXT))
            .await,
    );
    let captured = receipt(&captured).clone();
    let receipt_id = captured["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    let pool = nexus_local_db::open_pool_read_only(&env.db_path)
        .await
        .unwrap();

    // Whatever the last committed sweep wrote is all a reopen can seed from.
    let floor_before = persisted_clock_floor(&pool).await;

    provider.advance_clock(receipt_retention + 1);
    let observed = wire(
        &provider
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        error_code(&observed),
        "RECEIPT_EXPIRED",
        "the read reports the aged body as expired: {observed}"
    );
    assert_eq!(
        persisted_clock_floor(&pool).await,
        floor_before,
        "the read persisted no clock reading and no eviction (MEM-R011)"
    );
    let (result, _, _) = receipt_payloads(&pool, &receipt_id).await;
    assert!(result.is_some(), "the aged body is still on disk");

    // Restart straight from the read: nothing committed the expiry, and the wall
    // clock is back where this process started — i.e. before the observation.
    drop(provider);
    let reopened = open_provider(&env).await;

    let served = wire(
        &reopened
            .get_receipt(&admin, receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(
        read_receipt(&served),
        &captured,
        "D13 residual: the un-persisted expiry is not durable, so the reopened \
         provider serves the aged body again: {served}"
    );
}

/// §6.10/MEM-R021: the retention clock is durable. Every sweep commits the
/// reading it judged retention by, and a reopened provider seeds its clock from
/// that row before it evaluates anything — so a wall clock that moved backwards
/// across the restart can neither lower the floor nor re-stamp a write earlier
/// than the reading the previous process already reached.
#[tokio::test]
async fn restart_resumes_the_durable_retention_floor() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let admin = principal(CREATOR);
    let (receipt_retention, _) = retention_limits(&provider, &admin).await;
    let pool = nexus_local_db::open_pool_read_only(&env.db_path)
        .await
        .unwrap();

    // A sweep just past the receipt window commits the reading it decided at.
    let aged_floor = chrono::Utc::now().timestamp_micros() + (receipt_retention + 1) * 1_000_000;
    provider.advance_clock(receipt_retention + 1);
    wire(
        &provider
            .ensure_subject(&admin, ensure_request("sbj_aged"))
            .await,
    );
    let committed = persisted_clock_floor(&pool).await;
    assert!(
        committed >= aged_floor,
        "the sweep commits the reading it judged retention by: {committed} < {aged_floor}"
    );
    let (committed_at,): (String,) =
        sqlx::query_as("SELECT updated_at FROM memory_provider_meta WHERE key = ?")
            .bind("clock_floor_micros")
            .fetch_one(&pool)
            .await
            .unwrap();

    // Restart with the wall clock back where this process started.
    drop(provider);
    let reopened = open_provider(&env).await;

    // The reopen sweep runs seeded: it neither lowers the floor nor rewrites it.
    assert!(
        persisted_clock_floor(&pool).await >= committed,
        "reopening never lowers the durable floor"
    );

    // The reopened store evaluates at that floor instead of the wall clock that
    // moved backwards, so its write is stamped no earlier than it.
    let written = wire(
        &reopened
            .ensure_subject(&admin, ensure_request("sbj_after_restart"))
            .await,
    );
    let written_id = receipt(&written)["receipt_id"]
        .as_str()
        .expect("receipt id")
        .to_owned();
    let (created_at,): (String,) =
        sqlx::query_as("SELECT created_at FROM memory_receipts WHERE receipt_id = ?")
            .bind(&written_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        created_at.as_str() >= committed_at.as_str(),
        "a write after the restart is stamped at least at the durable floor: \
         created_at {created_at} < floor {committed_at}"
    );
}

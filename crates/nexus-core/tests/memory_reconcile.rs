//! §5.2/D-10 SQL+file reconciliation (C-113): both crash windows, staged
//! publish, bounded recovery, quarantine, and the journal migration.
//!
//! Every scenario runs on a temp nexus home. The crash windows are injected
//! through the provider's `test-hooks` seams — the same shape the round-A
//! commit-boundary proof uses — so they are deterministic, never timing-based:
//!
//! * `inject_commit_fault(false)` stops the effect transaction *before* its
//!   COMMIT (the journal never exists);
//! * `inject_commit_fault(true)` stops it *after* COMMIT (the journal intent is
//!   committed, no file effect ran);
//! * `inject_file_fault(false)` stops after the hidden staged write, before the
//!   publish rename;
//! * `inject_file_fault(true)` stops after the publish rename, before the
//!   journal is settled;
//! * `inject_settle_fault(..)` fails the post-commit settlement itself.
//!
//! A crash is simulated by dropping the provider and reopening it; `provision`
//! runs the bounded recovery pass on every open. Until that pass settles an
//! intent, the receipt of the crashed operation stays `pending`: both the
//! same-key replay and `memory.receipts.get` report the pending state.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nexus_core::memory_provider::LocalMemoryProvider;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{MemoryPort, SpokeResult};
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SPACE: &str = "spc_local_default";
const SUBJECT: &str = "sbj_mira";
const KIND: &str = "nexus.custom";
const TEXT: &str = "Mira promised to return before the northern gate opens at dawn.";
const UPDATED: &str = "Mira promised to return before the western gate opens at dusk.";
const THIRD: &str = "Mira promised nothing; the northern route stayed closed all winter.";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
    db_path: PathBuf,
}

fn seed_env() -> Env {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    std::fs::create_dir_all(user_home.join(".nexus42")).unwrap();
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

fn admin() -> schemas::Principal {
    principal(CREATOR)
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

fn receipt(body: &Value) -> &Value {
    assert_eq!(
        body["status"], "committed",
        "expected a committed receipt, got {body}"
    );
    body
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

fn record_create_request(key: &str, text: &str) -> schemas::MemoryRecordsCreateRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "kind": KIND,
        "content": { "text": text },
        "scope": { "visibility": "shared" },
        "idempotency_key": key,
    }))
}

fn record_get_request(record_id: &str) -> schemas::MemoryRecordsGetRequest {
    dto(json!({ "space_id": SPACE, "record_id": record_id }))
}

fn records_list_request() -> schemas::MemoryRecordsListRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "selection": { "mode": "subject_all" },
    }))
}

fn recall_request(max_items: u64) -> schemas::MemoryRecallRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "selection": { "mode": "subject_all" },
        "budget": { "max_items": max_items },
    }))
}

fn capture_request(key: &str) -> schemas::MemoryCaptureRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "content": { "text": TEXT },
        "capture_scope": { "visibility": "shared" },
        "source_context_ids": [],
        "idempotency_key": key,
    }))
}

fn review_request(key: &str, candidate_id: &str) -> schemas::MemoryCandidatesReviewRequest {
    dto(json!({
        "space_id": SPACE,
        "candidate_id": candidate_id,
        "expected_revision": 1,
        "decision": { "action": "promote" },
        "idempotency_key": key,
    }))
}

fn dismiss_request(key: &str, candidate_id: &str) -> schemas::MemoryCandidatesDismissRequest {
    dto(json!({
        "space_id": SPACE,
        "candidate_id": candidate_id,
        "expected_revision": 1,
        "reason": "superseded",
        "idempotency_key": key,
    }))
}

fn profile_update_request(key: &str, body: &str) -> schemas::MemoryProfileUpdateRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "profile_kind": "personality",
        "expected_revision": null,
        "document": { "format": "markdown", "body": body },
        "idempotency_key": key,
    }))
}

fn profile_revise_request(key: &str, body: &str) -> schemas::MemoryProfileUpdateRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "profile_kind": "personality",
        "expected_revision": 1,
        "document": { "format": "markdown", "body": body },
        "idempotency_key": key,
    }))
}

fn update_request(
    key: &str,
    record_id: &str,
    revision: u64,
    text: &str,
) -> schemas::MemoryRecordsUpdateRequest {
    dto(json!({
        "space_id": SPACE,
        "record_id": record_id,
        "expected_revision": revision,
        "content": { "text": text },
        "idempotency_key": key,
    }))
}

fn delete_request(
    key: &str,
    record_id: &str,
    revision: u64,
) -> schemas::MemoryRecordsDeleteRequest {
    dto(json!({
        "space_id": SPACE,
        "record_id": record_id,
        "expected_revision": revision,
        "reason": "superseded",
        "idempotency_key": key,
    }))
}

fn candidate_get_request(candidate_id: &str) -> schemas::MemoryCandidatesGetRequest {
    dto(json!({ "space_id": SPACE, "candidate_id": candidate_id }))
}

fn receipts_get_request(receipt_id: &str) -> schemas::MemoryReceiptsGetRequest {
    dto(json!({ "space_id": SPACE, "receipt_id": receipt_id }))
}

/// Every regular file under the provider's projection root, as sorted paths
/// relative to it. A missing root is the honest-empty case.
fn files_under(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect(root, root, &mut files);
    files.sort();
    files
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            files.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn record_id_of(body: &Value) -> String {
    body["result"]["record_id"]
        .as_str()
        .or_else(|| body["result"]["record_ids"][0].as_str())
        .unwrap_or_else(|| panic!("expected a record id, got {body}"))
        .to_string()
}

fn candidate_id_of(body: &Value) -> String {
    body["result"]["candidate_ids"][0]
        .as_str()
        .unwrap_or_else(|| panic!("expected a candidate id, got {body}"))
        .to_string()
}

fn read_receipt(body: &Value) -> &Value {
    &body["receipt"]
}

fn stranger() -> schemas::Principal {
    principal(OTHER)
}

async fn store_pool(env: &Env) -> sqlx::SqlitePool {
    nexus_local_db::open_pool(&env.db_path).await.unwrap()
}

/// The receipt id a crashed caller never saw, straight from the store.
async fn receipt_id_of_key(env: &Env, key: &str) -> String {
    let pool = store_pool(env).await;
    let (receipt_id,): (String,) =
        sqlx::query_as("SELECT receipt_id FROM memory_receipts WHERE idempotency_key = ?")
            .bind(key)
            .fetch_one(&pool)
            .await
            .unwrap();
    receipt_id
}

/// The only record id in the space, for fixtures that crashed before the caller
/// learned it.
async fn only_record_id(env: &Env) -> String {
    let pool = store_pool(env).await;
    let (record_id,): (String,) = sqlx::query_as("SELECT record_id FROM memory_records")
        .fetch_one(&pool)
        .await
        .unwrap();
    record_id
}

async fn any_record_id(env: &Env) -> String {
    let pool = store_pool(env).await;
    let (record_id,): (String,) =
        sqlx::query_as("SELECT record_id FROM memory_records ORDER BY record_id LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    record_id
}

/// Every journal intent state of the space, in settlement order.
async fn journal_states(env: &Env) -> Vec<String> {
    let pool = store_pool(env).await;
    let rows: Vec<(String,)> = sqlx::query_as("SELECT state FROM memory_journal ORDER BY seq")
        .fetch_all(&pool)
        .await
        .unwrap();
    rows.into_iter().map(|(state,)| state).collect()
}

/// The persisted journal payloads of the space, in settlement order.
async fn journal_targets(env: &Env) -> Vec<String> {
    let pool = store_pool(env).await;
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT targets_json FROM memory_journal ORDER BY seq")
            .fetch_all(&pool)
            .await
            .unwrap();
    rows.into_iter().map(|(targets,)| targets).collect()
}

/// The last file target an intent journalled — the path whose projection the
/// intent owes.
fn journalled_target(targets_json: &str) -> String {
    let effects: Vec<Value> = serde_json::from_str(targets_json).unwrap();
    effects
        .last()
        .and_then(|effect| effect["path"].as_str())
        .unwrap_or_else(|| panic!("an intent journals at least one target: {targets_json}"))
        .to_string()
}

// ── Window (a): a crash before the journal commit ───────────────────────

/// The journal commit is the point of no return. A crash before it leaves no
/// effect at all — no row, no receipt, no file — and the idempotency key is
/// still free, so the retry performs the operation for the first time.
#[tokio::test]
async fn crash_before_journal_commit_leaves_no_effect_and_frees_the_key() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_commit_fault(false);
    let lost = wire(
        &provider
            .create_record(&admin(), record_create_request("k-pre-commit", TEXT))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class(&lost), "uncertain");

    // Nothing was journaled, staged or published.
    assert_eq!(files_under(&root), Vec::<String>::new());
    let listed = wire(
        &provider
            .list_records(&admin(), records_list_request())
            .await,
    );
    assert_eq!(listed["total_authorized"], 0);
    let recalled = wire(&provider.recall(&admin(), recall_request(10)).await);
    assert_eq!(recalled["total_authorized"], 0);

    drop(provider);
    let provider = open_provider(&env).await;
    // The recovery pass has nothing to settle, and the retry is a first effect.
    assert_eq!(files_under(&root), Vec::<String>::new());
    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-pre-commit", TEXT))
            .await,
    );
    let created = receipt(&created);
    let record_id = record_id_of(created);
    assert_eq!(files_under(&root), vec![format!("records/{record_id}.md")]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT);
}

// ── Window (b): a crash after the journal commit ────────────────────────

/// After the journal commit the SQL effect is durable and recoverable. Until
/// recovery settles the intent, the subject's objects are withheld from
/// recall/list/get and the receipt is *pending* — the file effect was never
/// published, so neither the replay nor `memory.receipts.get` may report a
/// terminal outcome. A reopen publishes the planned file from the journaled
/// bytes and only then terminalizes the receipt.
#[tokio::test]
async fn crash_after_journal_commit_is_withheld_then_published_on_reopen() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .create_record(&admin(), record_create_request("k-post-commit", TEXT))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");

    // §5.2 step 2: the receipt is durable at the point of no return, but the
    // pending window reports the pending state on both channels.
    let replayed = wire(
        &provider
            .create_record(&admin(), record_create_request("k-post-commit", TEXT))
            .await,
    );
    assert_eq!(error_code(&replayed), "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class(&replayed), "uncertain");
    let receipt_id = receipt_id_of_key(&env, "k-post-commit").await;
    let read = wire(
        &provider
            .get_receipt(&admin(), receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(error_code(&read), "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class(&read), "uncertain");

    // D-10 isolation: no half-committed content is readable while recovering.
    let record_id = only_record_id(&env).await;
    let withheld = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(error_code(&withheld), "NOT_FOUND");
    let listed = wire(
        &provider
            .list_records(&admin(), records_list_request())
            .await,
    );
    assert_eq!(listed["total_authorized"], 0);
    let recalled = wire(&provider.recall(&admin(), recall_request(10)).await);
    assert_eq!(recalled["total_authorized"], 0);
    assert_eq!(files_under(&root), Vec::<String>::new());

    drop(provider);
    let provider = open_provider(&env).await;

    // Recovery published exactly the planned target and settled the journal.
    assert_eq!(files_under(&root), vec![format!("records/{record_id}.md")]);
    let published = std::fs::read_to_string(root.join(format!("records/{record_id}.md"))).unwrap();
    assert!(
        published.contains(TEXT),
        "the published projection carries the record body: {published}"
    );
    assert_eq!(journal_states(&env).await, vec!["applied"]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT);
    let recalled = wire(&provider.recall(&admin(), recall_request(10)).await);
    assert_eq!(recalled["total_authorized"], 1);

    // Only now is the receipt terminal, on both channels and with the same
    // outcome the crashed caller never saw.
    let replayed = wire(
        &provider
            .create_record(&admin(), record_create_request("k-post-commit", TEXT))
            .await,
    );
    let replayed = receipt(&replayed);
    assert_eq!(record_id_of(replayed), record_id);
    let read = wire(
        &provider
            .get_receipt(&admin(), receipts_get_request(&receipt_id))
            .await,
    );
    assert_eq!(read_receipt(&read), replayed);
}

/// C-113's "post-commit pre-publish" window: the staged file exists and
/// matches the intent digest, so recovery publishes it instead of regenerating.
#[tokio::test]
async fn crash_after_staging_is_published_from_the_staged_file() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_file_fault(false);
    let lost = wire(
        &provider
            .create_record(&admin(), record_create_request("k-staged", TEXT))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");

    let staged = files_under(&root);
    assert_eq!(staged.len(), 1, "one hidden staged file: {staged:?}");
    assert!(
        staged[0].contains(".staged"),
        "the staged file stays hidden until publish: {staged:?}"
    );
    let replayed = wire(
        &provider
            .create_record(&admin(), record_create_request("k-staged", TEXT))
            .await,
    );
    assert_eq!(error_code(&replayed), "OUTCOME_UNCERTAIN");
    let record_id = only_record_id(&env).await;
    let withheld = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(error_code(&withheld), "NOT_FOUND");

    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(files_under(&root), vec![format!("records/{record_id}.md")]);
    let staged_published =
        std::fs::read_to_string(root.join(format!("records/{record_id}.md"))).unwrap();
    assert!(staged_published.contains(TEXT));
    assert_eq!(journal_states(&env).await, vec!["applied"]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT);
}

/// A crash after the publish rename: the target is already consistent, so
/// recovery marks the intent applied without rewriting or re-staging it.
#[tokio::test]
async fn crash_after_publish_marks_the_intent_applied() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_file_fault(true);
    let lost = wire(
        &provider
            .create_record(&admin(), record_create_request("k-published", TEXT))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");
    let replayed = wire(
        &provider
            .create_record(&admin(), record_create_request("k-published", TEXT))
            .await,
    );
    // The file is published but the intent is not settled, so the receipt is
    // still pending and the replay says so.
    assert_eq!(error_code(&replayed), "OUTCOME_UNCERTAIN");
    let record_id = only_record_id(&env).await;
    assert_eq!(files_under(&root), vec![format!("records/{record_id}.md")]);
    let before = std::fs::read_to_string(root.join(format!("records/{record_id}.md"))).unwrap();

    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(files_under(&root), vec![format!("records/{record_id}.md")]);
    let after = std::fs::read_to_string(root.join(format!("records/{record_id}.md"))).unwrap();
    assert_eq!(after, before, "recovery neither rewrites nor re-stages it");
    assert_eq!(journal_states(&env).await, vec!["applied"]);
    let replayed = wire(
        &provider
            .create_record(&admin(), record_create_request("k-published", TEXT))
            .await,
    );
    assert_eq!(record_id_of(receipt(&replayed)), record_id);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT);
}

/// A staged file that no longer matches the intent digest is regenerated from
/// the journaled bytes, never published as-is.
#[tokio::test]
async fn mismatched_staged_file_is_regenerated_from_the_journal() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_file_fault(false);
    wire(
        &provider
            .create_record(&admin(), record_create_request("k-mismatch", TEXT))
            .await,
    );
    let record_id = only_record_id(&env).await;
    let staged = files_under(&root);
    assert_eq!(staged.len(), 1);
    std::fs::write(root.join(&staged[0]), "corrupted by a torn write").unwrap();

    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(files_under(&root), vec![format!("records/{record_id}.md")]);
    let published = std::fs::read_to_string(root.join(format!("records/{record_id}.md"))).unwrap();
    assert!(
        published.contains(TEXT),
        "regenerated from the journal: {published}"
    );
    assert!(!published.contains("torn write"));
    assert_eq!(journal_states(&env).await, vec!["applied"]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT);
}

/// A target that already holds content the intent did not produce is never
/// overwritten: the intent is quarantined and the subject's reads answer the
/// terminal `INTERNAL_ERROR` (D-10) instead of serving half-committed content.
#[tokio::test]
async fn conflicting_target_is_quarantined_and_the_subject_reads_terminal() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_commit_fault(true);
    wire(
        &provider
            .create_record(&admin(), record_create_request("k-conflict", TEXT))
            .await,
    );
    let record_id = only_record_id(&env).await;

    // A different writer left bytes at the planned target.
    let target = root.join(format!("records/{record_id}.md"));
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "content this intent never produced").unwrap();

    drop(provider);
    let provider = open_provider(&env).await;

    let view = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(error_code(&view), "INTERNAL_ERROR");
    assert_eq!(retry_class(&view), "terminal");
    assert_eq!(error_details(&view)["subject_id"], SUBJECT);
    for body in [
        wire(
            &provider
                .list_records(&admin(), records_list_request())
                .await,
        ),
        wire(&provider.recall(&admin(), recall_request(10)).await),
    ] {
        assert_eq!(error_code(&body), "INTERNAL_ERROR");
        assert_eq!(retry_class(&body), "terminal");
    }
    // §5.2/D-10 disclosure: the terminal refusal names the subject, so it is
    // reserved for authorized readers. An unauthorized probe learns nothing —
    // not that the record exists, not which subject it belongs to.
    let probed = wire(
        &provider
            .get_record(&stranger(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(error_code(&probed), "NOT_FOUND");
    assert_eq!(retry_class(&probed), "terminal");
    assert_eq!(error_details(&probed)["subject_id"], Value::Null);
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "content this intent never produced",
        "a quarantined intent never overwrites foreign content"
    );
}

// ── The three file-effect seams ─────────────────────────────────────────

/// Every content seam produces its projection: record create/promote, candidate
/// consumption (the content-free audit shell) and profile authoring.
#[tokio::test]
async fn all_three_seams_publish_their_projections() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    // capture → review(promote): a record projection plus the consumed shell.
    let captured = wire(
        &provider
            .capture(&admin(), capture_request("k-capture"))
            .await,
    );
    let candidate_id = candidate_id_of(receipt(&captured));
    let reviewed = wire(
        &provider
            .review_candidate(&admin(), review_request("k-review", &candidate_id))
            .await,
    );
    let record_id = record_id_of(receipt(&reviewed));
    let mut expected = vec![
        format!("candidates/{candidate_id}.md"),
        format!("records/{record_id}.md"),
    ];
    expected.sort();
    assert_eq!(files_under(&root), expected);
    let record_file =
        std::fs::read_to_string(root.join(format!("records/{record_id}.md"))).unwrap();
    assert!(record_file.contains(TEXT));
    let shell_file =
        std::fs::read_to_string(root.join(format!("candidates/{candidate_id}.md"))).unwrap();
    assert!(shell_file.contains("status: consumed"));
    assert!(
        !shell_file.contains(TEXT),
        "the consumed shell is content-free: {shell_file}"
    );

    // profile.update: the authored document becomes its own projection.
    wire(
        &provider
            .update_profile(
                &admin(),
                profile_update_request("k-profile", "# Soul\n\nSteady."),
            )
            .await,
    );
    let files = files_under(&root);
    let profiles: Vec<&String> = files
        .iter()
        .filter(|path| path.starts_with("profiles/"))
        .collect();
    assert_eq!(profiles.len(), 1, "one profile projection: {files:?}");
    let profile_file = std::fs::read_to_string(root.join(profiles[0])).unwrap();
    assert!(profile_file.contains("profile_kind: personality"));
    assert!(profile_file.contains("Steady."));

    // dismiss: the shell is projected, and no record is produced.
    let captured = wire(
        &provider
            .capture(&admin(), capture_request("k-capture-2"))
            .await,
    );
    let dismissed_id = candidate_id_of(receipt(&captured));
    wire(
        &provider
            .dismiss_candidate(&admin(), dismiss_request("k-dismiss", &dismissed_id))
            .await,
    );
    let files = files_under(&root);
    assert!(
        files.contains(&format!("candidates/{dismissed_id}.md")),
        "the dismissed shell is projected: {files:?}"
    );
    assert_eq!(
        files
            .iter()
            .filter(|path| path.starts_with("records/"))
            .count(),
        1,
        "a dismissal produces no record projection"
    );
}

// ── C1: a committed version publishes over its own predecessor ──────────

/// A crash after the commit of an **update** leaves the previous legitimate
/// version at the target. Recovery must recognise it as the intent's recorded
/// predecessor and publish the committed new version, not quarantine the
/// subject on its own old file.
#[tokio::test]
async fn update_crash_publishes_over_the_previous_committed_version() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-update-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let target = root.join(format!("records/{record_id}.md"));
    assert!(std::fs::read_to_string(&target).unwrap().contains(TEXT));

    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .update_record(&admin(), update_request("k-update", &record_id, 1, UPDATED))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");

    drop(provider);
    let provider = open_provider(&env).await;

    let published = std::fs::read_to_string(&target).unwrap();
    assert!(
        published.contains(UPDATED) && !published.contains(TEXT),
        "recovery publishes the committed update over its predecessor: {published}"
    );
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], UPDATED);
    assert_eq!(served["record"]["revision"], 2);
}

/// The delete crash window: the committed tombstone replaces the deleted body
/// at the target, and that body is gone from the projection.
#[tokio::test]
async fn delete_crash_publishes_the_tombstone_over_the_deleted_body() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-delete-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let target = root.join(format!("records/{record_id}.md"));

    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .delete_record(&admin(), delete_request("k-delete", &record_id, 1))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");

    drop(provider);
    let provider = open_provider(&env).await;

    let published = std::fs::read_to_string(&target).unwrap();
    assert!(
        published.contains("tombstoned: true"),
        "recovery publishes the committed tombstone over its predecessor: {published}"
    );
    assert!(
        !published.contains(TEXT),
        "the deleted body is not in the projection: {published}"
    );
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], "");
    assert!(served["record"]["tombstone"].is_object());
}

/// The profile-update crash window: the committed document replaces the
/// previous legitimate profile projection.
#[tokio::test]
async fn profile_crash_publishes_over_the_previous_document() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    wire(
        &provider
            .update_profile(
                &admin(),
                profile_update_request("k-profile-1", "# Soul\n\nSteady."),
            )
            .await,
    );
    let profiles: Vec<String> = files_under(&root)
        .into_iter()
        .filter(|path| path.starts_with("profiles/"))
        .collect();
    assert_eq!(profiles.len(), 1, "one profile projection: {profiles:?}");
    let target = root.join(&profiles[0]);

    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .update_profile(
                &admin(),
                profile_revise_request("k-profile-2", "# Soul\n\nSteadier."),
            )
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");

    drop(provider);
    let provider = open_provider(&env).await;

    let published = std::fs::read_to_string(&target).unwrap();
    assert!(
        published.contains("Steadier.") && !published.contains("Steady."),
        "recovery publishes the committed document over its predecessor: {published}"
    );
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
    assert_eq!(
        wire(
            &provider
                .list_records(&admin(), records_list_request())
                .await
        )["total_authorized"],
        0
    );
}

// ── C2: publish never overwrites foreign bytes, and orders per target ────

/// The normal publish path refuses a target holding content the intent never
/// produced: the effect commit stands, the bytes stay, and recovery later
/// quarantines the subject rather than clobbering the target.
#[tokio::test]
async fn publish_never_overwrites_foreign_bytes_on_the_normal_path() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-foreign-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let target = root.join(format!("records/{record_id}.md"));
    let foreign = "bytes this provider never wrote";
    std::fs::write(&target, foreign).unwrap();

    let refused = wire(
        &provider
            .update_record(
                &admin(),
                update_request("k-foreign", &record_id, 1, UPDATED),
            )
            .await,
    );
    assert_eq!(error_code(&refused), "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class(&refused), "uncertain");
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        foreign,
        "the normal publish path never overwrites foreign content"
    );

    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(std::fs::read_to_string(&target).unwrap(), foreign);
    assert_eq!(journal_states(&env).await, vec!["applied", "quarantined"]);
    let view = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(error_code(&view), "INTERNAL_ERROR");
    assert_eq!(retry_class(&view), "terminal");
}

/// The recovery path applies the same ownership rule to an update crash: the
/// recorded predecessor is the previous *version*, not whatever bytes sit at
/// the target.
#[tokio::test]
async fn recovery_never_overwrites_foreign_bytes_for_an_update() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(
                &admin(),
                record_create_request("k-recovery-foreign-create", TEXT),
            )
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let target = root.join(format!("records/{record_id}.md"));

    provider.inject_commit_fault(true);
    wire(
        &provider
            .update_record(
                &admin(),
                update_request("k-recovery-foreign", &record_id, 1, UPDATED),
            )
            .await,
    );
    let foreign = "bytes another writer left behind";
    std::fs::write(&target, foreign).unwrap();

    drop(provider);
    let provider = open_provider(&env).await;

    assert_eq!(std::fs::read_to_string(&target).unwrap(), foreign);
    assert_eq!(journal_states(&env).await, vec!["applied", "quarantined"]);
    let view = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(error_code(&view), "INTERNAL_ERROR");
    assert_eq!(retry_class(&view), "terminal");
}

/// Two commits of one target publish in commit order. Update A stops after
/// staging (its version is not on disk yet), so B may not publish over the
/// target before A's owed projection is there: B settles A inline, then
/// publishes its own version — the file never trails the row.
#[tokio::test]
async fn committed_updates_publish_in_commit_order() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-order-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let target = root.join(format!("records/{record_id}.md"));

    provider.inject_file_fault(false);
    let first = wire(
        &provider
            .update_record(
                &admin(),
                update_request("k-order-a", &record_id, 1, UPDATED),
            )
            .await,
    );
    assert_eq!(error_code(&first), "OUTCOME_UNCERTAIN");
    let second = wire(
        &provider
            .update_record(&admin(), update_request("k-order-b", &record_id, 2, THIRD))
            .await,
    );
    let second = receipt(&second);
    assert_eq!(second["result"]["revision"], 3);

    // B settled A's unsettled predecessor inline, so A is terminal too.
    let pool = store_pool(&env).await;
    let (first_status,): (String,) =
        sqlx::query_as("SELECT status FROM memory_receipts WHERE idempotency_key = ?")
            .bind("k-order-a")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        first_status, "committed",
        "the settled predecessor's receipt is terminal, never pending"
    );
    drop(pool);

    drop(provider);
    let provider = open_provider(&env).await;

    let published = std::fs::read_to_string(&target).unwrap();
    assert!(
        published.contains(THIRD) && !published.contains(UPDATED),
        "the target carries B, not the stale A: {published}"
    );
    assert_eq!(
        journal_states(&env).await,
        vec!["applied", "applied", "applied"]
    );
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], THIRD);
    assert_eq!(served["record"]["revision"], 3);
}

// ── C3: a settled intent retains no projection content ──────────────────

/// A settled intent must not keep the body it published: after create →
/// delete — before and after a restart — the journal holds no record text, and
/// the outbox events carry ids only.
#[tokio::test]
async fn settled_intents_do_not_retain_projection_content() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-retain-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let deleted = wire(
        &provider
            .delete_record(&admin(), delete_request("k-retain-delete", &record_id, 1))
            .await,
    );
    assert_eq!(deleted["status"], "committed");

    assert!(
        journal_targets(&env)
            .await
            .iter()
            .all(|targets| targets == "[]"),
        "a settled intent keeps no projection payload"
    );
    let pool = store_pool(&env).await;
    let (leaked,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memory_journal WHERE targets_json LIKE ?")
            .bind("%northern gate%")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(leaked, 0, "no journal row retains the deleted body");
    let (events,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM core_changes WHERE resource_kind = 'memory_journal'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(events > 0, "the journal publishes its change events");
    let (content_bearing,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM core_changes WHERE resource_kind = 'memory_journal' \
         AND resource_id NOT LIKE 'rcp_%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        content_bearing, 0,
        "outbox events carry receipt ids only, never content"
    );
    drop(pool);

    drop(provider);
    let provider = open_provider(&env).await;
    assert!(
        journal_targets(&env)
            .await
            .iter()
            .all(|targets| targets == "[]"),
        "a restart neither restores nor re-stages the scrubbed payload"
    );
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
    assert_eq!(
        wire(
            &provider
                .list_records(&admin(), records_list_request())
                .await
        )["total_authorized"],
        0
    );
    let published = std::fs::read_to_string(root.join(format!("records/{record_id}.md"))).unwrap();
    assert!(!published.contains(TEXT));
}

// ── C4: post-commit settle failures are uncertain, never retryable ───────

/// Both failure modes of the post-commit settlement (its transaction never
/// opens / its write fails) leave the effect committed, so the caller learns
/// `OUTCOME_UNCERTAIN` — never a retryable `INTERNAL_ERROR`.
async fn assert_settle_failure_is_uncertain(before_write: bool) {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;

    provider.inject_settle_fault(before_write);
    let lost = wire(
        &provider
            .create_record(&admin(), record_create_request("k-settle", TEXT))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");
    assert_eq!(retry_class(&lost), "uncertain");

    // The effect committed and the next open settles the pending intent.
    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(journal_states(&env).await, vec!["applied"]);
    let replayed = wire(
        &provider
            .create_record(&admin(), record_create_request("k-settle", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&replayed));
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT);
}

#[tokio::test]
async fn settle_open_failure_after_commit_is_uncertain() {
    assert_settle_failure_is_uncertain(true).await;
}

#[tokio::test]
async fn settle_write_failure_after_commit_is_uncertain() {
    assert_settle_failure_is_uncertain(false).await;
}

// ── C5: quarantine is not a disclosure channel ──────────────────────────

/// A quarantined candidate answers the terminal refusal to a reader that may
/// see it, and is indistinguishable from any absent candidate for one that may
/// not.
#[tokio::test]
async fn quarantined_candidate_is_invisible_to_unauthorized_probers() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let captured = wire(
        &provider
            .capture(&admin(), capture_request("k-probe-capture"))
            .await,
    );
    let candidate_id = candidate_id_of(receipt(&captured));
    provider.inject_commit_fault(true);
    let lost = wire(
        &provider
            .dismiss_candidate(&admin(), dismiss_request("k-probe-dismiss", &candidate_id))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");

    let target = root.join(format!("candidates/{candidate_id}.md"));
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "bytes this intent never produced").unwrap();

    drop(provider);
    let provider = open_provider(&env).await;

    let view = wire(
        &provider
            .get_candidate(&admin(), candidate_get_request(&candidate_id))
            .await,
    );
    assert_eq!(error_code(&view), "INTERNAL_ERROR");
    assert_eq!(retry_class(&view), "terminal");
    assert_eq!(error_details(&view)["subject_id"], SUBJECT);

    let unknown = wire(
        &provider
            .get_candidate(
                &stranger(),
                candidate_get_request("cnd_0000000000000000000000000000"),
            )
            .await,
    );
    let probed = wire(
        &provider
            .get_candidate(&stranger(), candidate_get_request(&candidate_id))
            .await,
    );
    assert_eq!(error_code(&probed), "NOT_FOUND");
    assert_eq!(error_details(&probed), error_details(&unknown));
}

// ── C6: a bounded pass leaves nothing silently pending ──────────────────

/// More unsettled intents than one pass settles: the pass keeps its bound, the
/// leftovers read terminally instead of looking like absence, and the next open
/// finishes the work.
#[tokio::test]
async fn overflow_beyond_the_bounded_pass_reads_terminal_then_recovers() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let count: i64 = 65;
    for index in 0..count {
        provider.inject_commit_fault(true);
        let body = wire(
            &provider
                .create_record(
                    &admin(),
                    record_create_request(&format!("k-overflow-{index}"), TEXT),
                )
                .await,
        );
        assert_eq!(error_code(&body), "OUTCOME_UNCERTAIN");
    }
    assert_eq!(files_under(&root), Vec::<String>::new());

    drop(provider);
    let provider = open_provider(&env).await;
    let states = journal_states(&env).await;
    assert_eq!(
        states.iter().filter(|state| *state == "applied").count(),
        64,
        "one pass settles at most the work bound: {states:?}"
    );
    assert_eq!(
        states.iter().filter(|state| *state == "deferred").count(),
        1,
        "the remainder is deferred, not silently pending: {states:?}"
    );
    let first = any_record_id(&env).await;
    let probed = wire(
        &provider
            .get_record(&admin(), record_get_request(&first))
            .await,
    );
    assert_eq!(error_code(&probed), "INTERNAL_ERROR");
    assert_eq!(retry_class(&probed), "terminal");
    let listed = wire(
        &provider
            .list_records(&admin(), records_list_request())
            .await,
    );
    assert_eq!(error_code(&listed), "INTERNAL_ERROR");
    assert_eq!(retry_class(&listed), "terminal");

    drop(provider);
    let provider = open_provider(&env).await;
    assert!(journal_states(&env)
        .await
        .iter()
        .all(|state| state == "applied"));
    let listed = wire(
        &provider
            .list_records(&admin(), records_list_request())
            .await,
    );
    assert_eq!(listed["total_authorized"], count);
}

// ── NF4: a bounded pass advances past a head that keeps conflicting ─────

/// 65 quarantined intents, the first 64 conflicting forever: a pass that always
/// restarts at the oldest unsettled intent retries the same 64 by `seq` and the
/// 65th is never selected — resolving only its target never gets it recovered.
/// The pass now resumes after the `seq` the previous pass stopped at (and wraps),
/// so the bound stays intact and the tail is reached.
#[tokio::test]
async fn a_bounded_pass_advances_past_persistently_conflicting_heads() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let count = 65;
    for index in 0..count {
        provider.inject_commit_fault(true);
        wire(
            &provider
                .create_record(
                    &admin(),
                    record_create_request(&format!("k-starve-{index}"), TEXT),
                )
                .await,
        );
    }
    let targets: Vec<String> = journal_targets(&env)
        .await
        .iter()
        .map(|journaled| journalled_target(journaled))
        .collect();
    assert_eq!(targets.len(), count);
    // Every target conflicts: no intent can settle yet.
    std::fs::create_dir_all(root.join("records")).unwrap();
    for path in &targets {
        std::fs::write(root.join(path), "a writer this provider does not own").unwrap();
    }

    // Pass 1 settles the first 64 and defers the one the bound did not reach.
    drop(provider);
    let provider = open_provider(&env).await;
    let states = journal_states(&env).await;
    assert_eq!(
        states
            .iter()
            .filter(|state| *state == "quarantined")
            .count(),
        count - 1,
        "one pass still settles at most the work bound: {states:?}"
    );
    assert_eq!(
        states.iter().filter(|state| *state == "deferred").count(),
        1,
        "the intent the bound did not reach is deferred, not silently dropped: {states:?}"
    );

    // Pass 2 reaches the deferred one; it conflicts too, so all 65 are now
    // quarantined behind each other.
    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(
        journal_states(&env)
            .await
            .iter()
            .filter(|state| *state == "quarantined")
            .count(),
        count,
        "the whole quarantine is settled-as-failed before the head is resolved"
    );

    // Only the 65th's conflict is resolved; the head still conflicts.
    std::fs::remove_file(root.join(&targets[count - 1])).unwrap();
    drop(provider);
    let provider = open_provider(&env).await;
    let last = root.join(&targets[count - 1]);
    let Ok(published) = std::fs::read_to_string(&last) else {
        panic!(
            "the next pass reaches the intent behind the conflicting head: {:?}",
            journal_states(&env).await
        );
    };
    assert!(published.contains(TEXT), "{published}");
    assert_eq!(
        journal_states(&env)
            .await
            .iter()
            .filter(|state| *state == "quarantined")
            .count(),
        count - 1,
        "the conflicting head stays quarantined instead of being published over"
    );
    let listed = wire(
        &provider
            .list_records(&admin(), records_list_request())
            .await,
    );
    assert_eq!(error_code(&listed), "INTERNAL_ERROR");
    assert_eq!(retry_class(&listed), "terminal");
}

// ── F2: a later publication never overtakes an unsettled predecessor ────

/// A create that stops after staging owns a target nothing has written yet
/// (`row + journal + hidden staged file`, no target). A later delete of that
/// record must not install a tombstone as though no projection had ever been
/// owed: the delete settles the earlier intent inline first, so the record is
/// published before its tombstone replaces it — the file is never newer than
/// what recovery owes, and the earlier body leaves the journal with it.
#[tokio::test]
async fn staged_create_predecessor_is_settled_before_a_tombstone_is_published() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_file_fault(false);
    let staged = wire(
        &provider
            .create_record(
                &admin(),
                record_create_request("k-predecessor-create", TEXT),
            )
            .await,
    );
    assert_eq!(error_code(&staged), "OUTCOME_UNCERTAIN");
    let record_id = only_record_id(&env).await;
    let target = root.join(format!("records/{record_id}.md"));
    assert!(
        !target.exists(),
        "the after-staging window leaves no target to publish over"
    );

    let deleted = wire(
        &provider
            .delete_record(
                &admin(),
                delete_request("k-predecessor-delete", &record_id, 1),
            )
            .await,
    );
    let deleted = receipt(&deleted);
    assert_eq!(deleted["result"]["record_id"], record_id);
    assert_eq!(deleted["result"]["tombstoned"], true);

    let published = std::fs::read_to_string(&target).unwrap();
    assert!(published.contains("tombstoned: true"), "{published}");
    assert!(
        !published.contains(TEXT),
        "the deleted body is gone: {published}"
    );
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
    assert!(
        journal_targets(&env)
            .await
            .iter()
            .all(|targets| targets == "[]"),
        "a settled predecessor drops the body it no longer owes"
    );
    let pool = store_pool(&env).await;
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM memory_receipts WHERE idempotency_key = ?")
            .bind("k-predecessor-create")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        status, "committed",
        "the inline-settled predecessor's receipt is terminal"
    );
    drop(pool);

    drop(provider);
    let provider = open_provider(&env).await;
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert!(served["record"]["tombstone"].is_object(), "{served}");
}

/// The other half of the same rule: when the predecessor cannot be settled (its
/// target holds bytes the intent never produced), the later write is refused
/// before its own effect commits — typed retryable, naming the unsettled
/// predecessor — instead of publishing over it.
#[tokio::test]
async fn an_unsettleable_predecessor_refuses_the_later_write() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_commit_fault(true);
    wire(
        &provider
            .create_record(&admin(), record_create_request("k-refuse-create", TEXT))
            .await,
    );
    let record_id = only_record_id(&env).await;
    let target = root.join(format!("records/{record_id}.md"));
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let foreign = "content this intent never produced";
    std::fs::write(&target, foreign).unwrap();
    let predecessor = receipt_id_of_key(&env, "k-refuse-create").await;

    let refused = wire(
        &provider
            .delete_record(&admin(), delete_request("k-refuse-delete", &record_id, 1))
            .await,
    );
    assert_eq!(error_code(&refused), "INTERNAL_ERROR");
    assert_eq!(retry_class(&refused), "retryable");
    assert_eq!(
        error_details(&refused)["receipt_id"].as_str(),
        Some(predecessor.as_str()),
        "the refusal names the unsettled predecessor: {refused}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        foreign,
        "the refused deletion publishes nothing"
    );
    assert_eq!(
        journal_states(&env).await,
        vec!["quarantined"],
        "only the predecessor changed state; the deletion never committed"
    );
    let pool = store_pool(&env).await;
    let (deletes,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memory_receipts WHERE op = ?")
        .bind("memory.records.delete")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(deletes, 0, "a refused write leaves no receipt");
}

// ── NF2: the swap's own crash window is recoverable ─────────────────────

/// The window the rename-first protocol opened: the deleted body was displaced
/// into the receipt's scratch sibling and the tombstone never linked. Recovery
/// must finish the intent — and the superseded body must not survive as a
/// hidden file next to its own tombstone, which is the scrub clause of §7.19.
#[tokio::test]
async fn a_deletion_crash_inside_the_swap_leaves_no_body_behind() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-swap-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let target = root.join(format!("records/{record_id}.md"));
    assert!(std::fs::read_to_string(&target).unwrap().contains(TEXT));

    provider.inject_file_fault(false);
    let lost = wire(
        &provider
            .delete_record(&admin(), delete_request("k-swap-delete", &record_id, 1))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");
    let receipt_id = receipt_id_of_key(&env, "k-swap-delete").await;

    // The crash instant: the body was displaced to its scratch sibling, the
    // tombstone stayed in the staged file, and the target is gone.
    let scratch = root.join(format!("records/.{record_id}.md.{receipt_id}.prior"));
    std::fs::rename(&target, &scratch).unwrap();
    assert!(!target.exists());

    drop(provider);
    let provider = open_provider(&env).await;

    let published = std::fs::read_to_string(&target).unwrap();
    assert!(published.contains("tombstoned: true"), "{published}");
    assert!(!published.contains(TEXT), "{published}");
    assert_eq!(
        files_under(&root),
        vec![format!("records/{record_id}.md")],
        "neither the staged file nor the deleted body survives the settlement"
    );
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert!(served["record"]["tombstone"].is_object(), "{served}");
}

// ── F1: a failed cleanup leaves the intent unsettled, never applied ──────

/// The state an install that linked the tombstone but could not unlink its
/// verified predecessor leaves: the new projection at the target, the deleted
/// body at the receipt's scratch name, and the intent still pending. Ignoring
/// that failure marked the intent applied, and no later pass looked at the
/// hidden file again. The next pass must finish the cleanup and settle the
/// intent without leaving the body behind.
#[tokio::test]
async fn a_failed_scratch_cleanup_is_settled_by_the_next_pass() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-cleanup-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let target = root.join(format!("records/{record_id}.md"));

    provider.inject_file_fault(false);
    let lost = wire(
        &provider
            .delete_record(&admin(), delete_request("k-cleanup-delete", &record_id, 1))
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");
    let receipt_id = receipt_id_of_key(&env, "k-cleanup-delete").await;

    // The crash instant: the body is still at the target, the tombstone in the
    // staged file. The failed install linked the tombstone at the target and
    // kept the body at its scratch name; the intent is still pending.
    let staged_path = root.join(format!("records/.{record_id}.md.{receipt_id}.staged"));
    let scratch = root.join(format!("records/.{record_id}.md.{receipt_id}.prior"));
    std::fs::hard_link(&target, &scratch).unwrap();
    std::fs::remove_file(&target).unwrap();
    std::fs::hard_link(&staged_path, &target).unwrap();
    std::fs::remove_file(&staged_path).unwrap();

    drop(provider);
    let provider = open_provider(&env).await;

    let published = std::fs::read_to_string(&target).unwrap();
    assert!(published.contains("tombstoned: true"), "{published}");
    assert!(!published.contains(TEXT), "{published}");
    assert_eq!(
        files_under(&root),
        vec![format!("records/{record_id}.md")],
        "the retried cleanup removes the deleted body and the stale staged copy"
    );
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert!(served["record"]["tombstone"].is_object(), "{served}");
}

// ── NF3: settlement never precedes authorization or the dedup probe ──────

/// Predecessor settlement is not admission. A caller without `records_write`
/// gets the same refusal it gets for a clear target — never another intent's
/// receipt id and state, and never a settlement write it did not authorize.
#[tokio::test]
async fn an_unauthorized_update_never_learns_of_the_unsettled_predecessor() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let blocked = wire(
        &provider
            .create_record(&admin(), record_create_request("k-authz-a", TEXT))
            .await,
    );
    let blocked_id = record_id_of(receipt(&blocked));
    let clear = wire(
        &provider
            .create_record(&admin(), record_create_request("k-authz-b", TEXT))
            .await,
    );
    let clear_id = record_id_of(receipt(&clear));

    // An intent on A's own target that can never settle: the target holds bytes
    // no intent produced.
    provider.inject_commit_fault(true);
    let blocked_commit = wire(
        &provider
            .update_record(
                &admin(),
                update_request("k-authz-blocker", &blocked_id, 1, UPDATED),
            )
            .await,
    );
    assert_eq!(error_code(&blocked_commit), "OUTCOME_UNCERTAIN");
    let predecessor = receipt_id_of_key(&env, "k-authz-blocker").await;
    let target = root.join(format!("records/{blocked_id}.md"));
    let foreign = "a writer this provider does not own";
    std::fs::write(&target, foreign).unwrap();

    let refused_blocked = wire(
        &provider
            .update_record(
                &stranger(),
                update_request("k-authz-stranger-a", &blocked_id, 1, UPDATED),
            )
            .await,
    );
    let refused_clear = wire(
        &provider
            .update_record(
                &stranger(),
                update_request("k-authz-stranger-b", &clear_id, 1, UPDATED),
            )
            .await,
    );
    assert_eq!(
        refused_blocked, refused_clear,
        "the refusal cannot depend on another operation's unsettled intent"
    );
    assert_eq!(error_code(&refused_blocked), "NOT_AUTHORIZED");
    assert!(
        !refused_blocked.to_string().contains(&predecessor),
        "the refusal leaks no predecessor receipt: {refused_blocked}"
    );
    assert_eq!(
        journal_states(&env).await,
        vec!["applied", "applied", "pending"],
        "an unauthorized caller settled nothing"
    );
    assert_eq!(std::fs::read_to_string(&target).unwrap(), foreign);
}

/// An authorized same-key replay answers its own recorded receipt, even when a
/// later intent owns the same target and cannot be settled.
#[tokio::test]
async fn a_replay_reaches_its_own_receipt_past_a_later_unsettled_intent() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-replay-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let committed = wire(
        &provider
            .update_record(&admin(), update_request("k-replay", &record_id, 1, UPDATED))
            .await,
    );
    let committed = receipt(&committed).clone();
    let receipt_id = committed["receipt_id"]
        .as_str()
        .expect("a receipt id")
        .to_string();
    assert_eq!(committed["result"]["revision"], 2);

    // A later intent owns the same target and cannot settle there.
    provider.inject_commit_fault(true);
    let blocker = wire(
        &provider
            .update_record(&admin(), update_request("k-later", &record_id, 2, THIRD))
            .await,
    );
    assert_eq!(error_code(&blocker), "OUTCOME_UNCERTAIN");
    let target = root.join(format!("records/{record_id}.md"));
    std::fs::write(&target, "a writer this provider does not own").unwrap();

    let replayed = wire(
        &provider
            .update_record(&admin(), update_request("k-replay", &record_id, 1, UPDATED))
            .await,
    );
    let replayed = receipt(&replayed).clone();
    assert_eq!(
        replayed["receipt_id"].as_str(),
        Some(receipt_id.as_str()),
        "the replay returns the receipt it recorded: {replayed}"
    );
    assert_eq!(replayed["result"]["revision"], 2);
}

// ── F3: quarantine preserves recovery evidence and terminates ────────────

/// Quarantine is not a dead end. The intent keeps the payload a re-drive
/// republishes, its receipt records the failure terminally instead of answering
/// pending forever, and once the conflict is resolved the next pass publishes the
/// same intent and terminalizes the receipt as committed.
#[tokio::test]
async fn quarantined_intent_keeps_its_payload_and_redrives_once_resolved() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_commit_fault(true);
    wire(
        &provider
            .create_record(&admin(), record_create_request("k-redrive", TEXT))
            .await,
    );
    let record_id = only_record_id(&env).await;
    let target = root.join(format!("records/{record_id}.md"));
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "a writer this provider does not own").unwrap();
    let receipt_id = receipt_id_of_key(&env, "k-redrive").await;

    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(journal_states(&env).await, vec!["quarantined"]);
    assert!(
        journal_targets(&env).await[0].contains(&format!("records/{record_id}.md")),
        "a quarantined intent keeps the projection it still owes"
    );

    // No channel reports a quarantined intent as pending forever.
    let pool = store_pool(&env).await;
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM memory_receipts WHERE receipt_id = ?")
            .bind(&receipt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        status, "failed",
        "the receipt records the failure terminally"
    );
    let (pending,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memory_receipts WHERE status = 'pending'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, 0, "no receipt is left pending on a quarantine");
    drop(pool);

    let polled = wire(
        &provider
            .get_receipt(&admin(), receipts_get_request(&receipt_id))
            .await,
    );
    let polled = read_receipt(&polled);
    assert_eq!(polled["status"], "failed", "{polled}");
    assert_eq!(polled["error"]["code"], "INTERNAL_ERROR");
    assert_eq!(polled["error"]["details"]["retry_class"], "terminal");
    let message = polled["error"]["message"].as_str().unwrap();
    assert!(
        message.contains(&receipt_id)
            && message.contains(SUBJECT)
            && message.contains("quarantined"),
        "the recorded failure names the receipt, the subject and the quarantine: {message}"
    );
    assert!(
        polled.get("result").is_none(),
        "the failed shape carries the failure, not a result: {polled}"
    );

    let view = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(error_code(&view), "INTERNAL_ERROR");
    assert_eq!(retry_class(&view), "terminal");
    // Repeating the key cannot settle it; the journal re-drives without us.
    let replay = wire(
        &provider
            .create_record(&admin(), record_create_request("k-redrive", TEXT))
            .await,
    );
    assert_eq!(error_code(&replay), "INTERNAL_ERROR");
    assert_eq!(retry_class(&replay), "terminal");

    // The operator resolves the conflict; the next pass republishes the intent.
    std::fs::remove_file(&target).unwrap();
    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(journal_states(&env).await, vec!["applied"]);
    assert!(
        journal_targets(&env)
            .await
            .iter()
            .all(|targets| targets == "[]"),
        "the re-driven intent scrubs its payload once published"
    );
    assert!(std::fs::read_to_string(&target).unwrap().contains(TEXT));
    let committed = wire(
        &provider
            .get_receipt(&admin(), receipts_get_request(&receipt_id))
            .await,
    );
    let committed = read_receipt(&committed);
    assert_eq!(committed["status"], "committed", "{committed}");
    assert_eq!(committed["result"]["record_id"], record_id);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["content"]["text"], TEXT);
}

// ── F4: terminal states outrank a pending one for the same subject ───────

/// A quarantined subject that later acquires another pending intent still reads
/// terminally: the terminal diagnosis is never masked into silent absence.
#[tokio::test]
async fn terminal_state_outranks_pending_for_the_same_subject() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    provider.inject_commit_fault(true);
    wire(
        &provider
            .create_record(&admin(), record_create_request("k-mixed-a", TEXT))
            .await,
    );
    let quarantined_record = only_record_id(&env).await;
    let target = root.join(format!("records/{quarantined_record}.md"));
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "someone else's file").unwrap();
    drop(provider);
    let provider = open_provider(&env).await;
    assert_eq!(journal_states(&env).await, vec!["quarantined"]);

    // Same subject, a second intent that stays pending (its file never lands).
    provider.inject_commit_fault(true);
    wire(
        &provider
            .create_record(&admin(), record_create_request("k-mixed-b", TEXT))
            .await,
    );
    assert_eq!(journal_states(&env).await, vec!["quarantined", "pending"]);

    for body in [
        wire(
            &provider
                .get_record(&admin(), record_get_request(&quarantined_record))
                .await,
        ),
        wire(
            &provider
                .list_records(&admin(), records_list_request())
                .await,
        ),
        wire(&provider.recall(&admin(), recall_request(10)).await),
    ] {
        assert_eq!(error_code(&body), "INTERNAL_ERROR", "{body}");
        assert_eq!(retry_class(&body), "terminal");
    }
}

// ── §7.21: the promotion crash window ───────────────────────────────────

/// A committed placement change is projected on recovery: the target still holds
/// the pre-promotion frame until then, and after it the file and the served row
/// agree on the new placement.
#[tokio::test]
async fn promote_crash_publishes_the_new_placement() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    ensure(&provider).await;
    let root = provider.file_root();

    let created = wire(
        &provider
            .create_record(&admin(), record_create_request("k-promote-create", TEXT))
            .await,
    );
    let record_id = record_id_of(receipt(&created));
    let target = root.join(format!("records/{record_id}.md"));
    let before = std::fs::read_to_string(&target).unwrap();
    assert!(before.contains("visibility: shared"), "{before}");

    provider.inject_file_fault(false);
    let lost = wire(
        &provider
            .promote_record(
                &admin(),
                dto(json!({
                    "space_id": SPACE,
                    "record_id": record_id,
                    "expected_revision": 1,
                    "target_scope": { "visibility": "context", "context_id": "bnd_north" },
                    "idempotency_key": "k-promote-crash",
                })),
            )
            .await,
    );
    assert_eq!(error_code(&lost), "OUTCOME_UNCERTAIN");
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        before,
        "the placement is only in the row until recovery publishes it"
    );

    drop(provider);
    let provider = open_provider(&env).await;

    let published = std::fs::read_to_string(&target).unwrap();
    assert!(published.contains("visibility: context"), "{published}");
    assert!(published.contains("context_id: bnd_north"), "{published}");
    assert!(published.contains(TEXT), "{published}");
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
    let served = wire(
        &provider
            .get_record(&admin(), record_get_request(&record_id))
            .await,
    );
    assert_eq!(served["record"]["scope"]["visibility"], "context");
    assert_eq!(served["record"]["scope"]["context_id"], "bnd_north");
    assert_eq!(served["record"]["revision"], 2);
}

// ── §5.2 step 2: one publication gate per workspace ─────────────────────

/// The publication gate is keyed by workspace, not by provider instance: a second
/// provider opened on the same workspace waits for the first one's
/// settle-to-publish window instead of publishing concurrently.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_provider_instances_on_one_workspace_serialize_publication() {
    let env = seed_env();
    let first = Arc::new(open_provider(&env).await);
    ensure(&first).await;
    let second = Arc::new(open_provider(&env).await);

    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Barrier::new(2));
    first.park_next_publish(Arc::clone(&entered), Arc::clone(&release));
    let parked = tokio::spawn({
        let provider = Arc::clone(&first);
        async move {
            wire(
                &provider
                    .create_record(&admin(), record_create_request("k-gate-a", TEXT))
                    .await,
            )
        }
    });
    // Wait until that publication actually holds the workspace gate before the
    // second instance asks for it.
    tokio::time::timeout(std::time::Duration::from_secs(10), entered.notified())
        .await
        .expect("the first instance's publication holds the workspace gate");
    let mut queued = tokio::spawn({
        let provider = Arc::clone(&second);
        async move {
            wire(
                &provider
                    .create_record(&admin(), record_create_request("k-gate-b", TEXT))
                    .await,
            )
        }
    });

    // The other instance cannot even reach its own publication while the first
    // holds the workspace gate.
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), &mut queued)
            .await
            .is_err(),
        "a second provider instance must wait for the workspace publication gate"
    );
    tokio::time::timeout(std::time::Duration::from_secs(10), release.wait())
        .await
        .expect("the parked publication is released");

    let parked = receipt(&parked.await.unwrap()).clone();
    let queued = receipt(&queued.await.unwrap()).clone();
    let root = first.file_root();
    for body in [&parked, &queued] {
        let record_id = record_id_of(body);
        assert!(
            root.join(format!("records/{record_id}.md")).exists(),
            "both publications landed: {record_id}"
        );
    }
    assert_eq!(journal_states(&env).await, vec!["applied", "applied"]);
}

// ── C-117: the journal migration ────────────────────────────────────────

/// The journal migration is registered with the writer protocol, applies once,
/// and the `DB_SCHEMA_VERSION` line advanced with it.
#[tokio::test]
async fn journal_migration_is_registered_and_idempotent() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let pool = nexus_local_db::open_pool(&env.db_path).await.unwrap();

    let (applied,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE description LIKE '%memory_journal%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(applied, 1, "the journal migration applies exactly once");

    let (found,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
            .bind("memory_journal")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(found, 1, "memory_journal exists");

    for (kind, verb) in [
        ("guard", "insert"),
        ("guard", "update"),
        ("guard", "delete"),
        ("outbox", "insert"),
        ("outbox", "update"),
        ("outbox", "delete"),
    ] {
        let name = format!("{kind}_memory_journal_{verb}");
        let (found,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name=?")
                .bind(&name)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(found, 1, "missing writer-protocol trigger: {name}");
    }

    let versions = nexus_local_db::read_versions(&pool).await.unwrap();
    assert_eq!(
        versions.db_schema_version,
        nexus_local_db::DB_SCHEMA_VERSION,
        "the seeded version line is current"
    );

    drop(pool);
    drop(provider);
    let reopened = open_provider(&env).await;
    let spaces = wire(&reopened.list_spaces(&admin(), dto(json!({}))).await);
    assert_eq!(
        spaces["spaces"].as_array().unwrap().len(),
        1,
        "re-running the migration set neither duplicates nor drops the space"
    );
}

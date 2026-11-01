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
    let rows: Vec<(String,)> = sqlx::query_as("SELECT targets_json FROM memory_journal")
        .fetch_all(&pool)
        .await
        .unwrap();
    rows.into_iter().map(|(targets,)| targets).collect()
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
/// staging (its version is not on disk yet), so B's publish cannot be applied
/// first; recovery settles A and then B, leaving the row's newest version at
/// the target.
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
    assert_eq!(error_code(&second), "OUTCOME_UNCERTAIN");

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

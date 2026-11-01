//! `LocalMemoryProvider` (NXM-1 round A) protocol contract tests.
//!
//! Round A lands the provider skeleton, the additive control-metadata
//! migration, and the `memory.describe` / `memory.spaces.*` /
//! `memory.subjects.*` / `memory.grants.*` families. Round B
//! (capture/candidates/records/recall/profile) and the sub-capability gated
//! families (reflection, transfer) must answer a typed `op_unsupported`
//! refusal — never a silent success.
//!
//! Every scenario runs on a temp nexus home. Import-boundary note: the shared
//! `MemoryPort` vocabulary comes from `spoke_operations`, whose generated wire
//! types are reachable through its `spoke_schemas` re-export, so this crate
//! keeps exactly one spoke dependency edge.

use std::path::{Path, PathBuf};

use nexus_core::memory_provider::LocalMemoryProvider;
use nexus_creator_memory::bearer::MemoryBearerRef;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{MemoryPort, SpokeResult};
use sqlx::Connection as _;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SPACE: &str = "spc_local_default";
const SUBJECT: &str = "sbj_mira";

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

/// Project any response onto its wire JSON: assertions then read the
/// protocol shape (the memory family carries its errors inside the response
/// envelope), not Rust variant names.
fn wire<T: serde::Serialize>(result: &SpokeResult<T>) -> Value {
    match result {
        SpokeResult::Ok(value) => serde_json::to_value(value).expect("serializable response"),
        SpokeResult::Reject(reject) => panic!("expected a response envelope, got {reject:?}"),
    }
}

fn error_code(body: &Value) -> &str {
    body["error"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("expected a memory error envelope, got {body}"))
}

fn retry_class(body: &Value) -> &str {
    body["error"]["details"]["retry_class"]
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

fn fence_request(
    action: &str,
    target: Value,
    revision: u64,
    key: &str,
) -> schemas::MemorySubjectsFenceRequest {
    dto(json!({
        "space_id": SPACE,
        "subject_id": SUBJECT,
        "target": target,
        "action": action,
        "expected_revision": revision,
        "idempotency_key": key,
    }))
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
    let freeze = |key: &str, revision: u64| -> schemas::MemorySpacesFreezeRequest {
        dto(json!({ "space_id": SPACE, "expected_revision": revision, "idempotency_key": key }))
    };

    let first = wire(&provider.freeze_space(&admin, freeze("k-freeze", 1)).await);
    let first = receipt(&first);
    assert_eq!(first["op"], "memory.spaces.freeze");
    assert_eq!(first["result"]["status"], "frozen");
    assert_eq!(first["result"]["revision"], 2);
    assert_eq!(first["result"]["pending_receipts"], 0);
    assert_eq!(first["requester_principal_id"], CREATOR);
    let receipt_id = first["receipt_id"].as_str().unwrap().to_owned();

    let replay = wire(&provider.freeze_space(&admin, freeze("k-freeze", 1)).await);
    assert_eq!(
        receipt(&replay)["receipt_id"],
        receipt_id,
        "the same key + payload replays the stored receipt (§7.0.5)"
    );

    let conflicting = wire(&provider.freeze_space(&admin, freeze("k-freeze", 2)).await);
    assert_eq!(error_code(&conflicting), "IDEMPOTENCY_CONFLICT");
    assert_eq!(retry_class(&conflicting), "terminal");

    let stale = wire(&provider.freeze_space(&admin, freeze("k-stale", 1)).await);
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
                fence_request(
                    "archive",
                    json!({ "type": "context", "context_id": "bnd_1" }),
                    1,
                    "k-arch-ctx",
                ),
            )
            .await,
    );
    assert_eq!(error_code(&archived_on_context), "INVALID_INPUT");

    let fenced = wire(
        &provider
            .fence_subject(
                &admin,
                fence_request("fence", json!({ "type": "subject" }), 1, "k-fence"),
            )
            .await,
    );
    assert_eq!(receipt(&fenced)["result"]["status"], "fenced");
    assert_eq!(receipt(&fenced)["result"]["revision"], 2);

    let unfenced = wire(
        &provider
            .fence_subject(
                &admin,
                fence_request("unfence", json!({ "type": "subject" }), 2, "k-unfence"),
            )
            .await,
    );
    assert_eq!(receipt(&unfenced)["result"]["status"], "active");
    assert_eq!(receipt(&unfenced)["result"]["revision"], 3);

    let context_fenced = wire(
        &provider
            .fence_subject(
                &admin,
                fence_request(
                    "fence",
                    json!({ "type": "context", "context_id": "bnd_1" }),
                    3,
                    "k-fence-ctx",
                ),
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
            .fence_subject(
                &admin,
                fence_request("archive", json!({ "type": "subject" }), 4, "k-archive"),
            )
            .await,
    );
    assert_eq!(receipt(&archived)["result"]["status"], "archived");

    let resurrect = wire(
        &provider
            .fence_subject(
                &admin,
                fence_request("unfence", json!({ "type": "subject" }), 5, "k-unarchive"),
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

    let granted = wire(
        &provider
            .grant(&admin, grant_request("k-grant-1", "capture", OTHER))
            .await,
    );
    let granted = receipt(&granted);
    let grant_id = granted["result"]["grant_id"].as_str().unwrap().to_owned();
    assert_eq!(granted["result"]["revision"], 1);

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
            .ensure_subject(
                &grantee,
                dto(json!({ "space_id": SPACE, "subject_id": "sbj_after_revoke" })),
            )
            .await,
    );
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED");
}

/// Round B's families and the sub-capability gated families answer a typed
/// `op_unsupported` refusal (`retry_class: terminal`) instead of any success.
#[tokio::test]
async fn gated_families_refuse_with_op_unsupported() {
    let env = seed_env();
    let provider = open_provider(&env).await;
    let p = principal(CREATOR);

    let capture = wire(
        &provider
            .capture(
                &p,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "content": { "text": "Mira promised to return before dawn." },
                    "capture_scope": { "visibility": "shared" },
                    "idempotency_key": "k-capture",
                })),
            )
            .await,
    );
    assert_eq!(
        error_code(&capture),
        "op_unsupported",
        "round B owns capture"
    );
    assert_eq!(retry_class(&capture), "terminal");

    let recall = wire(
        &provider
            .recall(
                &p,
                dto(json!({
                    "space_id": SPACE,
                    "subject_id": SUBJECT,
                    "selection": { "mode": "subject_all" },
                    "budget": { "max_items": 10 },
                })),
            )
            .await,
    );
    assert_eq!(error_code(&recall), "op_unsupported", "round B owns recall");

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
    assert_eq!(
        error_code(&reflections),
        "op_unsupported",
        "reflection is a gated sub-capability"
    );

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
    assert_eq!(
        error_code(&export),
        "op_unsupported",
        "transfer is a gated sub-capability"
    );
}

/// C-117: the control-metadata migration lands once, re-runs as a no-op, is
/// writer-guard registered (guards + outbox on every control table), and the
/// `DB_SCHEMA_VERSION` line advanced with it.
#[tokio::test]
async fn control_metadata_migration_is_idempotent_and_writer_registered() {
    let env = seed_env();

    let provider = open_provider(&env).await;
    let pool = nexus_local_db::open_pool(&env.db_path).await.unwrap();

    let (applied,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE description LIKE '%memory_control_metadata%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(applied, 1, "the migration applies exactly once");

    for table in [
        "memory_spaces",
        "memory_subjects",
        "memory_grants",
        "memory_receipts",
    ] {
        let (found,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
                .bind(table)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(found, 1, "missing control-metadata table: {table}");
    }

    for table in [
        "memory_spaces",
        "memory_subjects",
        "memory_grants",
        "memory_receipts",
    ] {
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
    assert!(
        nexus_local_db::DB_SCHEMA_VERSION >= 26,
        "the control-metadata migration bumps DB_SCHEMA_VERSION (C-117)"
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

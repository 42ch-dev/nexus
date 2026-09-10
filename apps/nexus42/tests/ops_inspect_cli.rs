//! `nexus42 ops inspect` CLI contract tests (V1.182 P1 BL-04, Task 2).
//!
//! Hermetic daemon-free scenarios against the APPROVED inspect contract
//! (`.mstar/sdd/2026-09-03-v1.182-p1-bl04-checkpoint-resume-ux/inspect-contract.md`):
//!
//! - hidden `ops` group; `inspect [SESSION_ID] [--json]` surface;
//! - list mode over non-terminal rows + honest empty/absent-db states;
//! - detail mode `--json` shape field-by-field (verdict/caveat split);
//! - verdict projection: `chain_class_no_failure` / `typed_failure` /
//!   `not_converge_merge_class` / `context_unreadable`;
//! - unknown id → "No checkpointed session <id>." exit 1;
//! - read-only proof: rows and db file bytes unchanged after an inspect run.
//!
//! Run with: `cargo test -p nexus42 --test ops_inspect_cli`

use assert_cmd::Command;
use nexus_orchestration::run_state::{PresetSourceIdentity, RunDescriptorV1};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};

const CREATOR: &str = "ctr_inspect";

fn nexus42(home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("nexus42").expect("nexus42 binary");
    cmd.env("HOME", home);
    cmd
}

/// Seed a hermetic HOME with `active_creator_id` in config.toml. Returns the
/// workspace `state.db` path (not yet created).
fn seed_home_config(home: &Path) -> PathBuf {
    let nexus_dir = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).expect("create .nexus42");
    std::fs::write(
        nexus_dir.join("config.toml"),
        format!("active_creator_id = \"{CREATOR}\"\n"),
    )
    .expect("write config.toml");
    nexus_home_layout::workspace_state_db_path(home, CREATOR, "default")
}

/// Create the workspace db (migrations applied) at `db_path`.
async fn create_db(db_path: &Path) -> sqlx::SqlitePool {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).expect("create workspace dir");
    }
    let pool = nexus_local_db::open_pool(db_path).await.expect("open pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("migrations");
    pool
}

struct SeedRow<'a> {
    session_id: &'a str,
    preset_id: &'a str,
    status: &'a str,
    current_task_id: Option<&'a str>,
    context: &'a [u8],
    /// Persisted `updated_at` (unix seconds); the list view orders by this.
    updated_at: i64,
    /// Execution version (`0` legacy v0 row; `>=1` v1 authoritative row).
    execution_version: i64,
    /// State revision (`0` on v0 rows).
    state_revision: i64,
    /// Serialized `RunStateV1` blob for v1 rows (`None` on v0 rows).
    run_state_json: Option<Vec<u8>>,
}

impl<'a> SeedRow<'a> {
    const fn new(
        session_id: &'a str,
        preset_id: &'a str,
        status: &'a str,
        current_task_id: Option<&'a str>,
        context: &'a [u8],
    ) -> Self {
        Self {
            session_id,
            preset_id,
            status,
            current_task_id,
            context,
            updated_at: 1_756_990_300,
            execution_version: 0,
            state_revision: 0,
            run_state_json: None,
        }
    }
}

async fn seed_session(pool: &sqlx::SqlitePool, row: &SeedRow<'_>) {
    let descriptor = (row.execution_version == 1).then(|| run_descriptor(row.preset_id));
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, ?, ?, 3, ?, ?, ?, 1_756_990_000, ?, ?, ?, ?, ?)",
    )
    .bind(row.session_id)
    .bind(CREATOR)
    .bind(row.preset_id)
    .bind(row.status)
    .bind(row.current_task_id)
    .bind(row.context)
    .bind(row.updated_at)
    .bind(row.execution_version)
    .bind(row.state_revision)
    .bind(row.run_state_json.clone())
    .bind(descriptor)
    .execute(pool)
    .await
    .expect("seed session");
}

/// Serialized durable v1 run state (A2 `RunStateV1`), used by the v1 seeds.
fn run_state(step_in_flight: Option<&str>, cancel_requested: bool, wait: bool) -> Vec<u8> {
    serde_json::json!({
        "wait": wait.then(|| serde_json::json!({
            "wait_id": "wait-tok-1",
            "task_id": "task_7",
            "child_session_id": null,
            "child_task_id": null,
            "kind": "manual",
        })),
        "step_in_flight": step_in_flight,
        "in_flight": null,
        "failure": null,
        "cancel_requested": cancel_requested,
    })
    .to_string()
    .into_bytes()
}

fn run_descriptor(preset_id: &str) -> Vec<u8> {
    serde_json::to_vec(&RunDescriptorV1 {
        creator_id: CREATOR.to_string(),
        work_id: None,
        workspace_root: PathBuf::from("/tmp/nexus42-inspect"),
        preset_id: preset_id.to_string(),
        preset_version: 3,
        source: PresetSourceIdentity::Embedded {
            preset_id: preset_id.to_string(),
            content_hash: [0; 32],
        },
        input: serde_json::Map::new(),
        agent_bindings: HashMap::new(),
        parent_session_id: None,
        graph_name: None,
    })
    .expect("serialize descriptor")
}

fn run_async<F: Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt")
        .block_on(fut)
}

/// Chain-class context: live converge arrivals + join wait keys.
fn chain_context() -> Vec<u8> {
    json!({"data": {
        "_converge_arrivals_j1": ["src-a"],
        "_join_wait_start_j1": 1_756_990_100,
        "_gate_park_task_9": true
    }})
    .to_string()
    .into_bytes()
}

/// Typed-failure context (also carries a dead join key).
fn failed_context() -> Vec<u8> {
    json!({"data": {
        "_run_status": "failed",
        "_run_error": "join deadline exceeded",
        "_converge_arrivals_j1": null
    }})
    .to_string()
    .into_bytes()
}

/// tri-QC P1-C: a human wait whose frozen source no longer verifies is
/// preserved but must not advertise `continue`; the typed reason is exposed.
#[test]
fn inspect_v1_human_wait_with_unreconstructable_source_degrades_actions() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let plain = json!({"data": {"_creator_id": CREATOR}})
        .to_string()
        .into_bytes();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_wait_bad_source",
                preset_id: "preset_x",
                status: "waiting_for_input",
                current_task_id: Some("task_7"),
                context: &plain,
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 4,
                run_state_json: Some(run_state(None, false, true)),
            },
        )
        .await;
        // Valid-shape descriptor whose frozen version no longer matches.
        let caps = nexus_orchestration::capability::CapabilityRegistry::with_builtins();
        let loaded = nexus_orchestration::preset::load_embedded_preset("memory-augmented", &caps)
            .expect("embedded preset");
        let descriptor = serde_json::json!({
            "creator_id": CREATOR,
            "work_id": null,
            "workspace_root": "/tmp",
            "preset_id": "memory-augmented",
            "preset_version": loaded.version + 1,
            "source": loaded.source_identity.clone().expect("source identity"),
            "input": {},
            "agent_bindings": {},
            "parent_session_id": null,
            "graph_name": null
        });
        sqlx::query(
            "UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?",
        )
        .bind(serde_json::to_vec(&descriptor).expect("descriptor json"))
        .bind("ses_wait_bad_source")
        .execute(&pool)
        .await
        .expect("seed mismatched descriptor");
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_wait_bad_source", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["recovery_class"], json!("human_wait"));
    assert_eq!(parsed["wait_id"], json!("wait-tok-1"));
    assert_eq!(
        parsed["allowed_actions"],
        json!(["cancel", "new_run"]),
        "an unreconstructable source must not offer continue"
    );
    assert_eq!(parsed["reason_code"], json!("reconstruction_unavailable"));
}

#[test]
fn ops_group_is_hidden_from_root_help() {
    let output = nexus42(tempfile::TempDir::new().unwrap().path())
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output).unwrap();
    let commands_section = help.split("Commands:").nth(1).expect("Commands: section");
    assert!(
        !commands_section.contains("\n  ops"),
        "top-level 'ops' must be hidden from the Commands list:\n{commands_section}"
    );
}

#[test]
fn inspect_list_empty_store_is_honest() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        pool.close().await;
    });

    nexus42(home.path())
        .args(["ops", "inspect"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No checkpointed sessions."));
}

#[test]
fn inspect_list_json_empty_store_is_envelope_with_zero_total() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(
        parsed,
        json!({"db_present": true, "total": 0, "rows": []}),
        "empty store must be distinct from absent db"
    );
}

#[test]
fn inspect_absent_db_is_honest_not_an_error() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    assert!(!db_path.exists());

    nexus42(home.path())
        .args(["ops", "inspect"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No workspace state database at"))
        .stdout(predicates::str::contains("no checkpointed sessions"));
}

#[test]
fn inspect_no_active_creator_is_config_error() {
    let home = tempfile::TempDir::new().unwrap();
    let nexus_dir = home.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).unwrap();
    std::fs::write(nexus_dir.join("config.toml"), "").unwrap();

    let output = nexus42(home.path())
        .args(["ops", "inspect"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.contains("nexus42 creator use") || stderr.contains("init workspace"),
        "config error must point at init/use: {stderr}"
    );
}

#[test]
fn inspect_detail_json_matches_contract_field_by_field() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let ctx = chain_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_chain", "preset_chain", "running", Some("task_9"), &ctx),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_chain", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let obj = parsed.as_object().expect("top-level object");

    assert_eq!(obj["session_id"], json!("ses_chain"));
    assert_eq!(obj["creator_id"], json!(CREATOR));
    assert_eq!(obj["preset_id"], json!("preset_chain"));
    assert_eq!(obj["preset_version"], json!(3));
    assert_eq!(obj["db_status"], json!("running"));
    assert_eq!(obj["current_task_id"], json!("task_9"));
    assert_eq!(obj["created_at"], json!(1_756_990_000));
    assert_eq!(obj["updated_at"], json!(1_756_990_300));
    assert_eq!(obj["run_failure"], Value::Null);
    assert_eq!(
        obj["live_join_keys"],
        json!(["_converge_arrivals_j1", "_join_wait_start_j1"])
    );
    assert_eq!(
        obj["resumable"],
        json!({
            "verdict": "yes",
            "rule": "chain_class_no_failure",
            "runner_check": "boot_time",
            "explanation": obj["resumable"]["explanation"].clone(),
        }),
        "resumable verdict/caveat split must match contract"
    );
    let explanation = obj["resumable"]["explanation"].as_str().unwrap();
    assert!(
        explanation.contains("boot"),
        "verdict:yes explanation must state the boot-time runner caveat: {explanation}"
    );
    assert!(
        !obj.contains_key("context_readable"),
        "context_readable must be absent on readable rows: {parsed}"
    );
}

#[test]
fn inspect_detail_typed_failure_is_not_resumable() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let ctx = failed_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_failed", "preset_chain", "running", None, &ctx),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_failed", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["current_task_id"], Value::Null);
    assert_eq!(
        obj["run_failure"],
        json!({"run_status": "failed", "run_error": "join deadline exceeded"}),
        "run_failure must be DTO-verbatim"
    );
    assert_eq!(obj["live_join_keys"], json!([]));
    assert_eq!(obj["resumable"]["verdict"], json!("no"));
    assert_eq!(obj["resumable"]["rule"], json!("typed_failure"));
    assert_eq!(obj["resumable"]["runner_check"], json!("not_applicable"));

    // Human view: honest negative position + typed failure record.
    let human = nexus42(home.path())
        .args(["ops", "inspect", "ses_failed"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains("session:        ses_failed"), "{human}");
    assert!(human.contains("position:       (none recorded)"), "{human}");
    assert!(human.contains("failed: join deadline exceeded"), "{human}");
    assert!(human.contains("resumable:      no"), "{human}");
}

#[test]
fn inspect_detail_non_chain_class_is_not_resumable() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let ctx = json!({"data": {"_creator_id": CREATOR}})
        .to_string()
        .into_bytes();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_plain", "preset_plain", "paused", Some("task_1"), &ctx),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_plain", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["db_status"], json!("paused"));
    assert_eq!(parsed["resumable"]["verdict"], json!("no"));
    assert_eq!(
        parsed["resumable"]["rule"],
        json!("not_converge_merge_class")
    );
    assert_eq!(parsed["resumable"]["runner_check"], json!("not_applicable"));
}

#[test]
fn inspect_detail_corrupt_context_is_unknown_never_fabricated() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new(
                "ses_corrupt",
                "preset_x",
                "running",
                None,
                b"not-json-at-all",
            ),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_corrupt", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let obj = parsed.as_object().unwrap();

    assert_eq!(obj["context_readable"], json!(false));
    assert_eq!(obj["run_failure"], Value::Null);
    assert_eq!(obj["live_join_keys"], json!([]));
    assert_eq!(obj["resumable"]["verdict"], json!("unknown"));
    assert_eq!(obj["resumable"]["rule"], json!("context_unreadable"));
    assert_eq!(obj["resumable"]["runner_check"], json!("not_applicable"));

    // Human view: unknown wording, no fabricated verdict.
    let human = nexus42(home.path())
        .args(["ops", "inspect", "ses_corrupt"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains("resumable:      unknown"), "{human}");
    assert!(human.contains("context unreadable"), "{human}");
}

#[test]
fn inspect_detail_unknown_id_is_honest_not_found() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_missing"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("No checkpointed session ses_missing."),
        "unknown id must be an honest not-found: {combined}"
    );
    assert_eq!(output.status.code(), Some(1));

    // --json error: {"error": ...} on stdout, exit 1.
    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_missing", "--json"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("json error on stdout");
    assert_eq!(
        parsed["error"],
        json!("No checkpointed session ses_missing.")
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn inspect_list_mode_renders_rows_and_count() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let chain = chain_context();
    let failed = failed_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new(
                "ses_chain",
                "preset_chain",
                "running",
                Some("task_9"),
                &chain,
            ),
        )
        .await;
        seed_session(
            &pool,
            &SeedRow::new("ses_failed", "preset_chain", "running", None, &failed),
        )
        .await;
        // Terminal row must be filtered out of list mode.
        seed_session(
            &pool,
            &SeedRow::new(
                "ses_done",
                "preset_chain",
                "completed",
                Some("task_done"),
                &chain,
            ),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("ses_chain"), "{text}");
    assert!(text.contains("ses_failed"), "{text}");
    assert!(!text.contains("ses_done"), "terminal rows excluded: {text}");
    assert!(text.contains('2'), "trailing count line: {text}");

    // JSON list: envelope with the same DTOs, terminal row excluded,
    // honest total.
    let output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["db_present"], json!(true));
    assert_eq!(parsed["total"], json!(2));
    let rows = parsed["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 2);
    let ids: Vec<&str> = rows
        .iter()
        .map(|r| r["session_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"ses_chain"));
    assert!(ids.contains(&"ses_failed"));
    let chain_row = rows
        .iter()
        .find(|r| r["session_id"] == "ses_chain")
        .unwrap();
    assert_eq!(chain_row["resumable"]["verdict"], json!("yes"));
    assert_eq!(chain_row["resumable"]["runner_check"], json!("boot_time"));
}

#[test]
fn inspect_run_is_read_only() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let ctx = chain_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_chain", "preset_chain", "running", Some("task_9"), &ctx),
        )
        .await;
        pool.close().await;
    });

    let rows_before = run_async(async {
        let pool = nexus_local_db::open_pool_read_only(&db_path)
            .await
            .expect("ro pool");
        let rows: Vec<(String, Option<String>, Vec<u8>, i64)> = sqlx::query_as(
            "SELECT session_id, current_task_id, context_json, updated_at
             FROM orchestration_sessions ORDER BY session_id",
        )
        .fetch_all(&pool)
        .await
        .expect("snapshot rows");
        pool.close().await;
        rows
    });
    let bytes_before = std::fs::read(&db_path).expect("db bytes before");

    nexus42(home.path())
        .args(["ops", "inspect", "ses_chain", "--json"])
        .assert()
        .success();
    nexus42(home.path())
        .args(["ops", "inspect"])
        .assert()
        .success();

    let rows_after = run_async(async {
        let pool = nexus_local_db::open_pool_read_only(&db_path)
            .await
            .expect("ro pool");
        let rows: Vec<(String, Option<String>, Vec<u8>, i64)> = sqlx::query_as(
            "SELECT session_id, current_task_id, context_json, updated_at
             FROM orchestration_sessions ORDER BY session_id",
        )
        .fetch_all(&pool)
        .await
        .expect("snapshot rows");
        pool.close().await;
        rows
    });
    let bytes_after = std::fs::read(&db_path).expect("db bytes after");

    assert_eq!(rows_before, rows_after, "inspect must not mutate rows");
    assert_eq!(
        bytes_before, bytes_after,
        "inspect must not write the db file"
    );
}

// ---------------------------------------------------------------------------
// QC fix wave (v1.182 P1 BL-04) — behavior pins added with the fixes.
// ---------------------------------------------------------------------------

#[test]
fn inspect_detail_cancelled_with_live_join_keys_is_not_resumable() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let ctx = chain_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        // schedule-cancel writer leaves context untouched (schedules.rs:1226):
        // live join keys + no typed failure persist, only status='cancelled'.
        seed_session(
            &pool,
            &SeedRow::new(
                "ses_cancelled",
                "preset_chain",
                "cancelled",
                Some("task_9"),
                &ctx,
            ),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_cancelled", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let obj = parsed.as_object().unwrap();

    // Rule 1 (daemon recovery filter) projected into detail mode: a terminal
    // status row is never a re-drive candidate, even with live join keys.
    assert_eq!(obj["db_status"], json!("cancelled"));
    assert_eq!(
        obj["live_join_keys"],
        json!(["_converge_arrivals_j1", "_join_wait_start_j1"]),
        "live join keys are preserved on cancelled rows (context untouched)"
    );
    assert_eq!(obj["run_failure"], Value::Null);
    assert_eq!(obj["resumable"]["verdict"], json!("no"));
    assert_eq!(obj["resumable"]["rule"], json!("terminal_status"));
    assert_eq!(obj["resumable"]["runner_check"], json!("not_applicable"));

    // Human view: terminal status reads `no` with the rule-1 wording.
    let human = nexus42(home.path())
        .args(["ops", "inspect", "ses_cancelled"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains("status:         cancelled"), "{human}");
    assert!(human.contains("resumable:      no"), "{human}");
    assert!(
        human.contains("terminal status"),
        "rule-1 wording must name the terminal status: {human}"
    );
    assert!(
        human.contains("join state:     2 live join key(s):"),
        "{human}"
    );
}

#[test]
fn inspect_list_is_bounded_to_latest_200_and_reports_honest_total() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        for i in 0..205i64 {
            seed_session(
                &pool,
                &SeedRow {
                    session_id: &format!("ses_{i:03}"),
                    preset_id: "preset_bulk",
                    status: "running",
                    current_task_id: None,
                    context: &chain_context(),
                    updated_at: 1_756_990_000 + i,
                    execution_version: 0,
                    state_revision: 0,
                    run_state_json: None,
                },
            )
            .await;
        }
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let shown: Vec<&str> = text.lines().filter(|l| l.starts_with("ses_")).collect();
    assert_eq!(shown.len(), 200, "list must be bounded to 200 rows");
    assert_eq!(
        shown[0].split_whitespace().next().unwrap(),
        "ses_204",
        "rows ordered by updated_at DESC (most recent first)"
    );
    assert_eq!(
        shown[199].split_whitespace().next().unwrap(),
        "ses_005",
        "the oldest 5 rows are cut off by the LIMIT"
    );
    assert!(
        text.contains("200 of 205+ checkpointed session(s)."),
        "count line must surface the honest total: {text}"
    );

    // JSON: top-level object with db_present / total / rows (DTO-verbatim).
    let output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["db_present"], json!(true));
    assert_eq!(parsed["total"], json!(205));
    let rows = parsed["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 200, "json list bounded to 200 rows");
    assert_eq!(rows[0]["session_id"], json!("ses_204"));
    assert_eq!(rows[199]["session_id"], json!("ses_005"));
}

#[test]
fn inspect_list_json_missing_db_reports_db_present_false() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    assert!(!db_path.exists());

    let output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["db_present"], json!(false));
    assert_eq!(parsed["total"], json!(0));
    assert_eq!(parsed["rows"], json!([]));
}

#[test]
fn inspect_human_run_error_truncation_is_marked() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let ctx = json!({"data": {
        "_run_status": "failed",
        "_run_error": "line one\nline two\nline three"
    }})
    .to_string()
    .into_bytes();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_multi", "preset_x", "running", None, &ctx),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_multi"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(output).unwrap();
    let record = human
        .lines()
        .find(|l| l.starts_with("run record:"))
        .expect("run record line");
    assert!(
        record.contains("line one … (truncated; see --json)"),
        "human one-line cut must carry the truncation marker, got: {record}"
    );
    assert!(
        !record.contains("line two"),
        "only the first line may appear: {record}"
    );

    // --json stays DTO-verbatim (never truncated).
    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_multi", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(
        parsed["run_failure"]["run_error"],
        json!("line one\nline two\nline three"),
        "json run_error must be verbatim"
    );
}

#[test]
fn inspect_detail_shape_anomaly_wording_is_distinct_from_corrupt() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    // Parseable JSON, but `data` is not an object — schema-shape anomaly,
    // not byte corruption.
    let ctx = b"{\"data\": \"not-an-object\"}";
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_shape", "preset_x", "running", None, ctx),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_shape", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let obj = parsed.as_object().unwrap();
    // Verdict behavior matches corrupt-context (unknown, never fabricated) —
    // but the machine flag differs: valid JSON is byte-readable, so
    // `context_readable: true` with the shape anomaly in the explanation.
    assert_eq!(obj["context_readable"], json!(true));
    assert_eq!(obj["resumable"]["verdict"], json!("unknown"));
    assert_eq!(obj["resumable"]["rule"], json!("context_unreadable"));
    let explanation = obj["resumable"]["explanation"].as_str().unwrap();
    assert!(
        explanation.contains("shape"),
        "shape-anomaly explanation must name the schema shape, got: {explanation}"
    );
    assert!(
        !explanation.contains("corrupt"),
        "shape-anomaly must not be labelled corrupt context_json: {explanation}"
    );

    // Human view must mirror the same wording split (contract §5): the
    // resumable line names the unexpected shape, never "corrupt context_json".
    let human = nexus42(home.path())
        .args(["ops", "inspect", "ses_shape"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    let resumable = human
        .lines()
        .find(|l| l.starts_with("resumable:"))
        .expect("resumable line");
    assert!(
        resumable.contains("unexpected shape"),
        "human resumable line must name the unexpected shape, got: {resumable}"
    );
    assert!(
        !resumable.contains("corrupt"),
        "human resumable line must not claim corrupt context_json: {resumable}"
    );
}
#[test]
fn inspect_list_shape_anomaly_and_corrupt_pin_context_readable_flag() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    // Valid JSON, `data` not an object — byte-readable, shape anomaly.
    let shape = b"{\"data\": \"not-an-object\"}";
    // Corrupt bytes — bytes-level unreadable.
    let corrupt = b"not-json-at-all";
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_shape", "preset_x", "running", None, shape),
        )
        .await;
        seed_session(
            &pool,
            &SeedRow::new("ses_corrupt", "preset_x", "running", None, corrupt),
        )
        .await;
        pool.close().await;
    });
    let output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let rows = parsed["rows"].as_array().expect("rows array");
    let shape_row = rows
        .iter()
        .find(|r| r["session_id"] == "ses_shape")
        .expect("shape row listed");
    assert_eq!(
        shape_row["context_readable"],
        json!(true),
        "valid-JSON unexpected shape is byte-readable in list mode"
    );
    assert_eq!(shape_row["resumable"]["verdict"], json!("unknown"));
    assert_eq!(shape_row["resumable"]["rule"], json!("context_unreadable"));
    let corrupt_row = rows
        .iter()
        .find(|r| r["session_id"] == "ses_corrupt")
        .expect("corrupt row listed");
    assert_eq!(
        corrupt_row["context_readable"],
        json!(false),
        "corrupt bytes stay bytes-unreadable in list mode"
    );
    assert_eq!(corrupt_row["resumable"]["verdict"], json!("unknown"));
    assert_eq!(
        corrupt_row["resumable"]["rule"],
        json!("context_unreadable")
    );
}

// ---------------------------------------------------------------------------
// v1.186 P0 Task 2 — canonical A7 recovery class on the real binary.
// ---------------------------------------------------------------------------
#[test]
fn inspect_v1_interrupted_wins_over_old_join_keys_and_never_retries() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    // v1 running row whose durable state carries an in-flight step mark,
    // plus live converge/merge join keys in context — A7 rule 3: in-flight
    // wins over old join keys; never auto-retried.
    let ctx = chain_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_crash", "preset_chain", "running", Some("task_9"), &ctx),
        )
        .await;
        sqlx::query::<sqlx::Sqlite>(
            "UPDATE orchestration_sessions
                SET execution_version = 1, state_revision = 7, run_state_json = ?,
                    run_descriptor_json = ?
              WHERE session_id = 'ses_crash'",
        )
        .bind(run_state(Some("task_9"), false, false))
        .bind(run_descriptor("preset_chain"))
        .execute(&pool)
        .await
        .expect("promote to v1");
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_crash", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["execution_version"], json!(1));
    assert_eq!(parsed["state_revision"], json!(7));
    assert_eq!(parsed["recovery_class"], json!("interrupted"));
    assert_eq!(
        parsed["live_join_keys"],
        json!(["_converge_arrivals_j1", "_join_wait_start_j1"]),
        "old join keys are preserved but must not override the interrupted class"
    );
    // Truthful projection: the durable interrupted evidence controls the
    // verdict — the legacy context-key cascade (which would say 'yes') is
    // only the v0 projection and must not hide v1 interrupted work.
    assert_eq!(parsed["resumable"]["verdict"], json!("no"));
    assert_eq!(parsed["resumable"]["rule"], json!("interrupted"));
    assert_eq!(parsed["resumable"]["runner_check"], json!("not_applicable"));
    let list_output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list: Value = serde_json::from_slice(&list_output).expect("valid list json");
    assert_eq!(
        list["total"],
        json!(1),
        "interrupted rows count toward the honest list total"
    );
    let listed = list["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["session_id"] == "ses_crash")
        .expect("interrupted run remains visible in list mode");
    assert_eq!(listed["recovery_class"], "interrupted");
}

#[test]
fn inspect_v1_human_wait_stays_distinct_and_preserves_wait_token() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let plain = json!({"data": {"_creator_id": CREATOR}})
        .to_string()
        .into_bytes();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_wait",
                preset_id: "preset_x",
                status: "waiting_for_input",
                current_task_id: Some("task_7"),
                context: &plain,
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 4,
                run_state_json: Some(run_state(None, false, true)),
            },
        )
        .await;
        // A v1 human wait carries a frozen descriptor; the shared projection
        // only offers `continue` when that source still verifies (P1-C).
        let caps = nexus_orchestration::capability::CapabilityRegistry::with_builtins();
        let loaded = nexus_orchestration::preset::load_embedded_preset("memory-augmented", &caps)
            .expect("embedded preset");
        let descriptor = serde_json::json!({
            "creator_id": CREATOR,
            "work_id": null,
            "workspace_root": "/tmp",
            "preset_id": "memory-augmented",
            "preset_version": loaded.version,
            "source": loaded.source_identity.clone().expect("source identity"),
            "input": {},
            "agent_bindings": {},
            "parent_session_id": null,
            "graph_name": null
        });
        sqlx::query(
            "UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?",
        )
        .bind(serde_json::to_vec(&descriptor).expect("descriptor json"))
        .bind("ses_wait")
        .execute(&pool)
        .await
        .expect("seed reconstructable descriptor");
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_wait", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["recovery_class"], json!("human_wait"));
    assert_eq!(parsed["wait_id"], json!("wait-tok-1"));
    assert_eq!(parsed["db_status"], json!("waiting_for_input"));
    assert_eq!(
        parsed["allowed_actions"],
        json!(["continue", "cancel"]),
        "human_wait offers exactly continue/cancel (tri-QC P1-B)"
    );
    // Legacy resumable verdict stays a hard no for the human wait.
    assert_eq!(parsed["resumable"]["verdict"], json!("no"));
}

#[test]
fn inspect_v1_tokenless_wait_with_old_join_keys_stays_human_wait() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let old_join = json!({"data": {
        "_converge_arrivals_old": ["a"],
        "_join_wait_start_old": 1
    }})
    .to_string()
    .into_bytes();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_tokenless_wait",
                preset_id: "preset_x",
                status: "waiting_for_input",
                current_task_id: Some("manual_wait"),
                context: &old_join,
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 4,
                run_state_json: Some(run_state(None, false, false)),
            },
        )
        .await;
        pool.close().await;
    });
    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_tokenless_wait", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["recovery_class"], "human_wait");
    assert_eq!(parsed["resumable"]["verdict"], "no");
}

#[test]
fn inspect_v1_corrupt_descriptor_is_unreadable_in_detail_and_list() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_bad_descriptor",
                preset_id: "preset_x",
                status: "running",
                current_task_id: Some("task_9"),
                context: &chain_context(),
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 4,
                run_state_json: Some(run_state(None, false, false)),
            },
        )
        .await;
        sqlx::query(
            "UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?",
        )
        .bind(b"not-json".as_slice())
        .bind("ses_bad_descriptor")
        .execute(&pool)
        .await
        .expect("corrupt descriptor");
        pool.close().await;
    });

    let detail = nexus42(home.path())
        .args(["ops", "inspect", "ses_bad_descriptor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let detail: Value = serde_json::from_slice(&detail).expect("detail json");
    assert_eq!(detail["recovery_class"], "unreadable");

    let list = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list: Value = serde_json::from_slice(&list).expect("list json");
    let row = list["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["session_id"] == "ses_bad_descriptor")
        .expect("corrupt descriptor row listed");
    assert_eq!(row["recovery_class"], "unreadable");
}

#[test]
fn inspect_v1_corrupt_state_is_unreadable_non_replayable() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_badblob", "preset_x", "running", Some("t1"), b"{}"),
        )
        .await;
        sqlx::query::<sqlx::Sqlite>(
            "UPDATE orchestration_sessions
                SET execution_version = 1, state_revision = 2, run_state_json = ?
              WHERE session_id = 'ses_badblob'",
        )
        .bind(b"not-json-at-all".as_slice())
        .execute(&pool)
        .await
        .expect("corrupt v1 blob");
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_badblob", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["recovery_class"], json!("unreadable"));
    assert!(parsed.get("wait_id").is_none());
}

#[test]
fn inspect_v1_completed_is_terminal_without_live_runner() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        // v1 authoritative completed row with an unresolved cancel_requested
        // mark — terminal still wins over interrupted evidence (A7 rule 1).
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_done",
                preset_id: "preset_x",
                status: "completed",
                current_task_id: None,
                context: b"{}",
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 9,
                run_state_json: Some(run_state(None, true, false)),
            },
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_done", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["recovery_class"], json!("terminal"));
    assert_eq!(parsed["db_status"], json!("completed"));
    assert_eq!(parsed["resumable"]["verdict"], json!("no"));
    assert_eq!(parsed["resumable"]["rule"], json!("terminal_status"));

    // Terminal lookup works without a live engine runner: the human view
    // must also render the terminal class.
    let human = nexus42(home.path())
        .args(["ops", "inspect", "ses_done"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains("recovery_class: terminal"), "{human}");
    assert!(human.contains("status:         completed"), "{human}");
}

#[test]
fn inspect_v1_legacy_v0_chain_row_is_legacy_unverified() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let ctx = chain_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new(
                "ses_legacy",
                "preset_chain",
                "running",
                Some("task_9"),
                &ctx,
            ),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_legacy", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["execution_version"], json!(0));
    assert_eq!(parsed["recovery_class"], json!("legacy_unverified"));
    assert!(parsed.get("wait_id").is_none());
    // The shipped conservative legacy classifier still projects resumability.
    assert_eq!(parsed["resumable"]["verdict"], json!("yes"));
    assert_eq!(parsed["resumable"]["rule"], json!("chain_class_no_failure"));
}

#[test]
#[allow(clippy::too_many_lines)] // CLI inspect list-mode regression covering all recovery classes in one test
fn inspect_v1_list_mode_recovery_classes_and_read_only() {
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let chain = chain_context();
    let plain = json!({"data": {"_creator_id": CREATOR}})
        .to_string()
        .into_bytes();
    run_async(async {
        let pool = create_db(&db_path).await;
        // v1 converge/merge chain (running, live join keys).
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_chain",
                preset_id: "preset_chain",
                status: "running",
                current_task_id: Some("task_9"),
                context: &chain,
                updated_at: 1_756_990_400,
                execution_version: 1,
                state_revision: 3,
                run_state_json: Some(run_state(None, false, false)),
            },
        )
        .await;
        // v1 human wait.
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_wait",
                preset_id: "preset_x",
                status: "waiting_for_input",
                current_task_id: Some("task_7"),
                context: &plain,
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 4,
                run_state_json: Some(run_state(None, false, true)),
            },
        )
        .await;
        // A v1 human wait carries a frozen descriptor; the shared projection
        // only offers `continue` when that source still verifies (P1-C).
        let caps = nexus_orchestration::capability::CapabilityRegistry::with_builtins();
        let loaded = nexus_orchestration::preset::load_embedded_preset("memory-augmented", &caps)
            .expect("embedded preset");
        let descriptor = serde_json::json!({
            "creator_id": CREATOR,
            "work_id": null,
            "workspace_root": "/tmp",
            "preset_id": "memory-augmented",
            "preset_version": loaded.version,
            "source": loaded.source_identity.clone().expect("source identity"),
            "input": {},
            "agent_bindings": {},
            "parent_session_id": null,
            "graph_name": null
        });
        sqlx::query(
            "UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?",
        )
        .bind(serde_json::to_vec(&descriptor).expect("descriptor json"))
        .bind("ses_wait")
        .execute(&pool)
        .await
        .expect("seed reconstructable descriptor");
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let rows = parsed["rows"].as_array().expect("rows array");
    let chain_row = rows
        .iter()
        .find(|r| r["session_id"] == "ses_chain")
        .expect("chain row listed");
    assert_eq!(chain_row["recovery_class"], json!("converge_merge"));
    assert_eq!(chain_row["resumable"]["verdict"], json!("yes"));
    let wait_row = rows
        .iter()
        .find(|r| r["session_id"] == "ses_wait")
        .expect("wait row listed");
    assert_eq!(wait_row["recovery_class"], json!("human_wait"));
    assert_eq!(wait_row["wait_id"], json!("wait-tok-1"));
    assert_eq!(wait_row["resumable"]["verdict"], json!("no"));

    // Read-only proof for the v1 projection path: rows + db bytes unchanged.
    let rows_before = run_async(async {
        let pool = nexus_local_db::open_pool_read_only(&db_path)
            .await
            .expect("ro pool");
        let rows: Vec<(String, i64, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT session_id, execution_version, run_state_json
             FROM orchestration_sessions ORDER BY session_id",
        )
        .fetch_all(&pool)
        .await
        .expect("snapshot rows");
        pool.close().await;
        rows
    });
    let bytes_before = std::fs::read(&db_path).expect("db bytes before");

    nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success();
    nexus42(home.path())
        .args(["ops", "inspect", "ses_chain", "--json"])
        .assert()
        .success();

    let rows_after = run_async(async {
        let pool = nexus_local_db::open_pool_read_only(&db_path)
            .await
            .expect("ro pool");
        let rows: Vec<(String, i64, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT session_id, execution_version, run_state_json
             FROM orchestration_sessions ORDER BY session_id",
        )
        .fetch_all(&pool)
        .await
        .expect("snapshot rows");
        pool.close().await;
        rows
    });
    let bytes_after = std::fs::read(&db_path).expect("db bytes after");
    assert_eq!(rows_before, rows_after, "inspect must not mutate rows");
    assert_eq!(
        bytes_before, bytes_after,
        "inspect must not write the db file"
    );
}

// ---------------------------------------------------------------------------
// v1.186 P0 T2 fix round 1 — L2 review findings.
// ---------------------------------------------------------------------------

/// Promote a seeded row to v1 with a given state/descriptor, or set an
/// arbitrary `execution_version` (negative/forward) without state.
async fn set_v1_metadata(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    execution_version: i64,
    run_state_json: Option<&[u8]>,
) {
    let descriptor = run_descriptor("p");
    sqlx::query::<sqlx::Sqlite>(
        "UPDATE orchestration_sessions
            SET execution_version = ?, state_revision = 5,
                run_state_json = ?, run_descriptor_json = ?
          WHERE session_id = ?",
    )
    .bind(execution_version)
    .bind(run_state_json)
    .bind(descriptor)
    .bind(session_id)
    .execute(pool)
    .await
    .expect("set v1 metadata");
}

#[test]
fn inspect_v1_terminal_wins_with_absent_state_blob() {
    // Critical 1: a v1 row with NO run_state_json (absent durable state) or
    // a corrupt blob must still classify Terminal for authoritative
    // completed/failed/cancelled (A7 rule 1 beats rule 2).
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        for (id, status) in [
            ("ses_done_absent", "completed"),
            ("ses_failed_absent", "failed"),
            ("ses_cancelled_absent", "cancelled"),
        ] {
            seed_session(&pool, &SeedRow::new(id, "preset_x", status, None, b"{}")).await;
        }
        for (id, status) in [
            ("ses_done_corrupt", "completed"),
            ("ses_failed_corrupt", "failed"),
            ("ses_cancelled_corrupt", "cancelled"),
        ] {
            seed_session(&pool, &SeedRow::new(id, "preset_x", status, None, b"{}")).await;
        }
        for (id, state) in [
            ("ses_done_absent", None),
            ("ses_failed_absent", None),
            ("ses_cancelled_absent", None),
            ("ses_done_corrupt", Some(b"not-json".as_slice())),
            ("ses_failed_corrupt", Some(b"not-json".as_slice())),
            ("ses_cancelled_corrupt", Some(b"not-json".as_slice())),
        ] {
            set_v1_metadata(&pool, id, 1, state).await;
        }
        pool.close().await;
    });

    for id in [
        "ses_done_absent",
        "ses_failed_absent",
        "ses_cancelled_absent",
        "ses_done_corrupt",
        "ses_failed_corrupt",
        "ses_cancelled_corrupt",
    ] {
        let output = nexus42(home.path())
            .args(["ops", "inspect", id, "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let parsed: Value = serde_json::from_slice(&output).expect("valid json");
        assert_eq!(
            parsed["recovery_class"],
            json!("terminal"),
            "{id} must stay terminal even with absent/corrupt durable state"
        );
        assert_eq!(parsed["resumable"]["verdict"], json!("no"));
        assert_eq!(parsed["resumable"]["rule"], json!("terminal_status"));
        assert!(
            parsed.get("wait_id").is_none(),
            "terminal rows must not expose a wait token: {parsed}"
        );
    }
}

#[test]
fn inspect_v1_structurally_corrupt_state_is_unreadable_in_list_and_detail() {
    // Critical 3 + 2: JSON-valid but not deserializable as RunStateV1
    // (`{"cancel_requested":"false"}` — string where bool required) must be
    // Unreadable in BOTH modes; list must not label it SafeBoundary/
    // HumanWait/ConvergeMerge. Also proves list/detail parity through the
    // single canonical classifier.
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let structural = br#"{"cancel_requested":"false"}"#;
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_struct", "preset_x", "running", Some("task_1"), b"{}"),
        )
        .await;
        set_v1_metadata(&pool, "ses_struct", 1, Some(structural)).await;
        pool.close().await;
    });

    // Detail.
    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_struct", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let detail: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(detail["recovery_class"], json!("unreadable"));
    assert_eq!(detail["resumable"]["verdict"], json!("unknown"));
    assert_eq!(
        detail["resumable"]["rule"],
        json!("unreadable_metadata"),
        "v1 corrupt metadata must carry the explicit non-replayable metadata reason"
    );
    assert!(detail.get("wait_id").is_none());

    // List: same row must agree (single classifier, structural validation).
    let output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let rows = parsed["rows"].as_array().expect("rows array");
    let row = rows
        .iter()
        .find(|r| r["session_id"] == "ses_struct")
        .expect("row listed");
    assert_eq!(
        row["recovery_class"],
        json!("unreadable"),
        "list must not coerce structurally corrupt RunStateV1 evidence: {row}"
    );
    assert_eq!(row["resumable"]["verdict"], json!("unknown"));
    assert_eq!(
        row["resumable"]["rule"],
        json!("unreadable_metadata"),
        "detail and list must name the same unreadable-metadata rule"
    );
    assert!(row.get("wait_id").is_none());
}

#[test]
fn inspect_v1_unsupported_execution_versions_are_unreadable() {
    // Important 2 (detail + list): negative and forward (>= 2) execution
    // versions are unsupported — never coerced to legacy_unverified or v1.
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let state = run_state(None, false, false);
    run_async(async {
        let pool = create_db(&db_path).await;
        for (id, version) in [("ses_neg", -1), ("ses_fwd", 2), ("ses_fwd99", 99)] {
            let ctx = json!({"data": {"_creator_id": CREATOR}})
                .to_string()
                .into_bytes();
            seed_session(
                &pool,
                &SeedRow::new(id, "preset_x", "running", Some("task_1"), &ctx),
            )
            .await;
            // Even a structurally valid state blob must not salvage an
            // unsupported version.
            set_v1_metadata(&pool, id, version, Some(&state)).await;
        }
        pool.close().await;
    });

    for id in ["ses_neg", "ses_fwd", "ses_fwd99"] {
        let output = nexus42(home.path())
            .args(["ops", "inspect", id, "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let detail: Value = serde_json::from_slice(&output).expect("valid json");
        assert_eq!(
            detail["recovery_class"],
            json!("unreadable"),
            "{id} (execution_version) must be unreadable, not coerced"
        );
        assert_eq!(detail["resumable"]["verdict"], json!("unknown"));
        assert!(
            detail.get("wait_id").is_none(),
            "unsupported versions must not expose a wait token"
        );
    }

    // List mode must agree (bounded to non-terminal rows; both are running).
    let output = nexus42(home.path())
        .args(["ops", "inspect", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    let rows = parsed["rows"].as_array().expect("rows array");
    for id in ["ses_neg", "ses_fwd", "ses_fwd99"] {
        let row = rows
            .iter()
            .find(|r| r["session_id"] == id)
            .expect("row listed");
        assert_eq!(
            row["recovery_class"],
            json!("unreadable"),
            "list must agree {id} is unreadable: {row}"
        );
    }
}

#[test]
fn inspect_v1_human_wait_token_beats_old_join_keys() {
    // Important 1: a durable A4 wait token beats old scheduler join keys —
    // the row classifies HumanWait (not ConvergeMerge) and stays `no`.
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let chain = chain_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_wait_joins",
                preset_id: "preset_chain",
                status: "waiting_for_input",
                current_task_id: Some("task_7"),
                context: &chain,
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 4,
                run_state_json: Some(run_state(None, false, true)),
            },
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_wait_joins", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(
        parsed["live_join_keys"],
        json!(["_converge_arrivals_j1", "_join_wait_start_j1"]),
        "old join keys are preserved"
    );
    assert_eq!(
        parsed["recovery_class"],
        json!("human_wait"),
        "the durable wait token must beat old scheduler join keys"
    );
    assert_eq!(parsed["wait_id"], json!("wait-tok-1"));
    assert_eq!(parsed["resumable"]["verdict"], json!("no"));
    assert_eq!(parsed["resumable"]["rule"], json!("human_wait"));
}

#[test]
fn inspect_v0_stray_state_bytes_stay_legacy_no_wait_id() {
    // Important 4: a v0 row with stray/ambiguous durable bytes must NOT be
    // interpreted through v1 wait semantics — no wait_id, legacy_unverified.
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let wait_state = run_state(None, false, true); // carries wait-tok-1
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow::new("ses_v0_stray", "preset_x", "running", Some("task_1"), b"{}"),
        )
        .await;
        set_v1_metadata(&pool, "ses_v0_stray", 0, Some(&wait_state)).await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_v0_stray", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["execution_version"], json!(0));
    assert_eq!(parsed["recovery_class"], json!("legacy_unverified"));
    assert!(
        parsed.get("wait_id").is_none(),
        "v0 stray state bytes must never emit a v1 wait token: {parsed}"
    );
}

#[test]
fn inspect_list_human_output_names_recovery_class() {
    // Important 5: the human (non-JSON) list must name `recovery_class` per
    // row, just like JSON list and detail do.
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let chain = chain_context();
    run_async(async {
        let pool = create_db(&db_path).await;
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_chain",
                preset_id: "preset_chain",
                status: "running",
                current_task_id: Some("task_9"),
                context: &chain,
                updated_at: 1_756_990_400,
                execution_version: 1,
                state_revision: 3,
                run_state_json: Some(run_state(None, false, false)),
            },
        )
        .await;
        seed_session(
            &pool,
            &SeedRow::new("ses_legacy", "preset_x", "running", Some("task_1"), &chain),
        )
        .await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let chain_line = text
        .lines()
        .find(|l| l.starts_with("ses_chain"))
        .expect("chain row line");
    assert!(
        chain_line.contains("converge_merge"),
        "human list row must name recovery_class, got: {chain_line}"
    );
    let legacy_line = text
        .lines()
        .find(|l| l.starts_with("ses_legacy"))
        .expect("legacy row line");
    assert!(
        legacy_line.contains("legacy_unverified"),
        "human list row must name recovery_class, got: {legacy_line}"
    );
}

// ---------------------------------------------------------------------------
// v1.186 P0 T2 fix round 2 — unreadable-metadata reason + wait_id gating.
// ---------------------------------------------------------------------------

#[test]
fn inspect_v1_corrupt_metadata_with_readable_context_is_unreadable_metadata() {
    // Review round-2 Important: a v1 row whose context_json is perfectly
    // readable (`{}`) but whose durable run-state metadata is structurally
    // invalid must NOT be mapped to the legacy `context_unreadable` wording —
    // the explanation must name the non-replayable v1 metadata explicitly,
    // and the rule must be `unreadable_metadata`.
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    let structural = br#"{"cancel_requested":"false"}"#;
    run_async(async {
        let pool = create_db(&db_path).await;
        // Readable `{}` context: no `data` object, but that is irrelevant —
        // the v1 metadata governs, and it is corrupt.
        seed_session(
            &pool,
            &SeedRow::new("ses_meta", "preset_x", "running", Some("task_1"), b"{}"),
        )
        .await;
        set_v1_metadata(&pool, "ses_meta", 1, Some(structural)).await;
        pool.close().await;
    });

    let output = nexus42(home.path())
        .args(["ops", "inspect", "ses_meta", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(parsed["recovery_class"], json!("unreadable"));
    assert_eq!(parsed["resumable"]["verdict"], json!("unknown"));
    assert_eq!(
        parsed["resumable"]["rule"],
        json!("unreadable_metadata"),
        "corrupt v1 metadata must not be labelled context_unreadable: {parsed}"
    );
    let explanation = parsed["resumable"]["explanation"].as_str().unwrap();
    assert!(
        explanation.contains("metadata") && explanation.contains("non-replayable"),
        "explanation must name the explicit non-replayable metadata reason: {explanation}"
    );
    assert!(
        !explanation.contains("context unreadable"),
        "readable context must not be labelled context-unreadable: {explanation}"
    );
    assert!(
        parsed.get("wait_id").is_none(),
        "unreadable rows must not expose a wait token"
    );

    // Human view mirrors the explicit metadata wording.
    let human = nexus42(home.path())
        .args(["ops", "inspect", "ses_meta"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    let resumable = human
        .lines()
        .find(|l| l.starts_with("resumable:"))
        .expect("resumable line");
    assert!(
        resumable.contains("metadata") && resumable.contains("non-replayable"),
        "human resumable line must name the metadata reason, got: {resumable}"
    );
}

#[test]
fn inspect_v1_terminal_with_stale_wait_bytes_has_no_wait_id() {
    // Review round-2 Minor: wait_id is exposed ONLY when the canonical class
    // is human_wait. A terminal row carrying stale wait bytes (the durable
    // state still holds an old wait token) must not advertise a usable wait
    // token — the class is terminal and the row must not suggest resume.
    let home = tempfile::TempDir::new().unwrap();
    let db_path = seed_home_config(home.path());
    run_async(async {
        let pool = create_db(&db_path).await;
        // completed + wait-bearing run state (stale bytes).
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_done_wait",
                preset_id: "preset_x",
                status: "completed",
                current_task_id: None,
                context: b"{}",
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 9,
                run_state_json: Some(run_state(None, false, true)),
            },
        )
        .await;
        // cancelled + wait-bearing run state (stale bytes).
        seed_session(
            &pool,
            &SeedRow {
                session_id: "ses_cancelled_wait",
                preset_id: "preset_x",
                status: "cancelled",
                current_task_id: None,
                context: b"{}",
                updated_at: 1_756_990_300,
                execution_version: 1,
                state_revision: 9,
                run_state_json: Some(run_state(None, false, true)),
            },
        )
        .await;
        pool.close().await;
    });

    for id in ["ses_done_wait", "ses_cancelled_wait"] {
        let output = nexus42(home.path())
            .args(["ops", "inspect", id, "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let parsed: Value = serde_json::from_slice(&output).expect("valid json");
        assert_eq!(parsed["recovery_class"], json!("terminal"), "{id}");
        assert_eq!(parsed["resumable"]["verdict"], json!("no"), "{id}");
        assert_eq!(
            parsed["resumable"]["rule"],
            json!("terminal_status"),
            "{id}"
        );
        assert!(
            parsed.get("wait_id").is_none(),
            "terminal rows must not advertise a wait token even with stale \
             wait bytes: {parsed}"
        );
    }
}

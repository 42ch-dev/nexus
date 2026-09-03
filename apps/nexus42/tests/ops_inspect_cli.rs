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
use serde_json::{json, Value};
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
        }
    }
}

async fn seed_session(pool: &sqlx::SqlitePool, row: &SeedRow<'_>) {
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at)
         VALUES (?, ?, ?, 3, ?, ?, ?, 1_756_990_000, ?)",
    )
    .bind(row.session_id)
    .bind(CREATOR)
    .bind(row.preset_id)
    .bind(row.status)
    .bind(row.current_task_id)
    .bind(row.context)
    .bind(row.updated_at)
    .execute(pool)
    .await
    .expect("seed session");
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
        "_join_wait_start_j1": 1_756_990_100
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
    // only the explanation wording differs.
    assert_eq!(obj["context_readable"], json!(false));
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
}

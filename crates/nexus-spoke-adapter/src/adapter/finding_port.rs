//! Production `FindingPort` impl — routes `findings` storage through
//! spoke's port surface (spec §7.4).
//!
//! # Wire ↔ row mapping (impedance-aware)
//!
//! Nexus `findings` pre-dates spoke and carries creator-scoped,
//! per-Work lifecycle columns that the spoke `Finding` wire type does
//! not surface (per-creator isolation, routing hints, six-state
//! lifecycle). The adapter maps the wire type onto the storage row at
//! this boundary:
//!
//! | Spoke `Finding` field        | Nexus `findings` column             |
//! |------------------------------|-------------------------------------|
//! | `finding_id`                 | `finding_id`                        |
//! | `severity` (vocabulary mapped) | `severity`                        |
//! | `status` (vocabulary mapped) | `status`                            |
//! | `title`                      | `title`                             |
//! | `description`                | `description`                       |
//! | `kind`                       | `kind` (default `'craft'`)          |
//! | `suggested_fix`              | `rule_suggestion`                   |
//! | `created_at`                 | `created_at` (Unix epoch)           |
//! | `updated_at`                 | `updated_at` (Unix epoch)           |
//! | `extensions.nexus.work_id`   | `work_id` (required FK)             |
//! | `extensions.nexus.creator_id`| `creator_id` (required, isolation)  |
//! | `extensions.nexus.chapter`   | `chapter` (optional)                |
//! | `extensions.nexus.target_executor` | `target_executor` (default `'none'`) |
//!
//! ## Vocabulary mapping
//!
//! Spoke's documented severities `{info, warning, error}` map onto
//! nexus's `{info, minor, major, blocker}` (warning→minor, error→major;
//! `blocker` has no spoke equivalent). Spoke's documented statuses
//! `{open, resolved, dismissed}` map onto the V1.49 lifecycle
//! `{open, resolved, wont_fix}` (`dismissed`→`wont_fix`). Unknown
//! vocabulary rejects with `INVALID_INPUT` so the impedance does not
//! silently fabricate data.
//!
//! ## Nexus-only columns
//!
//! `source_schedule_id` has no spoke equivalent and defaults to NULL
//! (it is the V1.47 review-idempotency key, which the spoke port
//! surface does not own).
//!
//! # Required extensions
//!
//! `findings.work_id` and `findings.creator_id` are NOT NULL with no
//! sensible defaults (Work/creator isolation is the table's whole
//! point). The spoke `Finding` has neither field; they MUST be supplied
//! via `extensions.nexus.{work_id, creator_id}`. Missing values reject
//! with `INVALID_INPUT`.

use super::NexusAdapter;
use crate::{
    Finding as SpokeFinding, FindingExtensionsKey, FindingPort, SpokeReject, SpokeRejectCode,
    SpokeResult,
};
use nexus_local_db::findings::{validate_finding_enums, Finding as NexusFinding};
use nexus_local_db::LocalDbError;
use serde_json::{json, Map, Value};

impl FindingPort for NexusAdapter<'_> {
    fn put_findings(&self, findings: Vec<SpokeFinding>) -> SpokeResult<Vec<SpokeFinding>> {
        let pool = self.pool.clone();
        self.block_on(async move {
            let mut persisted: Vec<SpokeFinding> = Vec::with_capacity(findings.len());

            // W-1 (qc3): wrap the entire batch in one SQLite transaction so a
            // mid-batch failure rolls back every earlier insert. Without this,
            // each insert auto-committed independently and the caller would
            // receive a Reject while the preceding findings stayed persisted
            // (a retry would then hit UNIQUE errors on those rows). `tx` drops
            // uncommitted on any early `return` below → automatic rollback.
            let mut tx = match pool.begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InternalError,
                        format!("failed to begin findings batch transaction: {e}"),
                        json!({}),
                    );
                }
            };

            for finding in findings {
                let nexus_finding = match map_spoke_to_nexus(&finding) {
                    Ok(n) => n,
                    Err(rejection) => return SpokeResult::Reject(rejection),
                };
                let finding_id = nexus_finding.finding_id.clone();
                if let Err(e) = insert_finding_tx(&mut tx, &nexus_finding).await {
                    return reject(
                        SpokeRejectCode::InternalError,
                        format!("storage error on finding {finding_id} insert: {e}"),
                        json!({ "finding_id": finding_id }),
                    );
                }
                persisted.push(finding);
            }

            // All inserts succeeded — commit the batch atomically. Until this
            // point nothing is durably persisted; a failure here rolls back.
            if let Err(e) = tx.commit().await {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("failed to commit findings batch transaction: {e}"),
                    json!({}),
                );
            }

            SpokeResult::Ok(persisted)
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Insert a [`NexusFinding`] inside a caller-owned `SQLite` transaction.
///
/// Mirrors [`nexus_local_db::findings::create_finding`]'s INSERT (same column list,
/// bind order, and `validate_finding_enums` defense-in-depth) but targets a
/// `&mut Transaction` instead of `&SqlitePool` so the spoke `put_findings`
/// batch can wrap N inserts in one atomic `BEGIN`/`COMMIT` (W-1, qc3). The
/// caller owns the transaction boundary and must `commit()` once every row
/// is inserted.
///
/// # Errors
///
/// Returns [`LocalDbError`] if enum validation or the insert fails.
///
/// SAFETY: runtime query — mirrors the compile-time-checked INSERT in
/// `create_finding`; the `.sqlx` cache is not warmed for the
/// transaction-executor shape (see nexus-local-db AGENTS.md waiver
/// R-V140P0-S3). The column list + bind order match `create_finding`.
async fn insert_finding_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    f: &NexusFinding,
) -> Result<(), LocalDbError> {
    validate_finding_enums(&f.severity, &f.status, &f.target_executor)?;
    sqlx::query(
        "INSERT INTO findings (finding_id, work_id, chapter, severity, status, title, \
         description, target_executor, creator_id, kind, rule_suggestion, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&f.finding_id)
    .bind(&f.work_id)
    .bind(f.chapter)
    .bind(&f.severity)
    .bind(&f.status)
    .bind(&f.title)
    .bind(&f.description)
    .bind(&f.target_executor)
    .bind(&f.creator_id)
    .bind(&f.kind)
    .bind(&f.rule_suggestion)
    .bind(f.created_at)
    .bind(f.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Map a single spoke [`SpokeFinding`] onto a nexus [`NexusFinding`] row
/// ready for `create_finding`. Extracts nexus-required fields from
/// `extensions.nexus`, applies the vocabulary mapping, and rejects with
/// a structured [`SpokeReject`] on missing fields / unknown vocabulary.
fn map_spoke_to_nexus(finding: &SpokeFinding) -> Result<NexusFinding, SpokeReject> {
    let finding_id = finding.finding_id.clone();
    let ext = extract_nexus_extension(finding);

    let Some(work_id) = ext.world_id else {
        return Err(reject_obj(
            SpokeRejectCode::InvalidInput,
            format!("Finding {finding_id} is missing required extensions.nexus.work_id"),
            json!({
                "finding_id": finding_id,
                "missing": ["extensions.nexus.work_id"],
            }),
        ));
    };

    let Some(creator_id) = ext.creator_id else {
        return Err(reject_obj(
            SpokeRejectCode::InvalidInput,
            format!("Finding {finding_id} is missing required extensions.nexus.creator_id"),
            json!({
                "finding_id": finding_id,
                "missing": ["extensions.nexus.creator_id"],
            }),
        ));
    };

    let Some(severity) = map_severity(&finding.severity) else {
        return Err(reject_obj(
            SpokeRejectCode::InvalidInput,
            format!(
                "Finding {finding_id}: unknown spoke severity '{}'; expected one of: info, warning, error",
                finding.severity
            ),
            json!({
                "finding_id": finding_id,
                "invalid": {"severity": &finding.severity},
            }),
        ));
    };

    let Some(status) = map_status(&finding.status) else {
        return Err(reject_obj(
            SpokeRejectCode::InvalidInput,
            format!(
                "Finding {finding_id}: unknown spoke status '{}'; expected one of: open, resolved, dismissed",
                finding.status
            ),
            json!({
                "finding_id": finding_id,
                "invalid": {"status": &finding.status},
            }),
        ));
    };

    let now_epoch = chrono::Utc::now().timestamp();
    let created_at = finding.created_at.map_or(now_epoch, |dt| dt.timestamp());
    let updated_at = finding.updated_at.map_or(now_epoch, |dt| dt.timestamp());

    Ok(NexusFinding {
        finding_id: finding.finding_id.clone(),
        work_id,
        chapter: ext.chapter,
        severity,
        status,
        title: finding.title.clone(),
        description: finding.description.clone(),
        target_executor: ext.target_executor.unwrap_or_else(|| "none".to_string()),
        creator_id,
        kind: finding.kind.clone().unwrap_or_else(|| "craft".to_string()),
        rule_suggestion: finding.suggested_fix.clone(),
        created_at,
        updated_at,
    })
}

/// Build a [`SpokeReject`] from `code`, `message`, and an object-shaped
/// `details` payload. The details value MUST be a JSON object (the typical
/// shape for finders); non-object values fall back to a single
/// `"detail"` key to satisfy the typed `Map<String, Value>` details slot.
fn reject_obj(code: SpokeRejectCode, message: String, details: Value) -> SpokeReject {
    let details_map = match details {
        Value::Object(map) => Some(map),
        other => {
            let mut map = Map::new();
            map.insert("detail".to_string(), other);
            Some(map)
        }
    };
    SpokeReject {
        code,
        message,
        details: details_map,
    }
}

/// Borrowed view of the nexus-required fields carried under
/// `extensions.nexus` on a spoke `Finding`.
struct NexusExtension {
    world_id: Option<String>,
    creator_id: Option<String>,
    chapter: Option<i64>,
    target_executor: Option<String>,
}

/// Read the nexus-required fields from `extensions.nexus` on a spoke
/// `Finding`. Returns `NexusExtension { ..None }` when the namespace
/// is absent (the caller treats missing required fields as reject).
fn extract_nexus_extension(finding: &SpokeFinding) -> NexusExtension {
    // The `"nexus"` literal always satisfies the namespace regex —
    // construction is infallible at runtime (mirrors the V1.139
    // `KnowledgeEntryExtensionsKey` pattern).
    let Ok(key) = FindingExtensionsKey::try_from("nexus") else {
        return NexusExtension {
            world_id: None,
            creator_id: None,
            chapter: None,
            target_executor: None,
        };
    };
    let Some(ns) = finding.extensions.get(&key) else {
        return NexusExtension {
            world_id: None,
            creator_id: None,
            chapter: None,
            target_executor: None,
        };
    };
    NexusExtension {
        world_id: ns.get("work_id").and_then(Value::as_str).map(String::from),
        creator_id: ns
            .get("creator_id")
            .and_then(Value::as_str)
            .map(String::from),
        chapter: ns.get("chapter").and_then(Value::as_i64),
        target_executor: ns
            .get("target_executor")
            .and_then(Value::as_str)
            .map(String::from),
    }
}

/// Map spoke `Finding.severity` vocabulary onto nexus `findings.severity`.
///
/// - `info` → `info`
/// - `warning` → `minor`
/// - `error` → `major`
///
/// `blocker` has no spoke equivalent and is not produced here. Unknown
/// values return `None` (the caller rejects with `INVALID_INPUT`).
fn map_severity(spoke_severity: &str) -> Option<String> {
    match spoke_severity {
        "info" => Some("info".to_string()),
        "warning" => Some("minor".to_string()),
        "error" => Some("major".to_string()),
        _ => None,
    }
}

/// Map spoke `Finding.status` vocabulary onto the V1.49 `findings.status`
/// lifecycle.
///
/// - `open` → `open`
/// - `resolved` → `resolved`
/// - `dismissed` → `wont_fix`
///
/// The V1.49 additions (`triaged`, `in_review`, `duplicate`) are
/// nexus-side lifecycle states the spoke port surface does not produce.
/// Unknown values return `None` (the caller rejects with
/// `INVALID_INPUT`).
fn map_status(spoke_status: &str) -> Option<String> {
    match spoke_status {
        "open" => Some("open".to_string()),
        "resolved" => Some("resolved".to_string()),
        "dismissed" => Some("wont_fix".to_string()),
        _ => None,
    }
}

/// Construct a `SpokeResult::Reject` (mirrors the helper in
/// `knowledge_entry_port.rs`).
fn reject<T>(code: SpokeRejectCode, message: impl Into<String>, details: Value) -> SpokeResult<T> {
    let details_map = match details {
        Value::Object(map) => Some(map),
        other => {
            let mut map = Map::new();
            map.insert("detail".to_string(), other);
            Some(map)
        }
    };
    SpokeResult::Reject(SpokeReject {
        code,
        message: message.into(),
        details: details_map,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FindingPort;
    use nexus_local_db::{open_pool, run_migrations};
    use serde_json::json;

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_work(pool: &sqlx::SqlitePool) {
        // SAFETY: test-only static INSERTs with bind params. Findings
        // require a Work FK; the Work requires a creator. We seed the
        // minimum chain: creator → work. `works.created_at`/`updated_at`
        // are NOT NULL TEXT columns (see migration
        // `202606190004_work_profile_essay.sql`) — supply literal
        // timestamps so the row lands.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO works \
             (work_id, creator_id, workspace_slug, status, title, long_term_goal, \
              initial_idea, intake_status, created_at, updated_at) \
             VALUES ('wrk_test', 'ctr_test', 'wrk_test', 'active', 'Test', 'goal', 'idea', \
                     'complete', '2026-07-28T00:00:00Z', '2026-07-28T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Build a spoke `Finding` fixture with the nexus-required
    /// `extensions.nexus.{work_id, creator_id}` set.
    fn spoke_finding(
        finding_id: &str,
        severity: &str,
        status: &str,
        kind: Option<&str>,
        suggested_fix: Option<&str>,
    ) -> SpokeFinding {
        let mut value = json!({
            "schema_version": 1,
            "finding_id": finding_id,
            "severity": severity,
            "status": status,
            "title": format!("Finding {finding_id}"),
            "description": "test finding body",
            "extensions": {
                "nexus": {
                    "work_id": "wrk_test",
                    "creator_id": "ctr_test"
                }
            }
        });
        if let Some(k) = kind {
            value["kind"] = json!(k);
        }
        if let Some(f) = suggested_fix {
            value["suggested_fix"] = json!(f);
        }
        serde_json::from_value(value).expect("valid spoke Finding fixture")
    }

    async fn fetch_nexus_finding(pool: &sqlx::SqlitePool, finding_id: &str) -> NexusFinding {
        // SAFETY: test-only static SELECT with bind param.
        // The 11-tuple shape mirrors the `findings` row projection; clippy
        // would flag this as `type_complexity` if it appeared in production
        // code, but the test-only projection is intentional (no schema
        // row struct exists in the public DAO surface — `findings::Finding`
        // uses a different code path).
        #![allow(clippy::type_complexity)]
        type FindingRow = (
            String,
            String,
            Option<i64>,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
        );
        let row: FindingRow = sqlx::query_as(
            "SELECT finding_id, work_id, chapter, severity, status, title, description, \
             target_executor, creator_id, kind, rule_suggestion \
             FROM findings WHERE finding_id = ?",
        )
        .bind(finding_id)
        .fetch_one(pool)
        .await
        .expect("row persisted");
        NexusFinding {
            finding_id: row.0,
            work_id: row.1,
            chapter: row.2,
            severity: row.3,
            status: row.4,
            title: row.5,
            description: row.6,
            target_executor: row.7,
            creator_id: row.8,
            kind: row.9,
            rule_suggestion: row.10,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_findings_happy_path_persists_with_vocabulary_mapping() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let spoke = spoke_finding(
            "fnd_happy",
            "warning",
            "open",
            Some("continuity"),
            Some("fix me"),
        );

        let result = adapter.put_findings(vec![spoke]);
        let returned = match result {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        };
        assert_eq!(returned.len(), 1);
        assert_eq!(returned[0].finding_id, "fnd_happy");

        // Verify the persisted row carries the mapped vocabulary.
        let nexus = fetch_nexus_finding(&pool, "fnd_happy").await;
        assert_eq!(nexus.work_id, "wrk_test");
        assert_eq!(nexus.creator_id, "ctr_test");
        assert_eq!(nexus.severity, "minor", "spoke `warning` → nexus `minor`");
        assert_eq!(nexus.status, "open");
        assert_eq!(nexus.kind, "continuity");
        assert_eq!(nexus.rule_suggestion.as_deref(), Some("fix me"));
        assert_eq!(
            nexus.target_executor, "none",
            "no spoke field → default 'none'"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_findings_maps_error_severity_and_dismissed_status() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let spoke = spoke_finding("fnd_voc", "error", "dismissed", None, None);

        match adapter.put_findings(vec![spoke]) {
            SpokeResult::Ok(v) => assert_eq!(v.len(), 1),
            SpokeResult::Reject(r) => panic!("ok on valid vocabulary: {r:?}"),
        }

        let nexus = fetch_nexus_finding(&pool, "fnd_voc").await;
        assert_eq!(nexus.severity, "major", "spoke `error` → nexus `major`");
        assert_eq!(
            nexus.status, "wont_fix",
            "spoke `dismissed` → nexus `wont_fix`"
        );
        assert_eq!(nexus.kind, "craft", "default kind");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_findings_missing_work_id_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool).await;

        let adapter = NexusAdapter::new(pool);
        // Strip the work_id; keep creator_id so the rejection is
        // specifically about the missing work_id.
        let spoke: SpokeFinding = serde_json::from_value(json!({
            "schema_version": 1,
            "finding_id": "fnd_no_work",
            "severity": "info",
            "status": "open",
            "title": "no work",
            "description": "x",
            "extensions": {
                "nexus": { "creator_id": "ctr_test" }
            }
        }))
        .expect("valid Finding");

        match adapter.put_findings(vec![spoke]) {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert_eq!(
                    r.details.as_ref().and_then(|d| d.get("finding_id")),
                    Some(&json!("fnd_no_work"))
                );
            }
            SpokeResult::Ok(_) => panic!("expected reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_findings_unknown_severity_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool).await;

        let adapter = NexusAdapter::new(pool);
        // `critical` is not in the spoke documented vocabulary.
        let spoke: SpokeFinding = serde_json::from_value(json!({
            "schema_version": 1,
            "finding_id": "fnd_bad_sev",
            "severity": "critical",
            "status": "open",
            "title": "bad severity",
            "description": "x",
            "extensions": {
                "nexus": { "work_id": "wrk_test", "creator_id": "ctr_test" }
            }
        }))
        .expect("valid Finding shape (vocabulary not yet validated by spoke)");

        match adapter.put_findings(vec![spoke]) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "unknown severity must reject with INVALID_INPUT"
                );
            }
            SpokeResult::Ok(_) => panic!("expected reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_findings_empty_input_returns_empty_vec() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let result = adapter.put_findings(Vec::new());
        match result {
            SpokeResult::Ok(v) => assert!(v.is_empty()),
            SpokeResult::Reject(r) => panic!("empty input must be Ok: {r:?}"),
        }
    }

    /// Assert no `findings` row exists for `finding_id`.
    async fn assert_finding_absent(pool: &sqlx::SqlitePool, finding_id: &str) {
        // SAFETY: test-only static COUNT with a bind param.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings WHERE finding_id = ?")
            .bind(finding_id)
            .fetch_one(pool)
            .await
            .expect("count query succeeds");
        assert_eq!(
            count, 0,
            "finding {finding_id} must NOT be persisted after a rolled-back batch"
        );
    }

    /// W-1 (qc3): a mid-batch failure must roll back the ENTIRE batch — no
    /// partial writes. The batch is `[fnd_rb_first, fnd_rb_dup]` where the
    /// second item reuses the first's `finding_id`. The first INSERT lands
    /// inside the (uncommitted) transaction; the second hits the `finding_id`
    /// UNIQUE/PK constraint and rejects. After the call returns, neither
    /// finding may be present — proving the earlier insert was rolled back
    /// rather than left committed.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_findings_mid_batch_failure_rolls_back_entire_batch() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());

        // First item: valid, would persist on its own.
        let first = spoke_finding("fnd_rb_first", "info", "open", None, None);
        // Second item: collides on `finding_id` with the first → mid-batch
        // UNIQUE violation. Distinct severity so the two fixtures are
        // clearly separate objects.
        let second = spoke_finding("fnd_rb_first", "warning", "open", None, None);

        match adapter.put_findings(vec![first, second]) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "mid-batch collision must reject with INTERNAL_ERROR (storage-level UNIQUE violation)"
                );
            }
            SpokeResult::Ok(_) => panic!("expected reject on duplicate finding_id mid-batch"),
        }

        // Atomicity: the first insert must NOT have leaked through. Before the
        // W-1 transaction wrap, `fnd_rb_first` would be committed here.
        assert_finding_absent(&pool, "fnd_rb_first").await;
    }

    // ── V1.146 P0: InternalError on DB failure ─────────────────────────

    /// DB failure (dropped table) on put_findings surfaces `InternalError`.
    /// One representative test covers tx-begin/insert/commit — the first
    /// INSERT inside the transaction hits the missing table and rejects.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_findings_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool).await;
        sqlx::query("DROP TABLE findings")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusAdapter::new(pool);
        let finding = spoke_finding("fnd_fail", "info", "open", None, None);
        match adapter.put_findings(vec![finding]) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "dropped table must surface INTERNAL_ERROR on put_findings"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }
}

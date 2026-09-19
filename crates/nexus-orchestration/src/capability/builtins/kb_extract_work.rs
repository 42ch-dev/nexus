//! `kb.extract_work` capability — extract a `KnowledgeEntryRecord` from a work-scope KB entry.
//!
//! Full e2e pipeline (R16):
//! 1. When `job_id` present: load job row; reject wrong status.
//! 2. When `job_id` omitted: call `claim_job` for `creator_id`.
//! 3. Mark running → load work content → build extraction prompt → parse
//!    LLM response → mark done → insert `KnowledgeEntryRecord` via `SqliteKbStore`.
//!
//! The capability is stateful: it holds an `Option<SqlitePool>` for job
//! lifecycle management and `KnowledgeEntryRecord` insertion. Without a pool it returns
//! `WorkerUnavailable`.
//!
//! Design: plan `2026-05-26-v1.30-kb-extract-lifecycle-hardening.md` §K4.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Structured response expected from the extraction prompt
// ---------------------------------------------------------------------------

/// Structured response expected from the extraction LLM prompt.
#[derive(Debug, Deserialize)]
struct ExtractResponse {
    block_type: String,
    canonical_name: String,
    /// Body may be a structured JSON object (preferred) or a plain string.
    #[serde(deserialize_with = "deserialize_body")]
    body: String,
    #[serde(default)]
    #[allow(dead_code)] // read from LLM response, used for future provenance tracking
    source_work_entry_id: String,
}

/// Deserialize `body` from either a JSON object (serialized to string) or a plain string.
fn deserialize_body<'de, D: serde::Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    use serde::de::Error as _;
    let value = serde_json::Value::deserialize(de)?;
    match value {
        serde_json::Value::String(s) => Ok(s),
        other => serde_json::to_string(&other).map_err(D::Error::custom),
    }
}

// ---------------------------------------------------------------------------
// KbExtractWork capability
// ---------------------------------------------------------------------------

/// Bounded length of the source-anchor excerpt taken from the admitted Work
/// artifact (v1.191 P1 T13). The anchor is built from the artifact the
/// orchestrator loaded, never from the model's own prose.
const SOURCE_ANCHOR_EXCERPT_CHARS: usize = 256;

/// The placeholder the native callback returns when preparing the model's
/// candidate fails; the precise error is carried out of the callback and
/// returned instead of this reject.
fn extract_reject() -> nexus_spoke_adapter::SpokeReject {
    nexus_spoke_adapter::SpokeReject {
        code: nexus_spoke_adapter::SpokeRejectCode::InternalError,
        message: "the native extractor failed".to_string(),
        details: None,
    }
}

/// The `kb.extract_work` capability.
///
/// Holds an optional `SqlitePool` for job lifecycle management and `KnowledgeEntryRecord`
/// insertion. When `pool` is `None`, returns `WorkerUnavailable`.
///
/// Input schema:
/// ```json
/// {
///   "job_id": "string (optional)",
///   "creator_id": "string",
///   "work_entry_id": "string (optional, from job if omitted)",
///   "world_id": "string (optional, from job if omitted)",
///   "work_content": "string (optional, loaded by orchestrator)",
///   "llm_response": "string (optional, for finalizing after acp.prompt)"
/// }
/// ```
///
/// Output schema depends on the phase:
/// - **Prompt phase** (no `llm_response`): returns extraction prompt + job data
/// - **Finalize phase** (with `llm_response`): returns `KnowledgeEntryRecord` insert result
pub struct KbExtractWork {
    pool: Option<Arc<sqlx::SqlitePool>>,
}

impl KbExtractWork {
    /// Create a new instance without a pool (placeholder mode).
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    /// Create a new instance with a pool for full e2e pipeline.
    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(Arc::new(pool)),
        }
    }
}

impl Default for KbExtractWork {
    fn default() -> Self {
        Self::new()
    }
}

/// Structured prompt template for KB extraction.
pub fn extraction_prompt(work_content: &str) -> String {
    format!(
        r#"You are a knowledge extraction assistant. Given the following work-scope knowledge content, extract a single structured key block.

Respond with ONLY a JSON object (no markdown fences) with these fields:
- "block_type": one of "Character", "Ability", "Scene", "Organization", "Item", "Conflict", "InfoPoint", "Event"
- "canonical_name": a short, unique canonical name for this entity (snake_case or PascalCase)
- "body": a concise description or summary (1-3 paragraphs)
- "source_work_entry_id": the work entry ID if mentioned in the content, otherwise ""

Work content:
---
{work_content}
---

Respond with the JSON object now:"#
    )
}

/// Parse the JSON response from the extraction LLM into a structured type.
fn parse_extraction_response(response_text: &str) -> Result<ExtractResponse, CapabilityError> {
    let cleaned = response_text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    serde_json::from_str(cleaned).map_err(|e| {
        CapabilityError::InputInvalid(format!("Failed to parse extraction response: {e}"))
    })
}

/// Parse block type string into `BlockType`.
///
/// Accepts both `snake_case` wire values (P1 extract.md) and `PascalCase` (legacy).
fn parse_block_type(s: &str) -> Result<nexus_contracts::BlockType, CapabilityError> {
    match s {
        "Character" | "character" => Ok(nexus_contracts::BlockType::Character),
        "Ability" | "ability" => Ok(nexus_contracts::BlockType::Ability),
        "Scene" | "scene" => Ok(nexus_contracts::BlockType::Scene),
        "Organization" | "organization" => Ok(nexus_contracts::BlockType::Organization),
        "Item" | "item" => Ok(nexus_contracts::BlockType::Item),
        "Conflict" | "conflict" => Ok(nexus_contracts::BlockType::Conflict),
        "InfoPoint" | "info_point" => Ok(nexus_contracts::BlockType::InfoPoint),
        "Event" | "event" => Ok(nexus_contracts::BlockType::Event),
        _ => Err(CapabilityError::InputInvalid(format!(
            "Unknown block_type '{s}'"
        ))),
    }
}

// Single-pass claim→extract→insert→finalize pipeline; splitting would obscure the state machine.
#[allow(clippy::too_many_lines)]
#[async_trait]
impl Capability for KbExtractWork {
    fn name(&self) -> &'static str {
        "kb.extract_work"
    }
    fn input_schema(&self) -> &'static str {
        nexus_preset::capability_catalog::KB_EXTRACT_WORK_INPUT_SCHEMA
    }
    fn output_schema(&self) -> &'static str {
        r#"{
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["job_id", "status"],
        "properties": {
            "job_id": { "type": "string" },
            "status": { "type": "string" },
            "key_block_id": { "type": "string" },
            "world_id": { "type": "string" },
            "block_type": { "type": "string" },
            "canonical_name": { "type": "string" },
            "prompt": { "type": "string" },
            "prompt_length": { "type": "integer" }
        }
    }"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        let creator_id = input
            .get("creator_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'creator_id'".into()))?;

        // v1.191 P1 T13 (durable §8): the real production run identity. The
        // engine injects the trusted `_session_id` at run admission; the
        // extraction runs through `orchestrate_extract`, whose request type
        // refuses an empty run id. A capability invoked outside a trusted run
        // context refuses here — before any job claim or mutation — instead of
        // inventing a default session.
        let run_id = input
            .get("_session_id")
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                CapabilityError::Forbidden(
                    "missing trusted _session_id: orchestration context must inject the run identity"
                        .to_string(),
                )
            })?;

        // ── Phase 1: Load or claim job ──────────────────────────────
        let job = if let Some(job_id) = input.get("job_id").and_then(|v| v.as_str()) {
            // Load specific job
            let job = nexus_local_db::get_extract_job(pool, job_id)
                .await
                .map_err(|e| CapabilityError::Internal(format!("Failed to load job: {e}")))?
                .ok_or_else(|| {
                    CapabilityError::InputInvalid(format!("Job '{job_id}' not found"))
                })?;

            // QC2 W-005: Re-validate creator ownership on explicit job_id path.
            if job.creator_id != creator_id {
                return Err(CapabilityError::InputInvalid("job creator mismatch".into()));
            }

            // Reject wrong status
            if job.status != "queued" && job.status != "running" {
                return Err(CapabilityError::InputInvalid(format!(
                    "Job '{}' has status '{}', expected 'queued' or 'running'",
                    job.job_id, job.status
                )));
            }
            job
        } else {
            // Claim next queued job for this creator
            // WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P3-S1
            // — cross-creator job_id claim test gap: claim path only tested for
            // same-creator; cross-creator race condition not exercised in CI.
            // WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P3-S2
            // — failure-injection test gap: no integration test exercises the
            // insert-key-block failure path in the extract pipeline.
            nexus_local_db::next_queued_extract_job(pool, creator_id)
                .await
                .map_err(|e| CapabilityError::Internal(format!("Failed to claim job: {e}")))?
                .ok_or_else(|| {
                    CapabilityError::InputInvalid(
                        "No queued extract jobs available for this creator".into(),
                    )
                })?
        };

        let job_id = job.job_id.clone();
        // The job row is the authority for the extraction target (v1.191 P1
        // T13, durable §8): the stored target is what the run is admitted for,
        // so a caller that omits `world_id` cannot make the capability skip a
        // World-bound job. An explicit `world_id` stays an override, and the
        // stored-state recheck still refuses a foreign World.
        let world_id = input
            .get("world_id")
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
            .unwrap_or(&job.world_id)
            .to_string();
        let work_entry_id = input
            .get("work_entry_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&job.work_entry_id)
            .to_string();

        // QC1/2 W-002: a worldless Work (legacy V1.39 job with no World to
        // promote into) is a success no-op so the preset state machine can
        // cleanly transition to done.
        if world_id.is_empty() {
            return Ok(json!({
                "job_id": job_id,
                "status": "skipped",
                "reason": "world_id absent — worldless Work, no KB extraction needed"
            }));
        }

        // ── Phase 2: Load work content ──────────────────────────────
        let work_content = input
            .get("work_content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if work_content.is_empty() {
            // Return prompt-phase data so the outer flow can load content
            // and call acp.prompt before finalizing.
            return Ok(json!({
                "job_id": job_id,
                "status": "running",
                "creator_id": creator_id,
                "world_id": world_id,
                "work_entry_id": work_entry_id,
                "prompt_length": 0,
                "needs_content": true
            }));
        }

        // ── Phase 3: Check for LLM response ─────────────────────────
        let llm_response = input.get("llm_response").and_then(|v| v.as_str());

        let Some(response_text) = llm_response else {
            // Build and return the extraction prompt for acp.prompt execution
            let prompt = extraction_prompt(work_content);
            let prompt_length = prompt.len();
            return Ok(json!({
                "job_id": job_id,
                "status": "running",
                "creator_id": creator_id,
                "world_id": world_id,
                "work_entry_id": work_entry_id,
                "prompt": prompt,
                "prompt_length": prompt_length
            }));
        };

        // ── Phase 4: Parse LLM response → prepared candidate via the wrapper ────
        let extract = match parse_extraction_response(response_text) {
            Ok(resp) => resp,
            Err(e) => {
                // Mark job as failed
                let _ = nexus_local_db::mark_extract_job_failed(
                    pool,
                    &job_id,
                    &format!("LLM response parse error: {e}"),
                )
                .await;
                return Err(e);
            }
        };

        let block_type = match parse_block_type(&extract.block_type) {
            Ok(bt) => bt,
            Err(e) => {
                let _ = nexus_local_db::mark_extract_job_failed(
                    pool,
                    &job_id,
                    &format!("Invalid block type: {e}"),
                )
                .await;
                return Err(e);
            }
        };

        // Determine validation mode from profile_hint (V1.40 P3).
        let profile_hint = input
            .get("profile_hint")
            .and_then(|v| v.as_str())
            .unwrap_or("generic");
        let validation_mode = if profile_hint == "novel" {
            nexus_knowledge::world_kb::ValidationMode::Novel
        } else {
            nexus_knowledge::world_kb::ValidationMode::Generic
        };

        // Build body from LLM response.
        let body: nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody =
            if let Ok(parsed) = serde_json::from_str(&extract.body) {
                parsed
            } else {
                nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
                    summary: Some(extract.body.clone()),
                    attributes: None,
                    tags: None,
                    ..Default::default()
                }
            };

        // The trusted job target policy (durable §§3, 8): the job's stored
        // controlling Creator must still own the target World, and the audience
        // is the service's in-scope default. Resolved before the model's
        // candidate can name anything — the model never reaches this pair.
        let target =
            match crate::quality_loop::resolve_extraction_target(pool, &job.creator_id, &world_id)
                .await
            {
                Ok(target) => target,
                Err(e) => {
                    let _ = nexus_local_db::mark_extract_job_failed(
                        pool,
                        &job_id,
                        &format!("Extraction target refused: {e}"),
                    )
                    .await;
                    return Err(e);
                }
            };

        // The admitted source bundle and the source anchor both come from the
        // actual Work artifact: the text the orchestrator loaded plus the job's
        // stored artifact locator. Never the model's own prose, never empty.
        let source_id = job
            .source_locator
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| work_entry_id.clone());
        let anchor_excerpt = {
            let admitted = work_content.trim();
            if admitted.is_empty() {
                source_id.clone()
            } else {
                admitted.chars().take(SOURCE_ANCHOR_EXCERPT_CHARS).collect()
            }
        };
        let request: nexus_spoke_adapter::ExtractRequest = serde_json::from_value(json!({
            "run_id": run_id,
            "sources": [{
                "schema_version": 1,
                "source_id": source_id,
                "extensions": {},
            }],
        }))
        .map_err(|e| CapabilityError::Internal(format!("extract request refused: {e}")))?;
        let bundle = json!({
            "work_entry_id": work_entry_id,
            "source_kind": job.source_kind,
            "source_locator": job.source_locator,
            "text": work_content,
        });

        // The native extractor callback prepares the model's candidate under the
        // trusted target policy; the adapter converts it to the wire candidate
        // and the upstream orchestrator validates the run. The callback crosses
        // to the adapter's driver thread, so its failure travels back through
        // this slot.
        let failure: Arc<std::sync::Mutex<Option<CapabilityError>>> =
            Arc::new(std::sync::Mutex::new(None));
        let outcome = {
            let failure = Arc::clone(&failure);
            let target = target.clone();
            let world_id = world_id.clone();
            let canonical_name = extract.canonical_name.clone();
            let body = body.clone();
            let anchor_excerpt = anchor_excerpt.clone();
            nexus_spoke_adapter::extract_candidates(
                request,
                nexus_spoke_adapter::ResolvedExtractionInput {
                    run_id: run_id.to_string(),
                    bundle,
                },
                move |run_input| async move {
                    // The admitted bundle is the only source the extractor sees.
                    if run_input.input["text"].as_str().is_none() {
                        *failure
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some(CapabilityError::Internal(
                                "admitted extraction bundle carries no source text".to_string(),
                            ));
                        return nexus_spoke_adapter::SpokeResult::Reject(extract_reject());
                    }
                    match nexus_knowledge::world_kb::prepare_extract(
                        nexus_knowledge::world_kb::ExtractPrepareInput {
                            world_id: world_id.clone(),
                            block_type,
                            canonical_name: canonical_name.clone(),
                            body: body.clone(),
                            source_anchor:
                                nexus_knowledge::world_kb::source_anchor::SourceAnchor::from_excerpt(
                                    &anchor_excerpt,
                                ),
                            validation_mode,
                            governance: target.governance.clone(),
                        },
                    ) {
                        Ok(prepared) => nexus_spoke_adapter::SpokeResult::Ok(
                            nexus_spoke_adapter::NativeExtractionOutput {
                                candidates: vec![prepared],
                                method: Some("kb.extract_work".to_string()),
                                coverage_hint: None,
                                // This path's prompt extracts a single KE and
                                // proposes no relationships; SPOKE's success arm
                                // is KE-only, so the sidecar is carried (never
                                // dropped) and stays empty here.
                                relationships: Vec::new(),
                            },
                        ),
                        Err(e) => {
                            *failure
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(CapabilityError::Internal(format!(
                                    "KnowledgeEntryRecord prepare failed: {e}"
                                )));
                            nexus_spoke_adapter::SpokeResult::Reject(extract_reject())
                        }
                    }
                },
            )
            .await
        };

        // A callback failure is the precise error, not the placeholder reject.
        let callback_failure = failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(e) = callback_failure {
            let _ = nexus_local_db::mark_extract_job_failed(pool, &job_id, &e.to_string()).await;
            return Err(e);
        }
        let outcome = match outcome {
            nexus_spoke_adapter::SpokeResult::Ok(outcome) => outcome,
            nexus_spoke_adapter::SpokeResult::Reject(reject) => {
                let reason = format!(
                    "ke-extraction refused the run: {}: {}",
                    reject.code.as_str(),
                    reject.message
                );
                let _ = nexus_local_db::mark_extract_job_failed(pool, &job_id, &reason).await;
                return Err(CapabilityError::PermanentExternal(reason));
            }
        };
        if !outcome.relationships.is_empty() {
            tracing::debug!(
                job_id,
                relationships = outcome.relationships.len(),
                "kb-extract: extraction returned a relationship sidecar"
            );
        }

        // ── Phase 4b: Persist the validated candidates BEFORE marking done ─────
        // The candidates are persisted exactly as the protocol validated them
        // (same ids, same governance); only then is the job flipped to done, so
        // a refused/failed run can never leave a done job without its entry.
        let store = nexus_local_db::kb_store::SqliteKbStore::new(pool.as_ref().clone());
        let mut insert_result = None;
        for candidate in outcome.candidates {
            match nexus_knowledge::world_kb::persist_prepared_extract(&store, candidate).await {
                Ok(r) => insert_result = Some(r),
                Err(e) => {
                    // Mark job as failed so the content loss window is closed.
                    let _ = nexus_local_db::mark_extract_job_failed(
                        pool,
                        &job_id,
                        &format!("KnowledgeEntryRecord insert failed: {e}"),
                    )
                    .await;
                    return Err(CapabilityError::Internal(format!(
                        "KnowledgeEntryRecord insert failed: {e}"
                    )));
                }
            }
        }
        let Some(insert_result) = insert_result else {
            let reason = "ke-extraction accepted the run without a candidate".to_string();
            let _ = nexus_local_db::mark_extract_job_failed(pool, &job_id, &reason).await;
            return Err(CapabilityError::Internal(reason));
        };

        // Mark done only after the KnowledgeEntryRecord was successfully inserted.
        nexus_local_db::mark_extract_job_done(pool, &job_id)
            .await
            .map_err(|e| CapabilityError::Internal(format!("Failed to mark job done: {e}")))?;

        Ok(json!({
            "job_id": job_id,
            "status": "done",
            "key_block_id": insert_result.entry_id,
            "owner": insert_result.owner,
            "block_type": extract.block_type,
            "canonical_name": extract.canonical_name,
            "created_at": insert_result.created_at
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kb_extract_work_name() {
        assert_eq!(KbExtractWork::new().name(), "kb.extract_work");
    }

    #[test]
    fn test_parse_extraction_response_valid() {
        let json = r#"{"block_type": "Character", "canonical_name": "Elena", "body": "A brave warrior", "source_work_entry_id": "kb_abc"}"#;
        let resp = parse_extraction_response(json).unwrap();
        assert_eq!(resp.block_type, "Character");
        assert_eq!(resp.canonical_name, "Elena");
        assert_eq!(resp.body, "A brave warrior");
        assert_eq!(resp.source_work_entry_id, "kb_abc");
    }

    #[test]
    fn test_parse_extraction_response_with_fences() {
        let json = "```json\n{\"block_type\": \"Scene\", \"canonical_name\": \"Mountains\", \"body\": \"Cold peaks\", \"source_work_entry_id\": \"\"}\n```";
        let resp = parse_extraction_response(json).unwrap();
        assert_eq!(resp.block_type, "Scene");
        assert_eq!(resp.body, "Cold peaks");
    }

    #[test]
    fn test_parse_extraction_response_invalid() {
        let result = parse_extraction_response("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_extraction_prompt_contains_content() {
        let prompt = extraction_prompt("Hello world");
        assert!(prompt.contains("Hello world"));
        assert!(prompt.contains("block_type"));
    }

    #[test]
    fn test_parse_block_type_all_variants() {
        assert!(parse_block_type("Character").is_ok());
        assert!(parse_block_type("Ability").is_ok());
        assert!(parse_block_type("Scene").is_ok());
        assert!(parse_block_type("Organization").is_ok());
        assert!(parse_block_type("Item").is_ok());
        assert!(parse_block_type("Conflict").is_ok());
        assert!(parse_block_type("InfoPoint").is_ok());
        assert!(parse_block_type("Event").is_ok());
        assert!(parse_block_type("Unknown").is_err());
    }

    #[test]
    fn test_default_creates_no_pool() {
        let cap = KbExtractWork::default();
        assert!(cap.pool.is_none());
    }

    // ── v1.191 P1 T13: real caller path through the adapter wrapper ────────

    const CREATOR: &str = "ctr_t13";
    const WORLD: &str = "wld_t13";

    /// A guarded engine pool with one World owned by `CREATOR`.
    async fn test_pool_with_world() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pool = nexus_local_db::init_engine_pool(&dir.path().join("t13.db"))
            .await
            .unwrap()
            .clone_pool();
        nexus_local_db::kb_store::seed::world(
            &pool,
            WORLD,
            CREATOR,
            "T13 World",
            "t13",
            "private",
            "manual",
        )
        .await;
        (pool, dir)
    }

    /// Enqueue one queued extract job for the seeded World.
    async fn enqueue_job(pool: &sqlx::SqlitePool) -> String {
        nexus_local_db::enqueue_extract_job_with_artifact(
            pool,
            CREATOR,
            "ws_t13",
            "kb_work_t13",
            WORLD,
            Some("work_chapter"),
            Some("Works/t13/Stories/ch03.md"),
            Some("novel"),
            Some("wrk_t13"),
        )
        .await
        .unwrap()
        .job_id
    }

    /// A model response for one novel-profile candidate.
    fn model_response(attributes: serde_json::Value) -> String {
        json!({
            "block_type": "character",
            "canonical_name": "char_lin_xia",
            "body": { "summary": "A warrior", "attributes": attributes, "tags": ["novel"] },
            "source_work_entry_id": "kb_work_t13",
        })
        .to_string()
    }

    fn finalize_input(job_id: &str) -> Value {
        json!({
            "creator_id": CREATOR,
            "job_id": job_id,
            "work_content": "Lin Xia drew her blade at the Azure Gate.",
            "llm_response": model_response(json!({ "novel_category": "character" })),
            "_session_id": "run_t13",
        })
    }

    /// Rows in the World KB of the seeded World.
    async fn world_rows(
        pool: &sqlx::SqlitePool,
    ) -> Vec<nexus_knowledge::world_kb::KnowledgeEntryRecord> {
        use nexus_knowledge::world_kb::store::KbStore;
        let store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        store.list_by_world(WORLD).await.unwrap()
    }

    async fn job_status(pool: &sqlx::SqlitePool, job_id: &str) -> String {
        nexus_local_db::get_extract_job(pool, job_id)
            .await
            .unwrap()
            .expect("job row")
            .status
    }

    #[tokio::test]
    async fn v1191_extract_success_persists_the_validated_candidate_and_completes_the_job() {
        let (pool, _dir) = test_pool_with_world().await;
        let job_id = enqueue_job(&pool).await;
        let cap = KbExtractWork::with_pool(pool.clone());

        let result = cap.run(finalize_input(&job_id)).await.unwrap();

        assert_eq!(result["status"], "done");
        let entry_id = result["key_block_id"].as_str().expect("entry id");
        assert!(entry_id.starts_with("kb_"), "candidate id: {entry_id}");
        assert_eq!(result["canonical_name"], "char_lin_xia");
        assert_eq!(job_status(&pool, &job_id).await, "done");

        let rows = world_rows(&pool).await;
        assert_eq!(rows.len(), 1, "exactly the validated candidate");
        assert_eq!(rows[0].entry_id, entry_id, "same id the job reported");
        // The trusted job target policy (an in-scope World container) resolved
        // to shared: neither governance column is written.
        assert_eq!(rows[0].holder_entry_id, None);
        assert_eq!(rows[0].disclosure, None);
        // The source anchor comes from the admitted Work artifact, not the
        // model's own prose.
        let anchor = rows[0].source_anchor.as_ref().expect("source anchor");
        assert_eq!(
            anchor.excerpt.as_deref(),
            Some("Lin Xia drew her blade at the Azure Gate.")
        );
    }

    #[tokio::test]
    async fn v1191_extract_model_output_cannot_choose_governance() {
        let (pool, _dir) = test_pool_with_world().await;
        let job_id = enqueue_job(&pool).await;
        let cap = KbExtractWork::with_pool(pool.clone());

        // The model tries to make the row owner-private under its own holder;
        // the trusted target policy decides instead.
        let input = json!({
            "creator_id": CREATOR,
            "job_id": job_id,
            "work_content": "Lin Xia drew her blade.",
            "llm_response": model_response(json!({
                "novel_category": "character",
                "holder_entry_id": "hld_model_choice",
                "disclosure": "owner-private",
            })),
            "_session_id": "run_t13",
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["status"], "done");

        let rows = world_rows(&pool).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].holder_entry_id, None,
            "holder must not be model-chosen"
        );
        assert_eq!(
            rows[0].disclosure, None,
            "disclosure must not be model-chosen"
        );
    }

    #[tokio::test]
    async fn v1191_extract_invalid_terminal_writes_nothing_and_fails_the_job() {
        let (pool, _dir) = test_pool_with_world().await;
        let job_id = enqueue_job(&pool).await;
        let cap = KbExtractWork::with_pool(pool.clone());

        let input = json!({
            "creator_id": CREATOR,
            "job_id": job_id,
            "work_content": "Lin Xia drew her blade.",
            "llm_response": "{\"block_type\": \"character\", \"canonical_name\":",
            "_session_id": "run_t13",
        });
        let error = cap
            .run(input)
            .await
            .expect_err("incomplete terminal must fail");

        assert!(error.to_string().contains("parse"), "error: {error}");
        assert!(world_rows(&pool).await.is_empty(), "zero candidate writes");
        assert_eq!(
            job_status(&pool, &job_id).await,
            "failed",
            "no successful job terminal"
        );
    }

    #[tokio::test]
    async fn v1191_extract_protocol_failure_writes_nothing_and_fails_the_job() {
        let (pool, _dir) = test_pool_with_world().await;
        let job_id = enqueue_job(&pool).await;
        let cap = KbExtractWork::with_pool(pool.clone());

        // The model output names a candidate the domain rules refuse (a path
        // separator in the canonical name), so the native extractor rejects and
        // the run never reaches persistence.
        let input = json!({
            "creator_id": CREATOR,
            "job_id": job_id,
            "work_content": "Lin Xia drew her blade.",
            "llm_response": json!({
                "block_type": "character",
                "canonical_name": "char/lin*xia",
                "body": { "summary": "A warrior", "attributes": { "novel_category": "character" } },
                "source_work_entry_id": "kb_work_t13",
            })
            .to_string(),
            "_session_id": "run_t13",
        });
        let error = cap.run(input).await.expect_err("a refused run must fail");

        assert!(
            error.to_string().contains("prepare"),
            "expected the extractor failure, got: {error}"
        );
        assert!(world_rows(&pool).await.is_empty(), "zero candidate writes");
        assert_eq!(
            job_status(&pool, &job_id).await,
            "failed",
            "no success terminal"
        );
    }

    #[tokio::test]
    async fn v1191_extract_stale_target_writes_nothing_and_fails_the_job() {
        let (pool, _dir) = test_pool_with_world().await;
        let job_id = enqueue_job(&pool).await;
        let cap = KbExtractWork::with_pool(pool.clone());

        // The stored controlling Creator no longer matches the job's: the
        // trusted target refuses before anything is written.
        sqlx::query("UPDATE kb_extract_jobs SET creator_id = ? WHERE job_id = ?")
            .bind("ctr_other")
            .bind(&job_id)
            .execute(&pool)
            .await
            .unwrap();

        let mut input = finalize_input(&job_id);
        input["creator_id"] = json!("ctr_other");
        let error = cap
            .run(input)
            .await
            .expect_err("a stale target must refuse");

        assert!(error.to_string().contains("not owned"), "error: {error}");
        assert!(world_rows(&pool).await.is_empty(), "zero candidate writes");
        assert_eq!(
            job_status(&pool, &job_id).await,
            "failed",
            "no success terminal"
        );
    }

    #[tokio::test]
    async fn v1191_extract_missing_run_identity_refuses_before_touching_the_job() {
        let (pool, _dir) = test_pool_with_world().await;
        let job_id = enqueue_job(&pool).await;
        let cap = KbExtractWork::with_pool(pool.clone());

        let mut input = finalize_input(&job_id);
        input.as_object_mut().unwrap().remove("_session_id");
        let error = cap
            .run(input)
            .await
            .expect_err("no run identity must refuse");

        assert!(error.to_string().contains("_session_id"), "error: {error}");
        assert_eq!(
            job_status(&pool, &job_id).await,
            "queued",
            "the job is untouched"
        );
        assert!(world_rows(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn v1191_extract_interrupted_finalize_leaves_zero_candidates_and_no_success_terminal() {
        let (pool, _dir) = test_pool_with_world().await;
        let job_id = enqueue_job(&pool).await;
        let cap = KbExtractWork::with_pool(pool.clone());

        // Hold the write lock so the finalize's candidate insert can never
        // commit, then drop the run future on the timeout: a run that cannot
        // commit must leave zero candidate rows and no `done` terminal.
        let mut blocker = nexus_local_db::begin_immediate(&pool).await.unwrap();
        sqlx::query("UPDATE kb_extract_jobs SET status = status WHERE job_id = ?")
            .bind(&job_id)
            .execute(&mut *blocker)
            .await
            .unwrap();

        let interrupted = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            cap.run(finalize_input(&job_id)),
        )
        .await;
        drop(blocker);

        assert!(
            world_rows(&pool).await.is_empty(),
            "an interrupted finalize writes no candidate (outcome: {interrupted:?})"
        );
        assert_ne!(
            job_status(&pool, &job_id).await,
            "done",
            "no successful job terminal without a persisted candidate"
        );
    }
}

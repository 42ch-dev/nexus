//! `kb.extract_work` capability — extract a `KeyBlock` from a work-scope KB entry.
//!
//! Full e2e pipeline (R16):
//! 1. When `job_id` present: load job row; reject wrong status.
//! 2. When `job_id` omitted: call `claim_job` for `creator_id`.
//! 3. Mark running → load work content → build extraction prompt → parse
//!    LLM response → mark done → insert `KeyBlock` via `SqliteKbStore`.
//!
//! The capability is stateful: it holds an `Option<SqlitePool>` for job
//! lifecycle management and `KeyBlock` insertion. Without a pool it returns
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

/// The `kb.extract_work` capability.
///
/// Holds an optional `SqlitePool` for job lifecycle management and `KeyBlock`
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
/// - **Finalize phase** (with `llm_response`): returns `KeyBlock` insert result
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
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["creator_id"],
            "properties": {
                "job_id": { "type": "string", "description": "Existing extract job ID" },
                "work_entry_id": { "type": "string", "description": "Work-scope KB entry ID to extract" },
                "world_id": { "type": "string", "description": "Target world ID for the resulting KeyBlock" },
                "work_id": { "type": "string", "description": "Source work ID (parent of the chapter)" },
                "work_content": { "type": "string", "description": "Pre-loaded work content" },
                "creator_id": { "type": "string", "description": "Creator ID" },
                "llm_response": { "type": "string", "description": "LLM response text from acp.prompt for finalizing" },
                "source_kind": { "type": "string", "description": "Artifact kind (work_chapter, work_section, etc.)" },
                "source_locator": { "type": "string", "description": "Artifact locator (relative path)" },
                "profile_hint": { "type": "string", "description": "Extract profile (novel, screenplay, essay, generic)" }
            }
        }"#
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

        // ── Phase 1: Load or claim job ──────────────────────────────
        let job = if let Some(job_id) = input.get("job_id").and_then(|v| v.as_str()) {
            // Load specific job
            let job = nexus_local_db::get_extract_job(pool, job_id)
                .await
                .map_err(|e| CapabilityError::Internal(format!("Failed to load job: {e}")))?
                .ok_or_else(|| {
                    CapabilityError::InputInvalid(format!("Job '{job_id}' not found"))
                })?;

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
        let world_id = input
            .get("world_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&job.world_id)
            .to_string();
        let work_entry_id = input
            .get("work_entry_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&job.work_entry_id)
            .to_string();

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

        // ── Phase 4: Parse LLM response → KeyBlock insert ───────────
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
            nexus_kb::ValidationMode::Novel
        } else {
            nexus_kb::ValidationMode::Generic
        };

        // Build body from LLM response.
        let body: nexus_kb::key_block::KeyBlockBody =
            if let Ok(parsed) = serde_json::from_str(&extract.body) {
                parsed
            } else {
                nexus_kb::key_block::KeyBlockBody {
                    summary: Some(extract.body.clone()),
                    attributes: None,
                    tags: None,
                }
            };

        // Build source anchor from artifact locator.
        let source_locator = input
            .get("source_locator")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let source_anchor = if source_locator.is_empty() {
            nexus_kb::source_anchor::SourceAnchor::from_excerpt(
                &extract.body.chars().take(256).collect::<String>(),
            )
        } else {
            nexus_kb::source_anchor::SourceAnchor::from_excerpt(source_locator)
        };

        let finalize_input = nexus_kb::ExtractFinalizeInput {
            world_id: world_id.clone(),
            block_type,
            canonical_name: extract.canonical_name.clone(),
            body,
            source_anchor,
            validation_mode,
        };

        // ── Phase 4b: Mark job as done BEFORE inserting KeyBlock ───────
        // This ordering ensures: if mark_done fails, no KeyBlock was created
        // and the job can be safely retried. A "done" job without a KeyBlock
        // is recoverable; a "running" job with an orphaned KeyBlock is not.
        nexus_local_db::mark_extract_job_done(pool, &job_id)
            .await
            .map_err(|e| CapabilityError::Internal(format!("Failed to mark job done: {e}")))?;

        // ── Phase 5: Insert KeyBlock via T3 finalize helper ────────────
        let store = nexus_local_db::kb_store::SqliteKbStore::new(pool.as_ref().clone());
        let insert_result = nexus_kb::finalize_extract(&store, finalize_input)
            .await
            .map_err(|e| {
                tracing::error!(
                    job_id = %job_id,
                    error = %e,
                    "KeyBlock insert failed after job marked done — extraction content lost"
                );
                CapabilityError::Internal(format!("KeyBlock insert failed: {e}"))
            })?;

        Ok(json!({
            "job_id": job_id,
            "status": "done",
            "key_block_id": insert_result.key_block_id,
            "world_id": insert_result.world_id,
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
}

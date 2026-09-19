//! `nexus.llm.extract` capability — extract World KB candidates from chapter
//! prose using a judge-style agent call.
//!
//! Design: `.mstar/specs/llm-extract.md`, compass §0.1 #7.
//!
//! Sibling to `judge.llm`: both reuse the production prompt executor. Where
//! `judge.llm` emits a GO/NOGO verdict, `nexus.llm.extract` emits a
//! `candidates` array of World KB candidates carrying an agent-judged
//! `block_type`, `canonical_name`, `confidence`, and a verbatim
//! `source_quote`.
//!
//! Two execution modes (mirrors `judge.llm`):
//! 1. **With prompt executor**: builds an extraction prompt, executes it with
//!    `deny_all` tool policy (extraction is read-only), parses the agent
//!    response JSON into `candidates`.
//! 2. **Standalone (no executor)**: returns [`CapabilityError::WorkerUnavailable`]
//!    — no heuristic fallback inside the capability (the caller's review-time
//!    hook owns the fallback decision; see `quality_loop`).

use crate::capability::{Capability, CapabilityError, PromptExecutor, PromptRequest, ToolPolicy};
use crate::quality_loop::ExtractionTarget;
use async_trait::async_trait;
use nexus_knowledge::world_kb::source_anchor::SourceAnchor;
use nexus_knowledge::world_kb::{
    prepare_extract, ExtractPrepareInput, KnowledgeEntryBody, PreparedExtractCandidate,
    ValidationMode,
};
use nexus_spoke_adapter::{
    extract_candidates, ExtractRequest, NativeExtractionOutput, ResolvedExtractionInput,
    SpokeReject, SpokeRejectCode, SpokeResult,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// What the native callback hands back across the adapter's driver thread.
///
/// The callback cannot borrow the capability's stack (it crosses threads), so
/// both the precise failure and the local-shape candidates travel here.
#[derive(Debug, Default)]
struct ExtractionSlot {
    /// The exact error the extractor produced, when it failed.
    failure: Option<CapabilityError>,
    /// The local result shape, projected from the candidates the protocol
    /// accepted.
    candidates: Vec<Value>,
}

/// Parse the trusted `_extract_target` context field.
///
/// Fails closed: a missing target refuses the run instead of extracting under
/// an invented policy.
fn parse_extract_target(input: &Value) -> Result<ExtractionTarget, CapabilityError> {
    let raw = input.get("_extract_target").ok_or_else(|| {
        CapabilityError::Forbidden(
            "missing trusted _extract_target: the orchestration caller must resolve the \
             extraction job/task target policy"
                .to_string(),
        )
    })?;
    ExtractionTarget::from_context(raw)
}

/// Prepare one model-judged candidate under the trusted target policy.
///
/// Returns `None` — drop + log, exactly like the pre-wrapper pathway dropped
/// nameless candidates — when the model output cannot become a candidate at
/// all: no name, an unknown wire `block_type`, or a name/body the domain rules
/// refuse. The governance pair is never read from `raw`.
fn prepare_candidate(
    raw: &Value,
    target: &ExtractionTarget,
    admitted_text: &str,
    source_id: &str,
) -> Option<PreparedExtractCandidate> {
    let canonical_name = raw
        .get("canonical_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if canonical_name.is_empty() {
        return None;
    }
    let block_type_raw = raw
        .get("block_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let Ok(block_type) =
        serde_json::from_value::<nexus_contracts::BlockType>(json!(block_type_raw))
    else {
        tracing::warn!(
            block_type = %block_type_raw,
            "nexus.llm.extract: unknown wire block_type; dropping the candidate"
        );
        return None;
    };
    // The anchor comes from the actual artifact: the model's verbatim chapter
    // quote when it gave one, else the admitted chapter text it read, else the
    // admitted source's own identity. Never empty.
    let source_quote = raw
        .get("source_quote")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty());
    let anchor_text = source_quote.unwrap_or_else(|| {
        let admitted = admitted_text.trim();
        if admitted.is_empty() {
            source_id
        } else {
            admitted
        }
    });
    let body = KnowledgeEntryBody {
        summary: raw
            .get("summary")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        ..Default::default()
    };

    match prepare_extract(ExtractPrepareInput {
        world_id: target.world_id.clone(),
        block_type,
        canonical_name: canonical_name.clone(),
        body,
        source_anchor: SourceAnchor::from_excerpt(
            &anchor_text.chars().take(256).collect::<String>(),
        ),
        validation_mode: ValidationMode::Generic,
        governance: target.governance.clone(),
    }) {
        Ok(prepared) => Some(prepared),
        Err(e) => {
            tracing::warn!(
                canonical_name = %canonical_name,
                error = %e,
                "nexus.llm.extract: model candidate refused by the domain rules; dropping it"
            );
            None
        }
    }
}

/// Project one prepared candidate back into the capability's local result
/// shape, keeping the id the protocol validated.
///
/// The review-time caller reads `canonical_name` / `block_type` / `summary` /
/// `confidence` / `source_quote`; `entry_id` is the prepared candidate id the
/// `ke-extraction` response echoed. `canonical_name` and `block_type` come from
/// the prepared record itself, so the local shape cannot diverge from the
/// candidate the protocol accepted.
fn local_candidate(raw: &Value, prepared: &PreparedExtractCandidate) -> Value {
    let record = &prepared.record;
    json!({
        "canonical_name": record.canonical_name,
        "block_type": serde_json::to_value(record.block_type).unwrap_or(Value::Null),
        "summary": record.body.as_ref().and_then(|b| b.summary.clone()),
        "confidence": raw.get("confidence").cloned().unwrap_or(Value::Null),
        "source_quote": raw.get("source_quote").cloned().unwrap_or(Value::Null),
        "entry_id": record.entry_id,
    })
}

/// The `nexus.llm.extract` capability.
///
/// Holds an optional [`PromptExecutor`] for agent calls. When present, sends
/// the extraction prompt + chapter prose via the Host plane. When absent,
/// returns [`CapabilityError::WorkerUnavailable`] (standalone/test mode).
pub struct LlmExtract {
    executor: Option<Arc<dyn PromptExecutor>>,
    /// Per-run coordinator cancellation tokens (A1): the executor listens to
    /// the token for the current run concurrently with the Host stream.
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
}

impl LlmExtract {
    /// Create in standalone/test mode (no prompt executor).
    #[must_use]
    pub fn new() -> Self {
        Self {
            executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Create with a prompt executor for production Host dispatch.
    #[must_use]
    pub fn with_prompt_executor(executor: Arc<dyn PromptExecutor>) -> Self {
        Self {
            executor: Some(executor),
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Builder-style per-run coordinator cancellation tokens (A1).
    #[must_use]
    pub fn with_session_cancels(
        mut self,
        session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    ) -> Self {
        self.session_cancels = session_cancels;
        self
    }
}

impl Default for LlmExtract {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for LlmExtract {
    fn name(&self) -> &'static str {
        "nexus.llm.extract"
    }

    // Identity fields ("_creator_id", "_session_id") are injected by
    // orchestration context, NOT accepted from user input (security:
    // prevents cross-creator routing — SEC-V131-01, same rule as
    // judge.llm). `_extract_target` / `_extract_source_id` are the same kind
    // of trusted context: the caller's stored-state-resolved target policy and
    // the admitted chapter artifact (v1.191 P1 T13).
    fn input_schema(&self) -> &'static str {
        nexus_preset::capability_catalog::NEXUS_LLM_EXTRACT_INPUT_SCHEMA
    }

    fn output_schema(&self) -> &'static str {
        r#"{
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["candidates"],
        "properties": {
            "candidates": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["canonical_name", "block_type", "confidence", "source_quote"],
                    "properties": {
                        "canonical_name": { "type": "string" },
                        "block_type": { "type": "string" },
                        "summary": { "type": ["string", "null"] },
                        "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "source_quote": { "type": "string" }
                    }
                }
            },
            "relationships": {
                "type": "array",
                "description": "V1.76: optional relationship candidates proposed from chapter prose. Missing/empty array means no relationship candidates (backward compatible).",
                "items": {
                    "type": "object",
                    "required": ["source_canonical_name", "target_canonical_name", "relation_type", "symmetric", "confidence", "source_quote"],
                    "properties": {
                        "source_canonical_name": { "type": "string" },
                        "source_block_type": { "type": ["string", "null"] },
                        "target_canonical_name": { "type": "string" },
                        "target_block_type": { "type": ["string", "null"] },
                        "relation_type": { "type": "string", "description": "WorldKbRelationshipKind snake_case value; 'custom' requires custom_label" },
                        "custom_label": { "type": ["string", "null"] },
                        "symmetric": { "type": "boolean" },
                        "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "source_quote": { "type": "string" }
                    }
                }
            }
        }
    }"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let prompt_text = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'prompt' field".into()))?;
        let chapter_prose = input
            .get("chapter_prose")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'chapter_prose' field".into()))?;

        // Security: only accept context-injected identity fields (prefixed _).
        // Raw `creator_id`/`session_id` from user/preset input are ignored
        // to prevent cross-creator routing (IDOR). See SEC-V131-01.
        //
        // M-002: a missing trusted `_session_id` refuses with a typed error
        // — never a magic `default` run id. The orchestration engine seeds
        // the trusted `_session_id` at run admission; its absence means the
        // capability is being invoked outside a trusted run context.
        let session_id = input
            .get("_session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CapabilityError::Forbidden(
                "missing trusted _session_id: orchestration context must inject the run identity"
                    .to_string(),
            )
            })?;

        // The callback crosses to the adapter's driver thread, so the executor
        // handle it uses must be owned (`Arc`), not borrowed from `&self`.
        let executor = Arc::clone(
            self.executor
                .as_ref()
                .ok_or(CapabilityError::WorkerUnavailable)?,
        );

        // A1: resolve the coordinator cancellation token for this run from
        // the shared per-run map. FAIL-CLOSED: a run with no registered
        // token refuses with `CancellationUnavailable` — a fresh token would
        // be uncancellable by any coordinator (never mint one here). The run
        // admission path (engine start/spawn/recovery) registers the token.
        let cancellation =
            crate::capability::resolve_session_cancellation(&self.session_cancels, session_id)?;

        // v1.191 P1 T13 (durable §8): the trusted job/task target policy and
        // the admitted source identity. Both arrive as trusted context from
        // the orchestration caller; neither is model output.
        let target = parse_extract_target(&input)?;
        let source_id = input
            .get("_extract_source_id")
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                CapabilityError::Forbidden(
                    "missing trusted _extract_source_id: the admitted chapter artifact must be \
                     named"
                        .to_string(),
                )
            })?;

        // The wire request: the real run identity plus the actual chapter
        // artifact as its single admitted source. `ExtractRequest.run_id`
        // rejects an empty id at deserialize, so a run without a stored
        // identity can never reach the orchestrator.
        let request: ExtractRequest = serde_json::from_value(json!({
            "run_id": session_id,
            "sources": [{
                "schema_version": 1,
                "source_id": source_id,
                "extensions": {},
            }],
        }))
        .map_err(|e| CapabilityError::Forbidden(format!("extract request refused: {e}")))?;
        // The admitted, bounded native source bundle the run resolves over.
        // The adapter hands exactly this back to the orchestrator; it performs
        // no source I/O.
        let admitted = json!({
            "container": target.world_id,
            "sources": [{ "source_id": source_id, "text": chapter_prose }],
        });

        // Build the extraction prompt: instruction + verbatim prose, framed so
        // the agent returns a JSON object with a `candidates` array (entities)
        // and an optional `relationships` array (V1.76). deny_all tool policy —
        // extraction is read-only, no tools, no side-effect.
        let extract_prompt = format!(
            "{prompt_text}\n\n\
         Return ONLY a JSON object of the form {{\"candidates\": [{{\"canonical_name\": \
         string, \"block_type\": one of [character, ability, scene, organization, item, \
         conflict, info_point, event], \"summary\": string|null, \"confidence\": number \
         in [0.0,1.0], \"source_quote\": string}}], \"relationships\": [{{\
         \"source_canonical_name\": string, \"source_block_type\": block_type|null, \
         \"target_canonical_name\": string, \"target_block_type\": block_type|null, \
         \"relation_type\": one of [allied_with, rival_of, mentor_of, parent_of, child_of, \
         member_of, located_in, created_by, rules_over, custom], \"custom_label\": \
         string|null (required when relation_type is custom), \"symmetric\": boolean, \
         \"confidence\": number in [0.0,1.0], \"source_quote\": string}}]}}. \
         Use the wire `block_type` and `relation_type` enums (snake_case). \
         `source_quote` MUST be a verbatim excerpt from the chapter. The \
         `relationships` array MAY be empty when no relationships are evident.\n\n\
         CHAPTER PROSE:\n{chapter_prose}"
        );

        // The native extractor callback: it runs the real prompt through the
        // host over the admitted source, then prepares every candidate under
        // the trusted target policy. The callback crosses to the adapter's
        // driver thread, so its outcome crosses back through this slot: the
        // precise capability error (a `WorkerUnavailable` must stay
        // distinguishable from a refusal) and the local-shape candidates.
        let slot = Arc::new(std::sync::Mutex::new(ExtractionSlot::default()));
        let run_id = session_id.to_string();
        let outcome = {
            let slot = Arc::clone(&slot);
            let target = target.clone();
            let source_id = source_id.to_string();
            extract_candidates(
                request,
                ResolvedExtractionInput {
                    run_id: run_id.clone(),
                    bundle: admitted,
                },
                move |run_input| async move {
                    // The admitted bundle is the only source the extractor sees:
                    // the orchestrator resolved it and the port handed it back.
                    let admitted_text = run_input.input["sources"][0]["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    let result = executor
                        .execute(PromptRequest {
                            run_id: run_id.clone(),
                            task_id: "nexus.llm.extract".to_string(),
                            agent_ref: None,
                            prompt: extract_prompt,
                            tool_policy: ToolPolicy::DenyAll,
                            cancellation,
                        })
                        .await;
                    let result = match result {
                        Ok(result) => result,
                        Err(e) => {
                            slot.lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .failure = Some(e);
                            return SpokeResult::Reject(reject_extraction_failed());
                        }
                    };

                    let parsed = parse_extract_response(&result.full_text);
                    let relationships = parse_relationships_response(&result.full_text);
                    let mut prepared = Vec::with_capacity(parsed.len());
                    let mut local = Vec::with_capacity(parsed.len());
                    for raw in &parsed {
                        if let Some(candidate) =
                            prepare_candidate(raw, &target, &admitted_text, &source_id)
                        {
                            local.push(local_candidate(raw, &candidate));
                            prepared.push(candidate);
                        }
                    }
                    slot.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .candidates = local;

                    SpokeResult::Ok(NativeExtractionOutput {
                        candidates: prepared,
                        method: Some("nexus.llm.extract".to_string()),
                        coverage_hint: None,
                        relationships,
                    })
                },
            )
            .await
        };

        let ExtractionSlot {
            failure,
            candidates: local_candidates,
        } = std::mem::take(
            &mut *slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        // A callback failure is the precise error, not the placeholder reject
        // the callback returned to the orchestrator.
        if let Some(e) = failure {
            return Err(e);
        }
        let outcome = match outcome {
            SpokeResult::Ok(outcome) => outcome,
            SpokeResult::Reject(reject) => {
                return Err(CapabilityError::PermanentExternal(format!(
                    "ke-extraction refused the run: {}: {}",
                    reject.code.as_str(),
                    reject.message
                )));
            }
        };

        Ok(json!({
            "candidates": local_candidates,
            "relationships": outcome.relationships,
            // The upstream-validated protocol echo (run id + method echo +
            // candidate ids), so a caller can verify what the orchestrator
            // accepted. The candidate ids equal the local candidates' ids.
            "extract_run": outcome.response,
        }))
    }
}

/// The placeholder the native callback returns when the extractor itself fails;
/// the precise error is carried out of the callback and returned instead.
fn reject_extraction_failed() -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InternalError,
        message: "the native extractor failed".to_string(),
        details: None,
    }
}

/// Parse the LLM extraction response text into a `candidates` JSON array.
///
/// The LLM is instructed to return `{"candidates": [...]}` but may wrap the
/// JSON in markdown code fences or return a bare array. This parser:
/// 1. Strips markdown code fences if present.
/// 2. Tries to locate the `candidates` array inside a JSON object.
/// 3. Falls back to a bare top-level JSON array.
/// 4. On any parse failure, logs at `warn!` and returns an empty array
///    (best-effort; the caller's review-time hook is non-blocking).
///
/// Each candidate is normalized: unknown `block_type` values are kept as-is
/// (the caller's adopt path validates against the wire enum and surfaces a
/// clean error); `confidence` is clamped to `[0.0, 1.0]`; missing optional
/// fields default safely.
#[must_use]
pub fn parse_extract_response(text: &str) -> Vec<Value> {
    let trimmed = strip_code_fences(text.trim());
    // Try parsing as a JSON object with a `candidates` key first.
    if let Ok(obj) = serde_json::from_str::<serde_json::Map<String, Value>>(trimmed) {
        if let Some(Value::Array(arr)) = obj.get("candidates") {
            return arr.iter().map(normalize_candidate).collect();
        }
        // Object without `candidates` key — fall through to bare-array attempt.
    }
    // Try a bare top-level JSON array.
    if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(trimmed) {
        return arr.iter().map(normalize_candidate).collect();
    }
    tracing::warn!(
        raw_response = %&trimmed[..trimmed.len().min(120)],
        "nexus.llm.extract: failed to parse LLM response as JSON; returning empty candidates"
    );
    Vec::new()
}

/// Strip a single layer of markdown code fences (```json ... ``` or ``` ... ```).
fn strip_code_fences(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```") {
        // Skip an optional language tag on the opening fence line.
        let after_open = rest.find('\n').map_or(rest, |nl| &rest[nl + 1..]);
        if let Some(body) = after_open.strip_suffix("```") {
            return body.trim();
        }
        return after_open.trim();
    }
    s
}

/// Normalize a single candidate object: clamp confidence, ensure required
/// string fields exist (defaulting to empty string so downstream never panics
/// on a missing key — the adopt CLI validates and surfaces clean errors).
fn normalize_candidate(v: &Value) -> Value {
    let Some(obj) = v.as_object() else {
        return v.clone();
    };
    let mut out = serde_json::Map::new();
    for (k, val) in obj {
        out.insert(k.clone(), val.clone());
    }
    // Ensure required string fields are present.
    if !out.contains_key("canonical_name") {
        out.insert("canonical_name".into(), Value::String(String::new()));
    }
    if !out.contains_key("block_type") {
        out.insert("block_type".into(), Value::String("character".into()));
    }
    if !out.contains_key("source_quote") {
        out.insert("source_quote".into(), Value::String(String::new()));
    }
    // Clamp confidence to [0.0, 1.0]; default 0.0 when missing/invalid.
    let confidence = out
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    out.insert("confidence".into(), json!(confidence));
    Value::Object(out)
}

/// Parse the LLM extraction response text into a `relationships` JSON array.
///
/// V1.76: mirrors [`parse_extract_response`] but extracts the optional
/// `relationships` key. When the LLM omits the key, returns an empty array
/// (backward compatible — relationship proposal is best-effort). Each
/// relationship candidate is normalized via [`normalize_relationship`].
#[must_use]
pub fn parse_relationships_response(text: &str) -> Vec<Value> {
    let trimmed = strip_code_fences(text.trim());
    if let Ok(obj) = serde_json::from_str::<serde_json::Map<String, Value>>(trimmed) {
        if let Some(Value::Array(arr)) = obj.get("relationships") {
            return arr.iter().map(normalize_relationship).collect();
        }
    }
    // Bare array or object without `relationships` → no relationship candidates.
    Vec::new()
}

/// Normalize a single relationship candidate object: clamp confidence, ensure
/// required string fields exist (defaulting to empty string so downstream
/// never panics on a missing key — the persist path validates and skips).
fn normalize_relationship(v: &Value) -> Value {
    let Some(obj) = v.as_object() else {
        return v.clone();
    };
    let mut out = serde_json::Map::new();
    for (k, val) in obj {
        out.insert(k.clone(), val.clone());
    }
    // Ensure required string fields are present.
    if !out.contains_key("source_canonical_name") {
        out.insert("source_canonical_name".into(), Value::String(String::new()));
    }
    if !out.contains_key("target_canonical_name") {
        out.insert("target_canonical_name".into(), Value::String(String::new()));
    }
    if !out.contains_key("relation_type") {
        out.insert("relation_type".into(), Value::String("custom".into()));
    }
    if !out.contains_key("source_quote") {
        out.insert("source_quote".into(), Value::String(String::new()));
    }
    // `symmetric` defaults to false when missing/invalid.
    if !out
        .get("symmetric")
        .is_some_and(serde_json::Value::is_boolean)
    {
        out.insert("symmetric".into(), Value::Bool(false));
    }
    // Clamp confidence to [0.0, 1.0]; default 0.0 when missing/invalid.
    let confidence = out
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    out.insert("confidence".into(), json!(confidence));
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shared cancellation map with coordinator tokens registered for the
    /// given run ids (fail-closed contract; production registers at run
    /// admission).
    fn cancels_with(
        ids: &[&str],
    ) -> std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > {
        let map: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        {
            let mut guard = map
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for id in ids {
                guard.insert(id.to_string(), tokio_util::sync::CancellationToken::new());
            }
        }
        map
    }

    #[test]
    fn llm_extract_name() {
        let cap = LlmExtract::new();
        assert_eq!(cap.name(), "nexus.llm.extract");
    }

    #[tokio::test]
    async fn llm_extract_standalone_returns_unavailable() {
        let cap = LlmExtract::new();
        let input = json!({ "prompt": "extract", "chapter_prose": "Lin Xia walked.", "_session_id": "sess" });
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("worker unavailable"),
            "expected worker unavailable, got: {err}"
        );
    }

    #[tokio::test]
    async fn llm_extract_missing_prompt_errors() {
        let cap = LlmExtract::new();
        let input = json!({ "chapter_prose": "..." });
        let result = cap.run(input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn llm_extract_missing_prose_errors() {
        let cap = LlmExtract::new();
        let input = json!({ "prompt": "extract" });
        let result = cap.run(input).await;
        assert!(result.is_err());
    }

    /// Mock executor returning a JSON object with a `candidates` array.
    struct MockExtractExecutor {
        response: String,
    }

    #[async_trait]
    impl PromptExecutor for MockExtractExecutor {
        async fn execute(
            &self,
            _request: PromptRequest,
        ) -> Result<crate::capability::PromptResult, CapabilityError> {
            Ok(crate::capability::PromptResult {
                full_text: self.response.clone(),
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
        }
    }

    fn mock_executor(response: &str) -> Arc<MockExtractExecutor> {
        Arc::new(MockExtractExecutor {
            response: response.to_string(),
        })
    }

    /// The trusted extraction context the orchestration caller resolves from
    /// stored state before the model runs (v1.191 P1 T13, durable §§3, 8).
    fn trusted_context(session_id: &str) -> Value {
        json!({
            "_extract_target": {
                "world_id": "wld_fixture",
                "holder_entry_id": null,
                "disclosure": null,
            },
            "_extract_source_id": "Works/fixture/Stories/ch03.md",
            "_session_id": session_id,
        })
    }

    /// Decorate a fixture input with the trusted extraction context.
    fn with_trusted_context(mut input: Value, session_id: &str) -> Value {
        let trusted = trusted_context(session_id);
        let object = input
            .as_object_mut()
            .expect("fixture input is a JSON object");
        for (key, value) in trusted
            .as_object()
            .expect("trusted context is a JSON object")
        {
            object.insert(key.clone(), value.clone());
        }
        input
    }

    #[tokio::test]
    async fn llm_extract_with_mock_executor_returns_candidates() {
        let cap = LlmExtract::with_prompt_executor(mock_executor(
            r#"{"candidates":[
                {"canonical_name":"Lin Xia","block_type":"character","summary":"A warrior","confidence":0.9,"source_quote":"Lin Xia drew her blade."},
                {"canonical_name":"Azure Gate","block_type":"scene","summary":null,"confidence":0.8,"source_quote":"the Azure Gate groaned open"}
            ]}"#,
        ))
            .with_session_cancels(cancels_with(&["default"]));
        let input = with_trusted_context(
            json!({ "prompt": "extract", "chapter_prose": "..." }),
            "default",
        );
        let result = cap.run(input).await.unwrap();
        let candidates = result.get("candidates").and_then(|v| v.as_array()).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0]["canonical_name"], "Lin Xia");
        assert_eq!(candidates[0]["block_type"], "character");
        assert_eq!(candidates[1]["block_type"], "scene");
    }

    #[tokio::test]
    async fn llm_extract_parses_code_fenced_json() {
        let cap = LlmExtract::with_prompt_executor(mock_executor(
            "```json\n{\"candidates\":[{\"canonical_name\":\"X\",\"block_type\":\"item\",\"confidence\":0.5,\"source_quote\":\"q\"}]}\n```",
        ))
            .with_session_cancels(cancels_with(&["default"]));
        let input = with_trusted_context(
            json!({ "prompt": "extract", "chapter_prose": "..." }),
            "default",
        );
        let result = cap.run(input).await.unwrap();
        let candidates = result.get("candidates").and_then(|v| v.as_array()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0]["canonical_name"], "X");
    }

    #[tokio::test]
    async fn llm_extract_malformed_json_returns_empty_candidates() {
        let cap = LlmExtract::with_prompt_executor(mock_executor("this is not json at all"))
            .with_session_cancels(cancels_with(&["default"]));
        let input = with_trusted_context(
            json!({ "prompt": "extract", "chapter_prose": "..." }),
            "default",
        );
        let result = cap.run(input).await.unwrap();
        let candidates = result.get("candidates").and_then(|v| v.as_array()).unwrap();
        assert!(candidates.is_empty(), "malformed JSON → empty candidates");
    }

    #[test]
    fn parse_clamps_confidence() {
        let parsed = parse_extract_response(
            r#"{"candidates":[{"canonical_name":"A","block_type":"character","confidence":1.5,"source_quote":"q"}]}"#,
        );
        assert_eq!(parsed[0]["confidence"], json!(1.0));
        let parsed = parse_extract_response(
            r#"{"candidates":[{"canonical_name":"A","block_type":"character","confidence":-0.3,"source_quote":"q"}]}"#,
        );
        assert_eq!(parsed[0]["confidence"], json!(0.0));
    }

    #[test]
    fn parse_normalizes_missing_fields() {
        // Missing block_type/source_quote → defaulted; missing confidence → 0.0.
        let parsed = parse_extract_response(r#"{"candidates":[{"canonical_name":"A"}]}"#);
        assert_eq!(parsed[0]["canonical_name"], "A");
        assert_eq!(parsed[0]["block_type"], "character");
        assert_eq!(parsed[0]["source_quote"], "");
        assert_eq!(parsed[0]["confidence"], json!(0.0));
    }

    #[test]
    fn parse_bare_array_response() {
        let parsed = parse_extract_response(
            r#"[{"canonical_name":"A","block_type":"event","confidence":0.7,"source_quote":"q"}]"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["block_type"], "event");
    }

    // ── V1.76: relationship candidate parsing ──────────────────────────────

    #[test]
    fn parse_relationships_from_object_response() {
        let parsed = parse_relationships_response(
            r#"{"candidates":[],"relationships":[
                {"source_canonical_name":"Aria","source_block_type":"character",
                 "target_canonical_name":"Kael","target_block_type":"character",
                 "relation_type":"allied_with","symmetric":true,
                 "confidence":0.8,"source_quote":"Aria and Kael fought together"}
            ]}"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["source_canonical_name"], "Aria");
        assert_eq!(parsed[0]["target_canonical_name"], "Kael");
        assert_eq!(parsed[0]["relation_type"], "allied_with");
        assert_eq!(parsed[0]["symmetric"], true);
        assert_eq!(parsed[0]["confidence"], json!(0.8));
    }

    #[test]
    fn parse_relationships_missing_key_returns_empty() {
        // Object with candidates but no relationships key → empty (backward compat).
        let parsed = parse_relationships_response(
            r#"{"candidates":[{"canonical_name":"A","block_type":"character","confidence":0.5,"source_quote":"q"}]}"#,
        );
        assert!(parsed.is_empty(), "missing relationships key → empty");
    }

    #[test]
    fn parse_relationships_clamps_confidence() {
        let parsed = parse_relationships_response(
            r#"{"relationships":[{"source_canonical_name":"A","target_canonical_name":"B",
               "relation_type":"rival_of","symmetric":false,"confidence":1.5,"source_quote":"q"}]}"#,
        );
        assert_eq!(parsed[0]["confidence"], json!(1.0));
    }

    #[test]
    fn parse_relationships_normalizes_missing_fields() {
        let parsed = parse_relationships_response(
            r#"{"relationships":[{"source_canonical_name":"A","target_canonical_name":"B"}]}"#,
        );
        assert_eq!(parsed.len(), 1);
        // relation_type defaults to custom; symmetric to false; confidence 0.0.
        assert_eq!(parsed[0]["relation_type"], "custom");
        assert_eq!(parsed[0]["symmetric"], false);
        assert_eq!(parsed[0]["confidence"], json!(0.0));
    }

    #[tokio::test]
    async fn llm_extract_with_mock_executor_returns_relationships() {
        let cap = LlmExtract::with_prompt_executor(mock_executor(
            r#"{"candidates":[
                {"canonical_name":"Aria","block_type":"character","confidence":0.9,"source_quote":"Aria drew her blade."}
            ],"relationships":[
                {"source_canonical_name":"Aria","target_canonical_name":"Kael",
                 "relation_type":"allied_with","symmetric":true,"confidence":0.8,
                 "source_quote":"Aria and Kael fought together"}
            ]}"#,
        ))
            .with_session_cancels(cancels_with(&["default"]));
        let input = with_trusted_context(
            json!({ "prompt": "extract", "chapter_prose": "..." }),
            "default",
        );
        let result = cap.run(input).await.unwrap();
        // candidates still present.
        let candidates = result.get("candidates").and_then(|v| v.as_array()).unwrap();
        assert_eq!(candidates.len(), 1);
        // relationships present.
        let relationships = result
            .get("relationships")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0]["source_canonical_name"], "Aria");
        assert_eq!(relationships[0]["relation_type"], "allied_with");
    }

    // ── SEC-V131-01: identity boundary regression (mirrors judge.llm) ──────

    struct CapturingExecutor {
        captured: std::sync::Mutex<String>,
    }

    #[async_trait]
    impl PromptExecutor for CapturingExecutor {
        async fn execute(
            &self,
            request: PromptRequest,
        ) -> Result<crate::capability::PromptResult, CapabilityError> {
            *self.captured.lock().expect("capture lock") = request.run_id;
            Ok(crate::capability::PromptResult {
                full_text: "{\"candidates\":[]}".to_string(),
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn llm_extract_raw_creator_id_ignored_on_spoof_attempt() {
        let executor = Arc::new(CapturingExecutor {
            captured: std::sync::Mutex::new(String::new()),
        });
        let cap = LlmExtract::with_prompt_executor(executor.clone())
            .with_session_cancels(cancels_with(&["default"]));
        let input = json!({
            "prompt": "extract",
            "chapter_prose": "...",
            // Spoof attempt: raw preset args should be ignored.
            "creator_id": "spoofed_creator",
            "session_id": "spoofed_session"
        });
        // M-002: with no trusted `_session_id` the capability fails closed
        // (Forbidden) instead of falling back to a magic `default` run id.
        let result = cap.run(input).await;
        assert!(
            result.is_err(),
            "SEC-V131-01/M-002: raw session_id must never be trusted; missing trusted _session_id must refuse"
        );
        let captured = executor.captured.lock().expect("capture lock").clone();
        assert_eq!(captured, "", "SEC-V131-01: raw session_id leaked through");
    }

    // ── v1.191 P1 T13: real caller path through the adapter wrapper ────────

    /// An executor that also reports the run identity it was invoked with.
    struct RunCapturingExecutor {
        response: String,
        run_id: std::sync::Mutex<String>,
    }

    #[async_trait]
    impl PromptExecutor for RunCapturingExecutor {
        async fn execute(
            &self,
            request: PromptRequest,
        ) -> Result<crate::capability::PromptResult, CapabilityError> {
            *self.run_id.lock().expect("run id lock") = request.run_id;
            Ok(crate::capability::PromptResult {
                full_text: self.response.clone(),
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn v1191_extract_run_reaches_the_orchestrator_and_keeps_the_local_shape() {
        let executor = Arc::new(RunCapturingExecutor {
            response: json!({
                "candidates": [
                    {
                        "canonical_name": "Lin Xia",
                        "block_type": "character",
                        "summary": "A warrior",
                        "confidence": 0.9,
                        "source_quote": "Lin Xia drew her blade.",
                    }
                ],
                "relationships": [
                    {
                        "source_canonical_name": "Lin Xia",
                        "target_canonical_name": "Kael",
                        "relation_type": "allied_with",
                        "symmetric": true,
                        "confidence": 0.8,
                        "source_quote": "Aria and Kael fought together",
                    }
                ],
            })
            .to_string(),
            run_id: std::sync::Mutex::new(String::new()),
        });
        let cap = LlmExtract::with_prompt_executor(executor.clone())
            .with_session_cancels(cancels_with(&["run_t13"]));
        let input = with_trusted_context(
            json!({ "prompt": "extract", "chapter_prose": "Lin Xia drew her blade." }),
            "run_t13",
        );

        let result = cap.run(input).await.expect("the run is accepted");

        // The real production run identity reached the prompt executor.
        assert_eq!(
            executor.run_id.lock().expect("run id lock").as_str(),
            "run_t13"
        );
        // The orchestrator's response echoed the request run id and the method.
        assert_eq!(result["extract_run"]["run"]["run_id"], "run_t13");
        assert_eq!(result["extract_run"]["run"]["method"], "nexus.llm.extract");

        // The local result shape is preserved, and each candidate carries the
        // id the protocol validated (the wire candidate's own id).
        let candidates = result["candidates"].as_array().expect("candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0]["canonical_name"], "Lin Xia");
        assert_eq!(candidates[0]["block_type"], "character");
        assert_eq!(candidates[0]["summary"], "A warrior");
        assert_eq!(candidates[0]["confidence"], json!(0.9));
        assert_eq!(candidates[0]["source_quote"], "Lin Xia drew her blade.");
        let wire = result["extract_run"]["candidates"]
            .as_array()
            .expect("wire candidates");
        assert_eq!(wire.len(), 1);
        assert_eq!(candidates[0]["entry_id"], wire[0]["entry_id"]);
        // The relationship sidecar survives SPOKE's KE-only success arm.
        let relationships = result["relationships"].as_array().expect("relationships");
        assert_eq!(relationships.len(), 1);
        assert_eq!(relationships[0]["relation_type"], "allied_with");
        assert_eq!(relationships[0]["target_canonical_name"], "Kael");
    }

    #[tokio::test]
    async fn v1191_extract_model_output_cannot_choose_governance() {
        // The model tries to hand the candidate an owner and an owner-private
        // disclosure; the trusted target policy decides instead.
        let cap = LlmExtract::with_prompt_executor(mock_executor(
            r#"{"candidates":[{"canonical_name":"Lin Xia","block_type":"character","summary":"A warrior","confidence":0.9,"source_quote":"Lin Xia drew her blade."}]}"#,
        ))
        .with_session_cancels(cancels_with(&["run_t13"]));
        let mut input = with_trusted_context(
            json!({ "prompt": "extract", "chapter_prose": "Lin Xia drew her blade." }),
            "run_t13",
        );
        // Model output reaching the wire conversion by every path it owns.
        input["model_holder_entry_id"] = json!("hld_model_choice");
        input["model_disclosure"] = json!("owner-private");

        let result = cap.run(input).await.expect("the run is accepted");
        let wire = result["extract_run"]["candidates"]
            .as_array()
            .expect("wire candidates");
        assert_eq!(wire.len(), 1);
        assert_eq!(
            wire[0]["owner"],
            Value::Null,
            "the model cannot name a holder"
        );
        assert_eq!(
            wire[0]["disclosure"],
            Value::Null,
            "the model cannot make the row owner-private"
        );
    }

    #[tokio::test]
    async fn v1191_extract_missing_trusted_context_refuses() {
        // Missing target: refuse rather than extract under an invented policy.
        let cap = LlmExtract::with_prompt_executor(mock_executor("{\"candidates\":[]}"))
            .with_session_cancels(cancels_with(&["run_t13"]));
        let error = cap
            .run(json!({
                "prompt": "extract",
                "chapter_prose": "Lin Xia drew her blade.",
                "_session_id": "run_t13",
            }))
            .await
            .expect_err("a missing target must refuse");
        assert!(
            error.to_string().contains("_extract_target"),
            "error: {error}"
        );

        // Missing admitted source identity: same refusal.
        let mut input = with_trusted_context(
            json!({ "prompt": "extract", "chapter_prose": "Lin Xia drew her blade." }),
            "run_t13",
        );
        input
            .as_object_mut()
            .expect("object input")
            .remove("_extract_source_id");
        let error = cap
            .run(input)
            .await
            .expect_err("a missing source identity must refuse");
        assert!(
            error.to_string().contains("_extract_source_id"),
            "error: {error}"
        );
    }
}

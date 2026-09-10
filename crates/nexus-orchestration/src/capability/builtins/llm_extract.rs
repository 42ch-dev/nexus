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
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

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
    // judge.llm).
    fn input_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["prompt", "chapter_prose"],
            "properties": {
                "prompt": { "type": "string", "description": "Extraction instruction template (rendered by LlmExtractTask)" },
                "chapter_prose": { "type": "string", "description": "Verbatim chapter body to extract entities from" },
                "_creator_id": { "type": "string" },
                "_session_id": { "type": "string" }
            }
        }"#
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

        let executor = self
            .executor
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        // A1: resolve the coordinator cancellation token for this run from
        // the shared per-run map. FAIL-CLOSED: a run with no registered
        // token refuses with `CancellationUnavailable` — a fresh token would
        // be uncancellable by any coordinator (never mint one here). The run
        // admission path (engine start/spawn/recovery) registers the token.
        let cancellation =
            crate::capability::resolve_session_cancellation(&self.session_cancels, session_id)?;

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

        let result = executor
            .execute(PromptRequest {
                run_id: session_id.to_string(),
                task_id: "nexus.llm.extract".to_string(),
                agent_ref: None,
                prompt: extract_prompt,
                tool_policy: ToolPolicy::DenyAll,
                cancellation,
            })
            .await?;

        let candidates = parse_extract_response(&result.full_text);
        let relationships = parse_relationships_response(&result.full_text);
        Ok(json!({ "candidates": candidates, "relationships": relationships }))
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

    #[tokio::test]
    async fn llm_extract_with_mock_executor_returns_candidates() {
        let cap = LlmExtract::with_prompt_executor(mock_executor(
            r#"{"candidates":[
                {"canonical_name":"Lin Xia","block_type":"character","summary":"A warrior","confidence":0.9,"source_quote":"Lin Xia drew her blade."},
                {"canonical_name":"Azure Gate","block_type":"scene","summary":null,"confidence":0.8,"source_quote":"the Azure Gate groaned open"}
            ]}"#,
        ))
            .with_session_cancels(cancels_with(&["default"]));
        let input =
            json!({ "prompt": "extract", "chapter_prose": "...", "_session_id": "default" });
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
        let input =
            json!({ "prompt": "extract", "chapter_prose": "...", "_session_id": "default" });
        let result = cap.run(input).await.unwrap();
        let candidates = result.get("candidates").and_then(|v| v.as_array()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0]["canonical_name"], "X");
    }

    #[tokio::test]
    async fn llm_extract_malformed_json_returns_empty_candidates() {
        let cap = LlmExtract::with_prompt_executor(mock_executor("this is not json at all"))
            .with_session_cancels(cancels_with(&["default"]));
        let input =
            json!({ "prompt": "extract", "chapter_prose": "...", "_session_id": "default" });
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
        let input =
            json!({ "prompt": "extract", "chapter_prose": "...", "_session_id": "default" });
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
}

//! `soul.experience.aggregate` capability — deterministic SOUL Experience refresh.
//!
//! Reads long-term memory files for a creator, filters to experience-kind
//! entries, sorts by recency, and produces aggregated markdown for the
//! `## Experience` section of SOUL.md.
//!
//! Design: plan `2026-05-26-v1.29-soul-experience-preset` §A2.2.
//! This capability is designed to be called from a preset (schedule-driven)
//! or directly from the CLI. No LLM invocation occurs inside this capability;
//! if LLM polish is desired, chain `context.summarize` afterwards.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::{json, Value};

/// The `soul.experience.aggregate` capability.
///
/// Input schema:
/// ```json
/// { "creator_id": string, "home_dir": string }
/// ```
///
/// Output schema:
/// ```json
/// { "memories_processed": integer, "experience_markdown": string }
/// ```
pub struct SoulExperienceAggregate;

#[async_trait]
impl Capability for SoulExperienceAggregate {
    fn name(&self) -> &'static str {
        "soul.experience.aggregate"
    }

    fn input_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["creator_id", "home_dir"],
            "properties": {
                "creator_id": {
                    "type": "string",
                    "description": "Creator ID to aggregate experience for"
                },
                "home_dir": {
                    "type": "string",
                    "description": "Absolute path to the user home directory"
                }
            },
            "additionalProperties": false
        }"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["memories_processed", "experience_markdown"],
            "properties": {
                "memories_processed": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Number of experience-kind memories found"
                },
                "experience_markdown": {
                    "type": "string",
                    "description": "Aggregated markdown body for the Experience section"
                }
            },
            "additionalProperties": false
        }"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let creator_id = input
            .get("creator_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CapabilityError::InputInvalid("missing 'creator_id' field".to_string())
            })?;

        let home_dir = input
            .get("home_dir")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'home_dir' field".to_string()))?;

        let home = std::path::Path::new(home_dir);

        // Use the preview function (no SOUL.md write) so the capability is
        // side-effect-free. The preset orchestrator or CLI is responsible for
        // writing the result to SOUL.md.
        let result = nexus_creator_memory::experience_aggregation::aggregate_experience_preview(
            home, creator_id, None, // Deterministic path only — no LLM synthesizer
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("experience aggregation failed: {e}")))?;

        Ok(json!({
            "memories_processed": result.memories_processed,
            "experience_markdown": result.experience_markdown
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soul_experience_aggregate_name() {
        assert_eq!(SoulExperienceAggregate.name(), "soul.experience.aggregate");
    }

    #[test]
    fn soul_experience_aggregate_input_schema_valid() {
        let schema: Value = serde_json::from_str(SoulExperienceAggregate.input_schema())
            .expect("valid JSON Schema");
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["type"], "object");
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("creator_id")));
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("home_dir")));
    }

    #[test]
    fn soul_experience_aggregate_output_schema_valid() {
        let schema: Value = serde_json::from_str(SoulExperienceAggregate.output_schema())
            .expect("valid JSON Schema");
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("memories_processed")));
        assert!(required.contains(&json!("experience_markdown")));
    }

    #[tokio::test]
    async fn soul_experience_aggregate_missing_creator_id_errors() {
        let cap = SoulExperienceAggregate;
        let result = cap.run(json!({ "home_dir": "/tmp" })).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing 'creator_id'"));
    }

    #[tokio::test]
    async fn soul_experience_aggregate_missing_home_dir_errors() {
        let cap = SoulExperienceAggregate;
        let result = cap.run(json!({ "creator_id": "ctr_test" })).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing 'home_dir'"));
    }

    #[tokio::test]
    async fn soul_experience_aggregate_no_memories_returns_empty() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let home = tmp.path();

        // Create SOUL first so preview can load it
        nexus_creator_memory::soul_io::create(home, "ctr_captest").unwrap();

        let cap = SoulExperienceAggregate;
        let result = cap
            .run(json!({
                "creator_id": "ctr_captest",
                "home_dir": home.to_str().unwrap()
            }))
            .await
            .unwrap();

        assert_eq!(result["memories_processed"], 0);
        assert_eq!(result["experience_markdown"], "");
    }

    #[tokio::test]
    async fn soul_experience_aggregate_with_memories_returns_content() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let home = tmp.path();

        // Create SOUL
        nexus_creator_memory::soul_io::create(home, "ctr_captest2").unwrap();

        // Create an experience-kind memory
        let mut mem = nexus_creator_memory::LongTermMemory::new("story_summary");
        mem.set_body("An epic tale of courage and determination.");
        nexus_creator_memory::memory_io::save_memory(home, "ctr_captest2", "epic-tale", &mem)
            .unwrap();

        let cap = SoulExperienceAggregate;
        let result = cap
            .run(json!({
                "creator_id": "ctr_captest2",
                "home_dir": home.to_str().unwrap()
            }))
            .await
            .unwrap();

        assert_eq!(result["memories_processed"], 1);
        let markdown = result["experience_markdown"].as_str().unwrap();
        assert!(markdown.contains("Story Summary"));
        assert!(markdown.contains("epic-tale"));
    }
}

//! Core Context types for Creator Schedules (WS7).
//!
//! Immutable versioned prompt state that the engine reads at execution
//! boundaries. Per spec §3.2 and §6.
//!
//! Design: `.agents/plans/knowledge/creator-schedule-and-core-context-v1.md`

use serde::{Deserialize, Serialize};

/// Strictly increasing version number per Schedule. v0 is initial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CoreContextVersion(pub u32);

/// Full record of a single `core_context` version.
///
/// Each row in `core_context_versions` is immutable once written.
/// The Schedule's `current_core_context_version` column points at the head.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CoreContextRecord {
    pub schedule_id: String,
    pub version: CoreContextVersion,
    pub content: CoreContextPayload,
    pub derivation: DerivationStep,
    pub created_at: String,
    pub created_by: CoreContextAuthor,
}

/// The "what preset execution actually sees".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoreContextPayload {
    /// Flat text form (V1.4 minimum).
    /// Preset inserts `{{core_context.text}}` into prompts.
    Text { body: String },
    /// Structured form (V1.4 optional for presets that want it).
    /// Preset accesses specific fields via `{{core_context.struct.key}}`.
    Struct { body: serde_json::Value },
}

/// What caused this `core_context` version to be created.
///
/// Uses `#[serde(tag = "kind")]` for forward-compatible tagged enum
/// representation. `#[non_exhaustive]` on `LlmSummarize` reserves it for
/// V1.5+ without requiring schema migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DerivationStep {
    /// v0 — from `schedule add --seed`
    Seed { raw: String },
    /// User modify/append/delete via `schedule edit`
    UserEdit {
        op: EditOp,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_user: Option<String>,
    },
    /// Preset declared `context_update` hook fired on state exit
    PresetHook { state_id: String, hook_name: String },
    /// V1.5+ only; V1.4 does not emit this kind.
    /// A future `context.summarize` capability will write this variant.
    #[non_exhaustive]
    LlmSummarize {
        capability: String,
        prompt_hash: [u8; 32],
    },
    /// Preset `initial_action: llm_expand` (opt-in per preset; rare)
    PresetSeedExpansion { capability: String },
}

/// Edit operations for user-driven `core_context` changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditOp {
    /// Overwrite text content entirely
    Replace { body: String },
    /// Append text to existing content
    Append { body: String },
    /// JSON-merge into struct payload
    StructMerge { patch: serde_json::Value },
    /// Remove a struct key by dotted path
    StructRemove { path: String },
}

/// Who authored a `core_context` version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoreContextAuthor {
    User { id: String },
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_context_version_ordering() {
        let v0 = CoreContextVersion(0);
        let v1 = CoreContextVersion(1);
        let v2 = CoreContextVersion(2);
        assert!(v0 < v1);
        assert!(v1 < v2);
        assert_eq!(v0, CoreContextVersion(0));
    }

    #[test]
    fn core_context_payload_text_roundtrip() {
        let p = CoreContextPayload::Text {
            body: "hello world".to_string(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: CoreContextPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn core_context_payload_struct_roundtrip() {
        let p = CoreContextPayload::Struct {
            body: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: CoreContextPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn edit_op_replace_roundtrip() {
        let op = EditOp::Replace {
            body: "new content".to_string(),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"kind\":\"replace\""));
        let back: EditOp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }

    #[test]
    fn edit_op_struct_merge_roundtrip() {
        let op = EditOp::StructMerge {
            patch: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"kind\":\"struct_merge\""));
        let back: EditOp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }

    #[test]
    fn core_context_author_user_roundtrip() {
        let a = CoreContextAuthor::User {
            id: "usr_123".to_string(),
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"kind\":\"user\""));
        let back: CoreContextAuthor = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn core_context_author_system_roundtrip() {
        let a = CoreContextAuthor::System;
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"kind\":\"system\""));
        let back: CoreContextAuthor = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }
}

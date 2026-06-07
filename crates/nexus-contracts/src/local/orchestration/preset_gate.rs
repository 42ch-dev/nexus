//! Preset gate types for enqueue-time precondition evaluation (V1.37 §7.9).
//!
//! These types live in `nexus-contracts` so they can be referenced from both
//! `nexus-orchestration` (evaluator) and the preset manifest (`PresetHeader.gates`).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Gate declaration types (parsed from preset YAML `gates:` section)
// ---------------------------------------------------------------------------

/// A single gate declaration from a preset manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Gate {
    /// Check a field on the `works` table.
    WorkField {
        /// Column name (flat column only; dot-path not supported V1.37).
        field: String,
        /// Comparison operator and value.
        #[serde(flatten)]
        op: GateOp,
    },
    /// Check a filesystem path under workspace root.
    Filesystem {
        /// Path template with `{{var}}` substitution (e.g. `Works/{{work_ref}}/`).
        path: String,
        /// Whether the path must exist.
        must_exist: bool,
    },
    /// Check a prior preset's completion status for the same Work.
    PreviousPreset {
        /// Preset ID to look up.
        preset: String,
        /// Required status.
        status: PreviousPresetStatus,
        /// Scope: only `work` is normative V1.37.
        #[serde(default = "default_scope")]
        scope: String,
    },
}

fn default_scope() -> String {
    "work".to_string()
}

/// Comparison operator for `work_field` gates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum GateOp {
    /// Field must equal the given value.
    Equals { value: serde_json::Value },
    /// Field must not equal the given value.
    NotEquals { value: serde_json::Value },
    /// Field must be non-null.
    Required,
    /// Field must be one of the given values.
    #[serde(rename = "in")]
    In { value: Vec<serde_json::Value> },
    /// Field must not be one of the given values.
    NotIn { value: Vec<serde_json::Value> },
}

/// Status requirement for `previous_preset` gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviousPresetStatus {
    /// Preset must have reached terminal completion.
    Complete,
    /// Preset must have any session (completed/paused/`waiting_for_input`).
    AnySession,
}

/// A single failed gate with diagnostic information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedGate {
    /// The gate kind.
    pub kind: String,
    /// Human-readable description of what was expected.
    pub expected: String,
    /// Human-readable description of what was found.
    pub actual: String,
    /// User-facing remediation text.
    pub remediation: String,
}

/// Structured error returned when preset gates fail (spec §7.9.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresetGatesFailed {
    /// Error code (always `preset_gates_failed`).
    pub error: String,
    /// Preset ID that failed gate evaluation.
    pub preset_id: String,
    /// Work ID being evaluated.
    pub work_id: String,
    /// List of failed gates with diagnostics.
    pub failed_gates: Vec<FailedGate>,
}

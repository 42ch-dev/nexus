//! Preset gate types for enqueue-time precondition evaluation (V1.37 §7.9).
//!
//! These types live in `nexus-contracts` so they can be referenced from both
//! `nexus-orchestration` (evaluator) and the preset manifest (`PresetHeader.gates`).
// note: shared by the evaluator and the manifest parser, so it cannot live in either crate.

use serde::{Deserialize, Serialize};

// note: gate declaration types (parsed from preset YAML `gates:` section).

/// note: a single gate declaration from a preset manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Gate {
    /// note: check a field on the `works` table.
    WorkField {
        /// note: column name (flat column only; dot-path not supported V1.37).
        field: String,
        /// note: comparison operator and value.
        #[serde(flatten)]
        op: GateOp,
    },
    /// note: check a filesystem path under workspace root.
    Filesystem {
        /// note: path template with `{{var}}` substitution (e.g. `Works/{{work_ref}}/`).
        path: String,
        /// note: whether the path must exist.
        must_exist: bool,
    },
    /// note: check a prior preset's completion status for the same Work.
    PreviousPreset {
        /// note: preset ID to look up.
        preset: String,
        /// note: required status.
        status: PreviousPresetStatus,
        /// note: scope: only `work` is normative V1.37.
        #[serde(default = "default_scope")]
        scope: String,
    },
}

fn default_scope() -> String {
    "work".to_string()
}

/// note: comparison operator for `work_field` gates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum GateOp {
    /// note: field must equal the given value.
    Equals { value: serde_json::Value },
    /// note: field must not equal the given value.
    NotEquals { value: serde_json::Value },
    /// note: field must be non-null.
    Required,
    /// note: field must be one of the given values.
    #[serde(rename = "in")]
    In { value: Vec<serde_json::Value> },
    /// note: field must not be one of the given values.
    NotIn { value: Vec<serde_json::Value> },
}

/// note: status requirement for `previous_preset` gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviousPresetStatus {
    /// note: preset must have reached terminal completion.
    Complete,
    /// note: preset must have any session (completed/paused/`waiting_for_input`).
    AnySession,
}

/// note: a single failed gate with diagnostic information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedGate {
    /// note: the gate kind.
    pub kind: String,
    /// note: human-readable description of what was expected.
    pub expected: String,
    /// note: human-readable description of what was found.
    pub actual: String,
    /// note: user-facing remediation text.
    pub remediation: String,
}

/// note: structured error returned when preset gates fail (spec §7.9.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresetGatesFailed {
    /// note: error code (always `preset_gates_failed`).
    pub error: String,
    /// note: preset ID that failed gate evaluation.
    pub preset_id: String,
    /// note: work ID being evaluated.
    pub work_id: String,
    /// note: list of failed gates with diagnostics.
    pub failed_gates: Vec<FailedGate>,
}

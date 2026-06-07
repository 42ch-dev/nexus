//! Generic preset gate evaluator (V1.37, spec §7.9).
//!
//! Evaluates `preset.gates` at enqueue time to enforce preconditions before
//! scheduling a preset for execution. Gate types live in `nexus-contracts`;
//! this module provides the runtime evaluator.
//!
//! On failure: structured `PresetGatesFailed` error with per-gate remediation.
//! On `--force-gates`: bypass evaluation, log audit row.

pub use nexus_contracts::local::orchestration::preset_gate::*;

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Gate evaluation context
// ---------------------------------------------------------------------------

/// Work fields available for gate evaluation (snapshot at enqueue time).
#[derive(Debug, Clone)]
pub struct WorkSnapshot {
    /// Work entity ID.
    pub work_id: String,
    /// Creator ID owning the Work.
    pub creator_id: String,
    /// `work_profile` column value (None if NULL).
    pub work_profile: Option<String>,
    /// `work_ref` column value.
    pub work_ref: Option<String>,
    /// `workspace_slug` column value.
    pub workspace_slug: Option<String>,
    /// `intake_status` column value.
    pub intake_status: Option<String>,
    /// `world_id` column value.
    pub world_id: Option<String>,
    /// `status` column value.
    pub status: Option<String>,
    /// `current_stage` column value.
    pub current_stage: Option<String>,
    /// `title` column value.
    pub title: Option<String>,
    /// `total_planned_chapters` column value.
    pub total_planned_chapters: Option<i64>,
}

impl WorkSnapshot {
    /// Look up a field value by column name.
    #[must_use]
    pub fn get_field(&self, field: &str) -> Option<serde_json::Value> {
        match field {
            "work_id" => Some(serde_json::Value::String(self.work_id.clone())),
            "creator_id" => Some(serde_json::Value::String(self.creator_id.clone())),
            "work_profile" => self
                .work_profile
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone())),
            "work_ref" => self
                .work_ref
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone())),
            "workspace_slug" => self
                .workspace_slug
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone())),
            "intake_status" => self
                .intake_status
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone())),
            "world_id" => self
                .world_id
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone())),
            "status" => self
                .status
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone())),
            "current_stage" => self
                .current_stage
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone())),
            "title" => self
                .title
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone())),
            "total_planned_chapters" => self
                .total_planned_chapters
                .map(|n| serde_json::Value::Number(serde_json::Number::from(n))),
            _ => None,
        }
    }
}

/// Preset input variables for path substitution.
#[derive(Debug, Clone)]
pub struct PresetInput {
    /// Key-value pairs from `AddScheduleRequest.input` or prompt vars.
    pub vars: std::collections::HashMap<String, String>,
}

impl PresetInput {
    /// Substitute `{{key}}` placeholders in a path template.
    ///
    /// # Errors
    ///
    /// Returns `GateEvalError::PathSafety` if the result contains `..` or
    /// starts with `/`.
    pub fn substitute_path(&self, template: &str) -> Result<PathBuf, GateEvalError> {
        let mut result = template.to_string();
        for (k, v) in &self.vars {
            let pattern = format!("{{{{{k}}}}}");
            result = result.replace(&pattern, v);
        }
        // Path safety (spec §7.6.1): no `..` escape, no absolute path
        if result.contains("..") {
            return Err(GateEvalError::PathSafety {
                path: template.to_string(),
                reason: "path traversal detected".to_string(),
            });
        }
        if result.starts_with('/') {
            return Err(GateEvalError::PathSafety {
                path: template.to_string(),
                reason: "absolute path not allowed".to_string(),
            });
        }
        Ok(PathBuf::from(result))
    }
}

/// Previous-preset lookup result (provided by the caller from DB).
#[derive(Debug, Clone)]
pub struct PreviousPresetResult {
    /// Whether a matching session was found.
    pub found: bool,
    /// Whether the session reached terminal completion.
    pub is_complete: bool,
}

/// Callback trait for `previous_preset` lookups (DB access).
#[async_trait::async_trait]
pub trait PreviousPresetLookup: Send + Sync {
    /// Look up whether the named preset has the required status for the given
    /// `work_id` and `creator_id`.
    fn lookup(
        &self,
        preset_id: &str,
        work_id: &str,
        creator_id: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<PreviousPresetResult, GateEvalError>>
                + Send
                + '_,
        >,
    >;
}

// ---------------------------------------------------------------------------
// Gate evaluation error
// ---------------------------------------------------------------------------

/// Internal error during gate evaluation (not a gate failure per se).
#[derive(Debug, thiserror::Error)]
pub enum GateEvalError {
    /// Path safety violation.
    #[error("path safety violation for '{path}': {reason}")]
    PathSafety { path: String, reason: String },
    /// Database error during evaluation.
    #[error("database error: {0}")]
    Database(String),
    /// Unknown gate kind.
    #[error("unknown gate kind: {0}")]
    UnknownKind(String),
    /// Unknown operator.
    #[error("unknown gate operator: {0}")]
    UnknownOp(String),
    /// Unknown field name.
    #[error("unknown work field: {0}")]
    UnknownField(String),
}

// ---------------------------------------------------------------------------
// Gate evaluator
// ---------------------------------------------------------------------------

/// Evaluate a list of gates against the given context.
///
/// Returns `Ok(Ok(()))` if all gates pass, `Ok(Err(PresetGatesFailed))`
/// if any gate fails, or `Err(GateEvalError)` for internal errors.
///
/// # Errors
///
/// Returns `Err(GateEvalError)` on I/O failure or database lookup errors.
pub async fn evaluate_gates(
    gates: &[Gate],
    preset_id: &str,
    work: &WorkSnapshot,
    input: &PresetInput,
    workspace_root: &Path,
    previous_lookup: &dyn PreviousPresetLookup,
) -> Result<Result<(), PresetGatesFailed>, GateEvalError> {
    let mut failed: Vec<FailedGate> = Vec::new();

    for gate in gates {
        match gate {
            Gate::WorkField { field, op } => {
                let value = work.get_field(field);
                if !evaluate_work_field_op(op, value.as_ref()) {
                    let expected = describe_expected(op);
                    let actual = describe_actual(value.as_ref());
                    let remediation = work_field_remediation(field);
                    failed.push(FailedGate {
                        kind: "work_field".to_string(),
                        expected,
                        actual,
                        remediation,
                    });
                }
            }
            Gate::Filesystem { path, must_exist } => {
                let resolved = input.substitute_path(path)?;
                let full_path = workspace_root.join(&resolved);
                let exists = full_path.exists();
                let should_exist = *must_exist;

                if exists != should_exist {
                    let actual_str = if exists { "exists" } else { "missing" };
                    let expected_str = if should_exist {
                        "must exist"
                    } else {
                        "must not exist"
                    };
                    failed.push(FailedGate {
                        kind: "filesystem".to_string(),
                        expected: format!("{path}: {expected_str}"),
                        actual: actual_str.to_string(),
                        remediation: filesystem_remediation(should_exist, path),
                    });
                }
            }
            Gate::PreviousPreset {
                preset,
                status,
                scope: _,
            } => {
                let result = previous_lookup
                    .lookup(preset, &work.work_id, &work.creator_id)
                    .await?;
                let passed = match status {
                    PreviousPresetStatus::Complete => result.found && result.is_complete,
                    PreviousPresetStatus::AnySession => result.found,
                };
                if !passed {
                    let expected = match status {
                        PreviousPresetStatus::Complete => {
                            format!("preset '{preset}' must be complete")
                        }
                        PreviousPresetStatus::AnySession => {
                            format!("preset '{preset}' must have any session")
                        }
                    };
                    let actual_str = if result.found {
                        "found but not complete"
                    } else {
                        "not found"
                    };
                    failed.push(FailedGate {
                        kind: "previous_preset".to_string(),
                        expected,
                        actual: actual_str.to_string(),
                        remediation: previous_preset_remediation(preset),
                    });
                }
            }
        }
    }

    if failed.is_empty() {
        Ok(Ok(()))
    } else {
        Ok(Err(PresetGatesFailed {
            error: "preset_gates_failed".to_string(),
            preset_id: preset_id.to_string(),
            work_id: work.work_id.clone(),
            failed_gates: failed,
        }))
    }
}

// ---------------------------------------------------------------------------
// Gate evaluation helpers
// ---------------------------------------------------------------------------

/// Evaluate a `work_field` gate operator against the actual value.
fn evaluate_work_field_op(op: &GateOp, actual: Option<&serde_json::Value>) -> bool {
    match op {
        GateOp::Required => actual.is_some(),
        GateOp::Equals { value } => actual == Some(value),
        GateOp::NotEquals { value } => actual != Some(value),
        GateOp::In { value } => actual.is_some_and(|v| value.contains(v)),
        GateOp::NotIn { value } => !actual.is_some_and(|v| value.contains(v)),
    }
}

fn describe_expected(op: &GateOp) -> String {
    match op {
        GateOp::Required => "must be non-null".to_string(),
        GateOp::Equals { value } => format!("must equal {value}"),
        GateOp::NotEquals { value } => format!("must not equal {value}"),
        GateOp::In { value } => format!("must be one of {value:?}"),
        GateOp::NotIn { value } => format!("must not be one of {value:?}"),
    }
}

fn describe_actual(value: Option<&serde_json::Value>) -> String {
    value.map_or_else(|| "null".to_string(), |v| format!("{v}"))
}

fn work_field_remediation(field: &str) -> String {
    match field {
        "work_profile" => {
            "Ensure the Work has `work_profile: novel` set.".to_string()
        }
        "work_ref" => {
            "Run `creator run start --init-preset novel-project-init` to set work_ref.".to_string()
        }
        "intake_status" => {
            "Complete intake via `creator run stage advance --stage intake`.".to_string()
        }
        "world_id" => {
            "Set world_id via `creator run start --init-preset novel-project-init` or use `--force-gates`.".to_string()
        }
        "workspace_slug" => {
            "Ensure the workspace has a valid slug.".to_string()
        }
        _ => format!("Adjust the `{field}` field and retry."),
    }
}

fn filesystem_remediation(must_exist: bool, path: &str) -> String {
    if must_exist {
        if path.contains("Outlines") || path.contains("Stories") {
            format!(
                "Run `creator run start --init-preset novel-project-init` to scaffold `{path}`."
            )
        } else {
            format!("Ensure the path `{path}` exists before scheduling this preset.")
        }
    } else {
        format!("Remove or rename `{path}` before scheduling this preset.")
    }
}

fn previous_preset_remediation(preset: &str) -> String {
    match preset {
        "novel-project-init" => {
            "Run `creator run start --init-preset novel-project-init` first.".to_string()
        }
        "novel-writing" => "Run `creator run start` with a novel-writing preset first.".to_string(),
        _ => format!("Ensure preset '{preset}' has completed for this Work."),
    }
}

// ---------------------------------------------------------------------------
// Force-gates audit logging
// ---------------------------------------------------------------------------

/// Audit record for a forced gate bypass.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForceGatesAudit {
    /// Preset ID that was force-started.
    pub preset_id: String,
    /// Work ID.
    pub work_id: String,
    /// Creator ID who authorized the bypass.
    pub creator_id: String,
    /// User-provided reason text.
    pub reason: String,
    /// ISO-8601 timestamp.
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_work() -> WorkSnapshot {
        WorkSnapshot {
            work_id: "wrk_test".to_string(),
            creator_id: "c1".to_string(),
            work_profile: Some("novel".to_string()),
            work_ref: Some("my-novel".to_string()),
            workspace_slug: Some("default".to_string()),
            intake_status: Some("complete".to_string()),
            world_id: None,
            status: Some("active".to_string()),
            current_stage: Some("produce".to_string()),
            title: Some("Test Novel".to_string()),
            total_planned_chapters: Some(12),
        }
    }

    fn make_input() -> PresetInput {
        let mut vars = HashMap::new();
        vars.insert("work_ref".to_string(), "my-novel".to_string());
        vars.insert("work_id".to_string(), "wrk_test".to_string());
        PresetInput { vars }
    }

    struct MockPreviousLookup {
        found: bool,
        complete: bool,
    }

    #[async_trait::async_trait]
    impl PreviousPresetLookup for MockPreviousLookup {
        fn lookup(
            &self,
            _preset_id: &str,
            _work_id: &str,
            _creator_id: &str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<PreviousPresetResult, GateEvalError>>
                    + Send
                    + '_,
            >,
        > {
            let found = self.found;
            let complete = self.complete;
            Box::pin(async move {
                Ok(PreviousPresetResult {
                    found,
                    is_complete: complete,
                })
            })
        }
    }

    // ── work_field gates ──────────────────────────────────────────────

    #[tokio::test]
    async fn work_field_equals_passes() {
        let gates = vec![Gate::WorkField {
            field: "work_profile".to_string(),
            op: GateOp::Equals {
                value: serde_json::json!("novel"),
            },
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn work_field_equals_fails() {
        let gates = vec![Gate::WorkField {
            field: "work_profile".to_string(),
            op: GateOp::Equals {
                value: serde_json::json!("essay"),
            },
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error, "preset_gates_failed");
        assert_eq!(err.failed_gates.len(), 1);
        assert_eq!(err.failed_gates[0].kind, "work_field");
    }

    #[tokio::test]
    async fn work_field_required_passes() {
        let gates = vec![Gate::WorkField {
            field: "work_ref".to_string(),
            op: GateOp::Required,
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn work_field_required_fails_when_null() {
        let gates = vec![Gate::WorkField {
            field: "world_id".to_string(),
            op: GateOp::Required,
        }];
        let work = make_work(); // world_id = None
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.failed_gates[0].kind, "work_field");
    }

    #[tokio::test]
    async fn work_field_in_passes_with_null() {
        let gates = vec![Gate::WorkField {
            field: "work_profile".to_string(),
            op: GateOp::In {
                value: vec![serde_json::json!(null), serde_json::json!("novel")],
            },
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(
            &gates,
            "novel-project-init",
            &work,
            &input,
            tmp.path(),
            &lookup,
        )
        .await
        .unwrap();
        assert!(result.is_ok());
    }

    // ── filesystem gates ──────────────────────────────────────────────

    #[tokio::test]
    async fn filesystem_must_exist_passes() {
        let tmp = tempfile::tempdir().unwrap();
        let novel_dir = tmp.path().join("Works").join("my-novel");
        std::fs::create_dir_all(&novel_dir).unwrap();

        let gates = vec![Gate::Filesystem {
            path: "Works/{{work_ref}}/".to_string(),
            must_exist: true,
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn filesystem_must_exist_fails_when_missing() {
        let gates = vec![Gate::Filesystem {
            path: "Works/{{work_ref}}/Stories/".to_string(),
            must_exist: true,
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.failed_gates[0].kind, "filesystem");
        assert!(err.failed_gates[0]
            .remediation
            .contains("novel-project-init"));
    }

    #[tokio::test]
    async fn filesystem_path_traversal_rejected() {
        let gates = vec![Gate::Filesystem {
            path: "Works/{{work_ref}}/../../etc/".to_string(),
            must_exist: true,
        }];
        let work = make_work();
        let mut input = make_input();
        input
            .vars
            .insert("work_ref".to_string(), "../../etc".to_string());
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result =
            evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup).await;
        assert!(result.is_err());
    }

    // ── previous_preset gates ─────────────────────────────────────────

    #[tokio::test]
    async fn previous_preset_complete_passes() {
        let gates = vec![Gate::PreviousPreset {
            preset: "novel-project-init".to_string(),
            status: PreviousPresetStatus::Complete,
            scope: "work".to_string(),
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: true,
            complete: true,
        };

        let result = evaluate_gates(
            &gates,
            "novel-writing",
            &work,
            &input,
            tempfile::tempdir().unwrap().path(),
            &lookup,
        )
        .await
        .unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn previous_preset_complete_fails_when_not_found() {
        let gates = vec![Gate::PreviousPreset {
            preset: "novel-project-init".to_string(),
            status: PreviousPresetStatus::Complete,
            scope: "work".to_string(),
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };

        let result = evaluate_gates(
            &gates,
            "novel-writing",
            &work,
            &input,
            tempfile::tempdir().unwrap().path(),
            &lookup,
        )
        .await
        .unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.failed_gates[0].kind, "previous_preset");
    }

    #[tokio::test]
    async fn previous_preset_any_session_passes() {
        let gates = vec![Gate::PreviousPreset {
            preset: "novel-writing".to_string(),
            status: PreviousPresetStatus::AnySession,
            scope: "work".to_string(),
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: true,
            complete: false,
        };

        let result = evaluate_gates(
            &gates,
            "reflection-loop",
            &work,
            &input,
            tempfile::tempdir().unwrap().path(),
            &lookup,
        )
        .await
        .unwrap();
        assert!(result.is_ok());
    }

    // ── multiple gates ────────────────────────────────────────────────

    #[tokio::test]
    async fn multiple_gates_all_fail() {
        let gates = vec![
            Gate::WorkField {
                field: "work_profile".to_string(),
                op: GateOp::Equals {
                    value: serde_json::json!("essay"),
                },
            },
            Gate::Filesystem {
                path: "Works/{{work_ref}}/".to_string(),
                must_exist: true,
            },
        ];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.failed_gates.len(), 2);
    }

    // ── gate types roundtrip ──────────────────────────────────────────

    #[test]
    fn gate_yaml_roundtrip_work_field() {
        let gate = Gate::WorkField {
            field: "work_profile".to_string(),
            op: GateOp::Equals {
                value: serde_json::json!("novel"),
            },
        };
        let yaml = serde_yaml::to_string(&gate).unwrap();
        assert!(yaml.contains("work_field"));
        assert!(yaml.contains("equals"));
        let back: Gate = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(gate, back);
    }

    #[test]
    fn gate_yaml_roundtrip_filesystem() {
        let gate = Gate::Filesystem {
            path: "Works/{{work_ref}}/".to_string(),
            must_exist: true,
        };
        let yaml = serde_yaml::to_string(&gate).unwrap();
        assert!(yaml.contains("filesystem"));
        let back: Gate = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(gate, back);
    }

    #[test]
    fn gate_yaml_roundtrip_previous_preset() {
        let gate = Gate::PreviousPreset {
            preset: "novel-project-init".to_string(),
            status: PreviousPresetStatus::Complete,
            scope: "work".to_string(),
        };
        let yaml = serde_yaml::to_string(&gate).unwrap();
        assert!(yaml.contains("previous_preset"));
        let back: Gate = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(gate, back);
    }
}

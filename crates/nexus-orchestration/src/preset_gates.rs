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
    /// Returns `GateEvalError::PathSafety` if the result is absolute or
    /// canonicalizes outside the workspace root.
    pub fn substitute_path(&self, template: &str) -> Result<PathBuf, GateEvalError> {
        let mut result = template.to_string();
        for (k, v) in &self.vars {
            let pattern = format!("{{{{{k}}}}}");
            result = result.replace(&pattern, v);
        }
        // Path safety (spec §7.6.1): reject absolute paths before joining.
        if result.starts_with('/') {
            return Err(GateEvalError::PathSafety {
                path: template.to_string(),
                reason: "absolute path not allowed".to_string(),
            });
        }
        Ok(PathBuf::from(result))
    }

    /// Validate that a resolved path stays within `workspace_root` using
    /// canonicalize (spec §7.6.1).
    ///
    /// Returns the canonical `full_path` on success.
    ///
    /// # Errors
    ///
    /// Returns `GateEvalError::PathSafety` if the path does not exist, cannot
    /// be canonicalized, or escapes the workspace root (symlink / traversal).
    pub fn canonicalize_within(
        resolved: &Path,
        workspace_root: &Path,
    ) -> Result<PathBuf, GateEvalError> {
        let full_path = workspace_root.join(resolved);
        // If the path does not exist, fail early — gates check existence
        // separately, but canonicalize on a missing path errors.
        if !full_path.exists() {
            // Not a safety violation; return the joined path for existence
            // check downstream. Canonicalize is only needed for paths that
            // exist (to detect symlink escapes).
            return Ok(full_path);
        }
        let canonical_full =
            std::fs::canonicalize(&full_path).map_err(|e| GateEvalError::PathSafety {
                path: full_path.display().to_string(),
                reason: format!("canonicalize failed: {e}"),
            })?;
        let canonical_root =
            std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
        if !canonical_full.starts_with(&canonical_root) {
            return Err(GateEvalError::PathSafety {
                path: full_path.display().to_string(),
                reason: "path escapes workspace root (symlink or traversal)".to_string(),
            });
        }
        Ok(canonical_full)
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
                // Canonicalize to prevent symlink/traversal escape (spec §7.6.1).
                let full_path = PresetInput::canonicalize_within(&resolved, workspace_root)?;
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
    // V1.47 P1: normalize user-facing copy — spec names, not repo paths.
    // V1.46 P1 (spec hygiene): cite specs, not deleted quickstart.
    match field {
        "work_profile" => "Ensure the Work has `work_profile: novel` set. \
             See the creator-run-preset-entry spec"
            .to_string(),
        "work_ref" => "Run `creator bootstrap --init-preset novel-project-init` to set work_ref. \
             See the creator-run-preset-entry spec"
            .to_string(),
        // R-V146P1-QC3-S1: the previous remediation suggested
        // `creator bootstrap --preset creative-brief-intake`, which is wrong:
        // `--preset` overrides the PRODUCTION preset (not intake — intake is
        // always `creative-brief-intake` and hardcoded in bootstrap), and
        // `creator bootstrap` creates a NEW Work rather than completing
        // intake on the existing one. Per creator-run-preset-entry §3.2,
        // intake is triggered only via `creator bootstrap`. The honest
        // remediation is to tell the user intake runs via bootstrap.
        "intake_status" => "Intake (`creative-brief-intake`) has not completed on this Work. \
             Intake runs automatically during `nexus42 creator bootstrap`. \
             See novel-author-experience §3.2 for the onboarding path."
            .to_string(),
        "world_id" => "Create the World first via `nexus42 creator world create --title \"...\"` \
             or pick an existing one via `nexus42 creator world list`. \
             See the creator-run-preset-entry spec"
            .to_string(),
        "workspace_slug" => "Ensure the workspace has a valid slug.".to_string(),
        _ => format!("Adjust the `{field}` field and retry."),
    }
}

fn filesystem_remediation(must_exist: bool, path: &str) -> String {
    // V1.47 P1: normalize user-facing copy — spec names, not repo paths.
    // V1.46 P1 (spec hygiene): cite specs, not deleted quickstart.
    if must_exist {
        if path.contains("Outlines") || path.contains("Stories") {
            format!(
                "Run `creator bootstrap --init-preset novel-project-init` to scaffold `{path}`. \
                 See the creator-run-preset-entry spec"
            )
        } else {
            format!("Ensure the path `{path}` exists before scheduling this preset.")
        }
    } else {
        format!("Remove or rename `{path}` before scheduling this preset.")
    }
}

fn previous_preset_remediation(preset: &str) -> String {
    // V1.47 P1: normalize user-facing copy — spec names, not repo paths.
    // V1.46 P1 (spec hygiene): cite specs, not deleted quickstart.
    match preset {
        "novel-project-init" => "Run `creator bootstrap --init-preset novel-project-init` first. \
             See the creator-run-preset-entry spec"
            .to_string(),
        "novel-writing" => "Run `creator bootstrap` with a novel-writing preset first. \
             See novel-author-experience §3"
            .to_string(),
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
    async fn filesystem_path_traversal_rejected_by_canonicalize() {
        let tmp = tempfile::tempdir().unwrap();
        // Create a real directory outside the workspace to serve as an escape target.
        let outside = tempfile::tempdir().unwrap();
        // Create a symlink at Works/escape pointing outside the workspace.
        let works = tmp.path().join("Works");
        std::fs::create_dir_all(&works).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), works.join("escape")).unwrap();
        }

        let gates = vec![Gate::Filesystem {
            path: "Works/escape/".to_string(),
            must_exist: true,
        }];
        let work = make_work();
        let mut input = make_input();
        input
            .vars
            .insert("work_ref".to_string(), "escape".to_string());
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };

        let result =
            evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup).await;

        // On Unix: symlink exists but canonicalizes outside workspace → fail
        #[cfg(unix)]
        {
            assert!(result.is_err(), "symlink escape must be rejected");
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("escapes workspace"),
                "error should mention escape: {err}"
            );
        }
        // On non-Unix (no symlink), gate fails as missing
        #[cfg(not(unix))]
        {
            let _ = result;
        }
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
            "novel-chapter-review",
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

    // ── V1.39 P0.5 (T6): research preset gate integration ──────────────

    /// Research preset gates require intake_status == "complete" and
    /// work_ref to be present. Verify that a Work with intake complete
    /// and a work_ref passes all gates.
    #[tokio::test]
    async fn research_gates_pass_with_intake_complete_and_work_ref() {
        let gates = vec![
            Gate::WorkField {
                field: "intake_status".to_string(),
                op: GateOp::Equals {
                    value: serde_json::json!("complete"),
                },
            },
            Gate::WorkField {
                field: "work_ref".to_string(),
                op: GateOp::Required,
            },
        ];
        let work = make_work(); // intake_status=complete, work_ref=Some
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "research", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(
            result.is_ok(),
            "research gates should pass with intake complete and work_ref"
        );
    }

    /// Research preset gates fail when intake_status is not "complete".
    #[tokio::test]
    async fn research_gates_fail_when_intake_not_complete() {
        let gates = vec![Gate::WorkField {
            field: "intake_status".to_string(),
            op: GateOp::Equals {
                value: serde_json::json!("complete"),
            },
        }];
        let mut work = make_work();
        work.intake_status = Some("pending".to_string());
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "research", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(
            result.is_err(),
            "research gates should fail when intake is not complete"
        );
        let err = result.unwrap_err();
        assert_eq!(err.failed_gates[0].kind, "work_field");
    }

    /// Research preset gates fail when work_ref is missing.
    #[tokio::test]
    async fn research_gates_fail_when_work_ref_missing() {
        let gates = vec![Gate::WorkField {
            field: "work_ref".to_string(),
            op: GateOp::Required,
        }];
        let mut work = make_work();
        work.work_ref = None;
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "research", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        assert!(
            result.is_err(),
            "research gates should fail when work_ref is missing"
        );
    }

    // ── V1.43 P1 remediation citation tests ──────────────────────────────

    /// V1.43 (P1 §3 remediation — preset_gates_failed): work_field remediation
    /// strings cite quickstart §2/§3.
    #[tokio::test]
    async fn remediation_work_field_cites_quickstart() {
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
        let err = result.unwrap_err();
        assert!(
            err.failed_gates[0]
                .remediation
                .contains("creator-run-preset-entry"),
            "work_field remediation should cite the spec: {:?}",
            err.failed_gates[0].remediation
        );
        // R-V146P1-QC3-S4: no raw .mstar/ paths in user-facing copy.
        assert!(
            !err.failed_gates[0].remediation.contains(".mstar/"),
            "work_field remediation must not cite raw .mstar/ paths: {:?}",
            err.failed_gates[0].remediation
        );
    }

    /// V1.46 P1 (spec hygiene): filesystem gate remediation cites the
    /// preset-entry spec when scaffold paths are missing.
    #[tokio::test]
    async fn remediation_filesystem_scaffold_cites_preset_entry_spec() {
        let gates = vec![Gate::Filesystem {
            path: "Works/{{work_ref}}/Outlines/ch01-outline.md".to_string(),
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
        let err = result.unwrap_err();
        assert!(
            err.failed_gates[0]
                .remediation
                .contains("creator-run-preset-entry"),
            "scaffold remediation should cite the preset-entry spec: {:?}",
            err.failed_gates[0].remediation
        );
        assert!(
            !err.failed_gates[0].remediation.contains(".mstar/"),
            "scaffold remediation must not cite raw .mstar/ paths: {:?}",
            err.failed_gates[0].remediation
        );
    }

    /// V1.46 P1 (spec hygiene): previous_preset remediation for
    /// novel-project-init cites the preset-entry spec.
    #[tokio::test]
    async fn remediation_previous_preset_init_cites_preset_entry_spec() {
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
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        let err = result.unwrap_err();
        assert!(
            err.failed_gates[0]
                .remediation
                .contains("creator-run-preset-entry"),
            "previous_preset init remediation should cite the spec: {:?}",
            err.failed_gates[0].remediation
        );
        assert!(
            !err.failed_gates[0].remediation.contains(".mstar/"),
            "previous_preset init remediation must not cite raw .mstar/ paths: {:?}",
            err.failed_gates[0].remediation
        );
    }

    /// V1.46 P1 (spec hygiene): previous_preset remediation for
    /// novel-writing cites the author-experience spec.
    #[tokio::test]
    async fn remediation_previous_preset_writing_cites_author_experience_spec() {
        let gates = vec![Gate::PreviousPreset {
            preset: "novel-writing".to_string(),
            status: PreviousPresetStatus::Complete,
            scope: "work".to_string(),
        }];
        let work = make_work();
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "review", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        let err = result.unwrap_err();
        assert!(
            err.failed_gates[0]
                .remediation
                .contains("novel-author-experience"),
            "previous_preset writing remediation should cite the spec: {:?}",
            err.failed_gates[0].remediation
        );
        assert!(
            !err.failed_gates[0].remediation.contains(".mstar/"),
            "previous_preset writing remediation must not cite raw .mstar/ paths: {:?}",
            err.failed_gates[0].remediation
        );
    }

    // ── V1.47 P1 intake remediation tests (R-V146P1-QC3-S1) ──────────────

    /// R-V146P1-QC3-S1: the `intake_status` gate remediation must cite an
    /// executable `creator bootstrap` command, not the broken
    /// `creator bootstrap --preset creative-brief-intake` (which misuses
    /// `--preset` — that flag overrides the PRODUCTION preset, not intake).
    ///
    /// Per `creator-run-preset-entry.md` §3.2, intake is triggered only via
    /// `creator bootstrap`; the remediation must reference that command
    /// without the misleading `--preset creative-brief-intake` suffix.
    #[tokio::test]
    async fn intake_status_remediation_cites_executable_bootstrap() {
        let gates = vec![Gate::WorkField {
            field: "intake_status".to_string(),
            op: GateOp::Equals {
                value: serde_json::json!("complete"),
            },
        }];
        let mut work = make_work();
        // Force intake to be incomplete so the gate actually fails.
        work.intake_status = Some("pending".to_string());
        let input = make_input();
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        let result = evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
            .await
            .unwrap();
        let err = result.unwrap_err();
        let remediation = &err.failed_gates[0].remediation;

        // Must cite an executable `creator bootstrap` command.
        assert!(
            remediation.contains("creator bootstrap"),
            "intake_status remediation should cite `creator bootstrap`: {remediation:?}"
        );
        // Must NOT suggest the broken `--preset creative-brief-intake`
        // (that overrides the production preset, not intake).
        assert!(
            !remediation.contains("--preset creative-brief-intake"),
            "intake_status remediation must not suggest broken `--preset creative-brief-intake`: {remediation:?}"
        );
        // Must not cite raw .mstar/ paths.
        assert!(
            !remediation.contains(".mstar/"),
            "intake_status remediation must not cite raw .mstar/ paths: {remediation:?}"
        );
    }

    /// R-V146P1-QC3-S4 regression guard: no user-facing remediation string
    /// produced by any gate helper may embed raw `.mstar/knowledge/specs/`
    /// paths. Exercises every `work_field_remediation` branch plus the
    /// filesystem and previous_preset helpers.
    #[tokio::test]
    async fn no_gate_remediation_embeds_raw_dotmstar_paths() {
        let lookup = MockPreviousLookup {
            found: false,
            complete: false,
        };
        let tmp = tempfile::tempdir().unwrap();

        // Collect remediation strings from every gate helper path.
        let mut all_remediations: Vec<String> = Vec::new();

        // work_field gates — each branch in the match.
        for (field, op_value) in [
            ("work_profile", serde_json::json!("essay")),
            ("work_ref", serde_json::json!("some-ref")),
            ("intake_status", serde_json::json!("complete")),
            ("world_id", serde_json::json!("wld_abc")),
            ("workspace_slug", serde_json::json!("my-slug")),
        ] {
            let gates = vec![Gate::WorkField {
                field: field.to_string(),
                op: GateOp::Equals { value: op_value },
            }];
            let work = make_work();
            let input = make_input();
            if let Err(err) =
                evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
                    .await
                    .unwrap()
            {
                for g in &err.failed_gates {
                    all_remediations.push(g.remediation.clone());
                }
            }
        }

        // Force-collect the intake_status remediation explicitly (the
        // default make_work has intake_status=complete, so the gate above
        // passes without producing a remediation string).
        {
            let gates = vec![Gate::WorkField {
                field: "intake_status".to_string(),
                op: GateOp::Equals {
                    value: serde_json::json!("complete"),
                },
            }];
            let mut work = make_work();
            work.intake_status = Some("pending".to_string());
            let input = make_input();
            if let Err(err) =
                evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
                    .await
                    .unwrap()
            {
                for g in &err.failed_gates {
                    all_remediations.push(g.remediation.clone());
                }
            }
        }

        // filesystem gate — scaffold path.
        let gates = vec![Gate::Filesystem {
            path: "Works/{{work_ref}}/Outlines/ch01-outline.md".to_string(),
            must_exist: true,
        }];
        let work = make_work();
        let input = make_input();
        if let Err(err) =
            evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
                .await
                .unwrap()
        {
            for g in &err.failed_gates {
                all_remediations.push(g.remediation.clone());
            }
        }

        // previous_preset gates — both documented branches.
        for preset in ["novel-project-init", "novel-writing"] {
            let gates = vec![Gate::PreviousPreset {
                preset: preset.to_string(),
                status: PreviousPresetStatus::Complete,
                scope: "work".to_string(),
            }];
            let work = make_work();
            let input = make_input();
            if let Err(err) =
                evaluate_gates(&gates, "novel-writing", &work, &input, tmp.path(), &lookup)
                    .await
                    .unwrap()
            {
                for g in &err.failed_gates {
                    all_remediations.push(g.remediation.clone());
                }
            }
        }

        assert!(
            !all_remediations.is_empty(),
            "test harness should have collected at least some remediation strings"
        );
        for msg in &all_remediations {
            assert!(
                !msg.contains(".mstar/"),
                "gate remediation must not embed raw .mstar/ paths: {msg:?}"
            );
        }
    }
}

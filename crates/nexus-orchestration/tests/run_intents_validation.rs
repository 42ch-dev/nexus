//! Run-intents validation tests (R-V133P1-12).
//!
//! Migrated from inline `#[cfg(test)]` in `validation.rs` to standalone
//! integration test binary per T1 §7.5 contract.

use nexus_orchestration::capability::CapabilityRegistry;
use nexus_orchestration::preset::manifest::PresetManifest;
use nexus_orchestration::preset::validation::{
    validate_preset_semantic, DiagnosticCategory, DiagnosticSeverity,
};

fn test_caps() -> CapabilityRegistry {
    CapabilityRegistry::with_builtins()
}

#[test]
fn creator_preset_without_run_intents_is_error() {
    let yaml = r"
preset:
  id: no-intents
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
    let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
    let result = validate_preset_semantic(&manifest, &test_caps());
    assert!(
        result.has_errors(),
        "empty run_intents on creator should be an error"
    );
    let diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::RunIntents)
        .collect();
    assert_eq!(diags.len(), 1, "expected exactly 1 RunIntents diagnostic");
    assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
    assert!(
        diags[0]
            .message
            .contains("must declare at least one run_intent"),
        "message should say 'must': {:?}",
        diags[0].message
    );
}

#[test]
fn creator_preset_with_run_intents_passes() {
    let yaml = r"
preset:
  id: with-intents
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init, work_continue]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
    let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
    let result = validate_preset_semantic(&manifest, &test_caps());
    let run_intent_errors: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::RunIntents)
        .collect();
    assert!(
        run_intent_errors.is_empty(),
        "creator with run_intents should not generate RunIntents diagnostics: {run_intent_errors:?}",
    );
}

#[test]
fn system_preset_without_system_maintenance_is_warning() {
    let yaml = r"
preset:
  id: sys-no-maint
  version: 1
  kind: system
  description: test
  requires_capabilities: []
  run_intents: [system_maintenance]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
    let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
    let result = validate_preset_semantic(&manifest, &test_caps());
    let diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::RunIntents)
        .collect();
    assert!(
        diags.is_empty(),
        "system preset with system_maintenance should have no RunIntents diagnostics: {diags:?}",
    );
}

/// R-V133P1-05: creator preset claiming `system_maintenance` is an Error.
#[test]
fn creator_preset_with_system_maintenance_is_error() {
    let yaml = r"
preset:
  id: creator-sys-claim
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init, system_maintenance]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
    let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
    let result = validate_preset_semantic(&manifest, &test_caps());
    let diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::RunIntents)
        .collect();
    assert_eq!(diags.len(), 1, "expected exactly 1 RunIntents diagnostic");
    assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
    assert!(
        diags[0].message.contains("system_maintenance"),
        "message should mention system_maintenance: {:?}",
        diags[0].message
    );
}

/// R-V133P1-05: system preset claiming `work_init` (creator-only) is an Error.
#[test]
fn system_preset_with_creator_intent_is_error() {
    let yaml = r"
preset:
  id: sys-creator-claim
  version: 1
  kind: system
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
    let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
    let result = validate_preset_semantic(&manifest, &test_caps());
    let diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::RunIntents)
        .collect();
    // Expect both: Error for cross-claim + Warning for missing system_maintenance
    assert!(
        !diags.is_empty(),
        "expected at least 1 RunIntents diagnostic: {diags:?}",
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "expected exactly 1 Error for cross-claim");
    assert!(
        errors[0].message.contains("creator-only"),
        "error should mention creator-only: {:?}",
        errors[0].message
    );
}

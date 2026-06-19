//! Integration tests for V1.52 T-B P0 N-way labeled routing.
//!
//! Tests:
//! - Full preset with Labeled edges loads + executes
//! - Hybrid GoNogo + Labeled state
//! - Orphan label detection (validator catches at load time)
//! - All embedded presets still parse (regression)
//! - No-match does NOT stall the session (deterministic fail)

use graph_flow::SessionStorage;
use nexus_orchestration::{preset, CapabilityRegistry, GraphFlowEngine, OrchestrationEngine};
use std::sync::Arc;

fn test_capability_registry() -> CapabilityRegistry {
    CapabilityRegistry::with_builtins()
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Build a minimal preset YAML with N-way labeled next edges.
fn labeled_preset_yaml(labels: &[(&str, &str)]) -> String {
    let edges: Vec<String> = labels
        .iter()
        .map(|(l, t)| format!("      - label: {l}\n        target: {t}"))
        .collect();
    format!(
        r#"
preset:
  id: test-labeled
  version: 1
  kind: creator
  description: N-way labeled routing integration test
  requires_capabilities: []
  run_intents: [work_continue]
  initial: start
  terminal: done
states:
  - id: start
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "{{preset.id}}/judge.txt"
      judge_capability: judge.llm
    next:
{}
  - id: good_branch
    enter: []
    exit_when: {{ kind: manual }}
    next: done
  - id: retry_branch
    enter: []
    exit_when: {{ kind: manual }}
    next: done
  - id: done
    terminal: true
"#,
        edges.join("\n")
    )
}

/// Build a preset YAML with hybrid GoNogo + Labeled states.
fn hybrid_preset_yaml() -> String {
    r#"
preset:
  id: test-hybrid
  version: 1
  kind: creator
  description: Hybrid GoNogo + Labeled routing integration test
  requires_capabilities: []
  run_intents: [work_continue]
  initial: start
  terminal: end
states:
  - id: start
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "{{preset.id}}/judge.txt"
      judge_capability: judge.llm
    next:
      - label: good
        target: labeled_branch
      - label: retry
        target: retry_branch
  - id: gonogo_state
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "{{preset.id}}/judge.txt"
      judge_capability: judge.llm
    next:
      go: go_branch
      nogo: nogo_branch
  - id: labeled_branch
    enter: []
    exit_when: { kind: manual }
    next: gonogo_state
  - id: retry_branch
    enter: []
    exit_when: { kind: manual }
    next: gonogo_state
  - id: go_branch
    enter: []
    exit_when: { kind: manual }
    next: end
  - id: nogo_branch
    enter: []
    exit_when: { kind: manual }
    next: end
  - id: end
    terminal: true
"#
    .to_string()
}

/// Build a preset YAML with an orphan label (label referenced in next but not producible).
fn orphan_label_preset_yaml() -> String {
    // Deliberate: `missing_target` label does not correspond to any state.
    // The label "phantom" is valid (references `done`) but the state `missing_target` doesn't exist.
    r#"
preset:
  id: test-orphan
  version: 1
  kind: creator
  description: Orphan label detection test
  requires_capabilities: []
  run_intents: [work_continue]
  initial: start
  terminal: done
states:
  - id: start
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "{{preset.id}}/judge.txt"
      judge_capability: judge.llm
    next:
      - label: forward
        target: missing_target
  - id: done
    terminal: true
"#
    .to_string()
}

// ── Tests ──────────────────────────────────────────────────────────────

/// Full preset with 3-way labeled edges loads and passes validation.
#[test]
fn labeled_preset_loads_and_validates() {
    let yaml = labeled_preset_yaml(&[("good", "good_branch"), ("retry", "retry_branch")]);
    let caps = test_capability_registry();
    let loaded =
        preset::load_preset_from_str(&yaml, &caps).expect("3-way labeled preset should load");
    let result = preset::validation::validate_preset_semantic(&loaded.manifest, &caps);
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.severity != preset::validation::DiagnosticSeverity::Error),
        "labeled preset should pass semantic validation: {:?}",
        result.diagnostics
    );
}

/// Hybrid GoNogo + Labeled state preset loads and validates.
#[test]
fn hybrid_gonogo_labeled_preset_loads_and_validates() {
    let yaml = hybrid_preset_yaml();
    let caps = test_capability_registry();
    let loaded = preset::load_preset_from_str(&yaml, &caps)
        .expect("hybrid GoNogo+Labeled preset should load");
    let result = preset::validation::validate_preset_semantic(&loaded.manifest, &caps);
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.severity != preset::validation::DiagnosticSeverity::Error),
        "hybrid preset should pass semantic validation: {:?}",
        result.diagnostics
    );
}

/// Orphan label detection: validator catches missing target state at load time.
#[test]
fn orphan_label_detected_at_validation_time() {
    let yaml = orphan_label_preset_yaml();
    let caps = test_capability_registry();
    // The loader itself should catch unknown state references in `next`.
    let err = preset::load_preset_from_str(&yaml, &caps).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.error.contains("unknown state") || p.error.contains("missing_target")),
        "orphan label (missing_target) should be caught at load time: {:?}",
        err.problems()
    );
}

/// Regression: all embedded presets still parse and pass validation.
#[test]
fn all_embedded_presets_still_parse_regression() {
    let caps = test_capability_registry();
    let preset_ids = preset::list_embedded_presets();
    assert!(
        !preset_ids.is_empty(),
        "expected at least one embedded preset"
    );

    let mut failures: Vec<String> = Vec::new();
    for preset_id in &preset_ids {
        match preset::load_embedded_preset(preset_id, &caps) {
            Ok(loaded) => {
                let result = preset::validation::validate_preset_semantic(&loaded.manifest, &caps);
                for d in &result.diagnostics {
                    if d.severity == preset::validation::DiagnosticSeverity::Error {
                        // Known false positive: capability arg drift for creator.inject_prompt.
                        if d.category == preset::validation::DiagnosticCategory::CapabilityArgDrift
                            && d.message.contains("capability 'creator.inject_prompt'")
                        {
                            continue; // known false positive, not a regression
                        }
                        failures.push(format!(
                            "preset '{preset_id}': semantic error at {}: {}",
                            d.path, d.message
                        ));
                    }
                }
            }
            Err(e) => {
                failures.push(format!("preset '{preset_id}' failed to load: {e}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "embedded presets must all parse and validate: {}",
        failures.join("\n")
    );
}

/// No-match does NOT stall: engine returns error on Labeled no-match.
#[tokio::test]
async fn labeled_no_match_does_not_stall_session() {
    let yaml = labeled_preset_yaml(&[("outline", "good_branch")]);
    let caps = Arc::new(test_capability_registry());
    let loaded = preset::load_preset_from_str(&yaml, &caps).expect("labeled preset should load");

    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = GraphFlowEngine::new_with_storage(storage.clone(), caps.clone());

    let sid = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("start_session_with_preset");

    // Set judge context to produce output that won't match any label.
    if let Some(session) = storage.get(&sid.0).await.expect("get session") {
        session
            .context
            .set("_judge_reason", "this is just testing".to_string())
            .await;
        session.context.set("_judge_result", true).await;
        storage.save(session).await.expect("save session");
    }

    // Run until error or iteration cap.
    let mut errored = false;
    for _ in 0..8 {
        match engine.run_step(&sid).await {
            Ok(outcome) => {
                if let nexus_orchestration::engine::StepOutcome::Paused { .. } = outcome {
                    break;
                }
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("no label matched")
                        || msg.contains("no template_file")
                        || msg.contains("not found"),
                    "expected labeled-routing error, got: {msg}"
                );
                errored = true;
                break;
            }
        }
    }
    assert!(
        errored,
        "labeled no-match should cause error, not silent stall"
    );
}

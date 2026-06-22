//! Expression routing integration tests (V1.58 P2 — R-V156P2-M003).
//!
//! Exercises expression routing through the **full dispatch path**:
//! preset YAML load → expression cache build (parser + context-dep scanner)
//! → `StateCompositeTask::run()` → `resolve_expression_target()` → GoTo transition.
//!
//! Unlike the unit tests in `src/tasks/mod.rs` which construct `StateCompositeTask`
//! by hand, these tests load presets from YAML strings, proving the loader,
//! expression parser, context-dependency scanner, and task-construction paths
//! all wire correctly end-to-end.
//!
//! # Test contract
//!
//! Each test:
//! 1. Defines a preset YAML with `next: { branches: [...], default: ... }`.
//! 2. Loads it via `preset::load_preset_from_str` (exercises the real loader).
//! 3. Extracts the start task from the outer graph.
//! 4. Pre-populates context with expression-relevant values.
//! 5. Runs the task and asserts `NextAction::GoTo(target)` matches the expected
//!    branch or default.

use graph_flow::{Context, NextAction};
use nexus_orchestration::preset;
use nexus_orchestration::CapabilityRegistry;

/// Load a preset YAML, extract the start task, run it with the given context
/// values, and return the resulting `NextAction`.
async fn run_expression_route(
    yaml: &str,
    context_values: &[(&str, serde_json::Value)],
) -> NextAction {
    let caps = CapabilityRegistry::with_builtins();
    let loaded =
        preset::load_preset_from_str(yaml, &caps).expect("preset YAML should load successfully");

    let start_id = loaded.manifest.preset.initial.as_str();
    let task = loaded
        .outer_graph
        .get_task(start_id)
        .expect("start task should exist in the loaded outer graph");

    let ctx = Context::new();
    for (key, value) in context_values {
        ctx.set_sync(*key, value.clone());
    }

    let result = task.run(ctx).await.expect("task run should succeed");
    result.next_action
}

/// Helper: assert a `NextAction` is `GoTo` with the expected target.
fn assert_goto(action: &NextAction, expected: &str) {
    match action {
        NextAction::GoTo(target) => assert!(
            target == expected,
            "expression routing should go to '{expected}', got '{target}'"
        ),
        other => panic!("expected GoTo('{expected}'), got {other:?}"),
    }
}

// ── Test 1: numeric threshold branch matches ──────────────────────────

/// A preset with `branches` routing on a numeric threshold.
/// When `score > 80`, routes to `high_path`; otherwise falls to `default: standard_path`.
const THRESHOLD_YAML: &str = r#"
preset:
  id: expr-threshold
  version: 1
  kind: creator
  description: "expression routing threshold test"
  requires_capabilities: []
  run_intents: [work_continue]
  initial: evaluate
  terminal: done
states:
  - id: evaluate
    enter: []
    next:
      branches:
        - when: "_context.score > 80"
          target: high_path
      default: standard_path
  - id: high_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: standard_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;

#[tokio::test]
async fn expression_routes_to_high_path_when_score_above_threshold() {
    let action = run_expression_route(THRESHOLD_YAML, &[("score", serde_json::json!(95))]).await;
    assert_goto(&action, "high_path");
}

// ── Test 2: no branch matches → default ───────────────────────────────

#[tokio::test]
async fn expression_routes_to_default_when_no_branch_matches() {
    let action = run_expression_route(THRESHOLD_YAML, &[("score", serde_json::json!(50))]).await;
    assert_goto(&action, "standard_path");
}

// ── Test 3: string equality multi-branch ──────────────────────────────

/// A preset with multiple string-equality branches.
const STRING_MATCH_YAML: &str = r#"
preset:
  id: expr-string-match
  version: 1
  kind: creator
  description: "expression routing string match test"
  requires_capabilities: []
  run_intents: [work_continue]
  initial: route
  terminal: done
states:
  - id: route
    enter: []
    next:
      branches:
        - when: "_context.status == 'draft'"
          target: draft_path
        - when: "_context.status == 'review'"
          target: review_path
        - when: "_context.status == 'published'"
          target: published_path
      default: unknown_path
  - id: draft_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: review_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: published_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: unknown_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;

#[tokio::test]
async fn expression_routes_to_matching_string_branch() {
    let action = run_expression_route(
        STRING_MATCH_YAML,
        &[("status", serde_json::json!("review"))],
    )
    .await;
    assert_goto(&action, "review_path");
}

#[tokio::test]
async fn expression_routes_to_default_for_unmatched_string() {
    let action = run_expression_route(
        STRING_MATCH_YAML,
        &[("status", serde_json::json!("archived"))],
    )
    .await;
    assert_goto(&action, "unknown_path");
}

// ── Test 4: registry.refresh dependency → synthetic fallback ──────────

/// A preset whose branch expression references `_context.registry_refresh.*`.
/// Since the unwired test graph has no registry, `inject_registry_refresh_context`
/// falls back to the synthetic output (`source: "synthetic"`), which the
/// expression should match.
const REGISTRY_DEP_YAML: &str = r#"
preset:
  id: expr-registry-dep
  version: 1
  kind: creator
  description: "expression routing with registry.refresh dependency"
  requires_capabilities: []
  run_intents: [work_continue]
  initial: check
  terminal: done
states:
  - id: check
    enter: []
    next:
      branches:
        - when: "_context.registry_refresh.source == 'synthetic'"
          target: synthetic_path
        - when: "_context.registry_refresh.capability_count > 50"
          target: high_cap_path
      default: standard_path
  - id: synthetic_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: high_cap_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: standard_path
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;

#[tokio::test]
async fn expression_routes_to_synthetic_when_registry_unwired() {
    // The unwired graph has self.registry = None, so
    // inject_registry_refresh_context uses the synthetic fallback
    // (source = "synthetic"). The first branch should match.
    let action = run_expression_route(REGISTRY_DEP_YAML, &[]).await;
    assert_goto(&action, "synthetic_path");
}

// ── Test 5: compound boolean expression ───────────────────────────────

/// A preset with a compound `&&` expression requiring two conditions.
const COMPOUND_YAML: &str = r#"
preset:
  id: expr-compound
  version: 1
  kind: creator
  description: "compound boolean expression routing"
  requires_capabilities: []
  run_intents: [work_continue]
  initial: decide
  terminal: done
states:
  - id: decide
    enter: []
    next:
      branches:
        - when: "_context.score > 80 && _context.status == 'active'"
          target: fast_track
        - when: "_context.score > 60"
          target: normal_track
      default: slow_track
  - id: fast_track
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: normal_track
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: slow_track
    enter: []
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;

#[tokio::test]
async fn expression_compound_both_conditions_match_first_branch() {
    let action = run_expression_route(
        COMPOUND_YAML,
        &[
            ("score", serde_json::json!(90)),
            ("status", serde_json::json!("active")),
        ],
    )
    .await;
    assert_goto(&action, "fast_track");
}

#[tokio::test]
async fn expression_compound_partial_match_falls_to_second_branch() {
    // score > 80 is true but status != 'active', so first branch fails.
    // score > 60 is true, so second branch matches.
    let action = run_expression_route(
        COMPOUND_YAML,
        &[
            ("score", serde_json::json!(70)),
            ("status", serde_json::json!("inactive")),
        ],
    )
    .await;
    assert_goto(&action, "normal_track");
}

#[tokio::test]
async fn expression_compound_no_match_falls_to_default() {
    let action = run_expression_route(
        COMPOUND_YAML,
        &[
            ("score", serde_json::json!(30)),
            ("status", serde_json::json!("inactive")),
        ],
    )
    .await;
    assert_goto(&action, "slow_track");
}

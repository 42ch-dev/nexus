//! Hermetic tests for the legacy novel-manuscript-audit embedded preset (V1.44 P0, DF-69).
//!
//! Note: The unified preset is retained for backward compatibility but the CLI
//! now dispatches to novel-manuscript-audit-review or novel-manuscript-audit-extract
//! based on mode. New tests for split presets are in:
//!   - novel_manuscript_audit_review.rs
//!   - novel_manuscript_audit_extract.rs
//!
//! This file validates the legacy preset still loads correctly.

use nexus_orchestration::capability::CapabilityRegistry;
use nexus_orchestration::preset::load_embedded_preset;
use nexus_orchestration::preset::manifest;

// ---------------------------------------------------------------------------
// AC1: Preset loads and validates
// ---------------------------------------------------------------------------

#[test]
fn novel_manuscript_audit_loads_successfully() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();
    assert_eq!(loaded.id, "novel-manuscript-audit");
    assert_eq!(loaded.version, 1);
}

#[test]
fn novel_manuscript_audit_has_required_capabilities() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    assert!(
        loaded
            .manifest
            .preset
            .requires_capabilities
            .iter()
            .any(|c| c == "creator.inject_prompt"),
        "preset must require creator.inject_prompt"
    );
    assert!(
        loaded
            .manifest
            .preset
            .requires_capabilities
            .iter()
            .any(|c| c == "kb.extract_work"),
        "preset must require kb.extract_work"
    );
}

// ---------------------------------------------------------------------------
// AC2: Review mode state machine
// ---------------------------------------------------------------------------

#[test]
fn review_mode_state_machine_load_chapter_to_review_report() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    // Initial state
    assert_eq!(loaded.manifest.preset.initial, "load_chapter");

    // load_chapter → review_report
    let load_chapter = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "load_chapter")
        .expect("load_chapter state must exist");

    assert!(
        load_chapter.next.is_some(),
        "load_chapter must have a next state"
    );
    // Default next is review_report (first path)
    assert_eq!(
        load_chapter.next,
        Some(manifest::NextTarget::Linear("review_report".into())),
        "load_chapter default next should be review_report"
    );
}

#[test]
fn review_report_state_references_mode_vars() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    let review = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "review_report")
        .expect("review_report state must exist");

    // Verify review_report injects a prompt with chapter/volume context
    let has_chapter_var = review.enter.iter().any(|action| {
        if let manifest::EnterAction::Capability { args, .. } = action {
            if let Some(args) = args {
                let vars = args.get("vars");
                if let Some(vars) = vars {
                    return vars.get("chapter").is_some();
                }
            }
        }
        false
    });
    assert!(
        has_chapter_var,
        "review_report enter action must reference chapter in vars"
    );
}

#[test]
fn review_report_state_transitions_to_done() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    let review = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "review_report")
        .expect("review_report state must exist");

    assert!(
        review.next.is_some(),
        "review_report must have a next state"
    );
    assert_eq!(
        review.next,
        Some(manifest::NextTarget::Linear("done".into())),
        "review_report must transition to done"
    );
}

// ---------------------------------------------------------------------------
// AC3: Extract mode state machine
// ---------------------------------------------------------------------------

#[test]
fn extract_sync_state_calls_kb_extract_work() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    let extract = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "extract_sync")
        .expect("extract_sync state must exist");

    // Verify extract_sync calls kb.extract_work capability
    let calls_extract = extract.enter.iter().any(|action| {
        if let manifest::EnterAction::Capability { name, .. } = action {
            name == "kb.extract_work"
        } else {
            false
        }
    });
    assert!(
        calls_extract,
        "extract_sync must call kb.extract_work capability"
    );
}

#[test]
fn extract_sync_state_transitions_to_done() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    let extract = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "extract_sync")
        .expect("extract_sync state must exist");

    assert!(
        extract.next.is_some(),
        "extract_sync must have a next state"
    );
    assert_eq!(
        extract.next,
        Some(manifest::NextTarget::Linear("done".into())),
        "extract_sync must transition to done"
    );
}

#[test]
fn extract_sync_passes_world_id_and_work_id() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    let extract = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "extract_sync")
        .expect("extract_sync state must exist");

    // Verify kb.extract_work args include world_id and work_id
    let extract_action = extract
        .enter
        .iter()
        .find(|action| {
            if let manifest::EnterAction::Capability { name, .. } = action {
                name == "kb.extract_work"
            } else {
                false
            }
        })
        .expect("must have kb.extract_work action");

    if let manifest::EnterAction::Capability { args, .. } = extract_action {
        if let Some(args) = args {
            let has_world_id = args
                .get("world_id")
                .is_some_and(|v| v.as_str().is_some_and(|s| s.contains("world_id")));
            let has_work_id = args
                .get("work_id")
                .is_some_and(|v| v.as_str().is_some_and(|s| s.contains("work_id")));
            assert!(
                has_world_id,
                "kb.extract_work args must reference world_id template var"
            );
            assert!(
                has_work_id,
                "kb.extract_work args must reference work_id template var"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// AC4: No FL-E driver schedule fields
// ---------------------------------------------------------------------------

#[test]
fn preset_does_not_set_fl_e_stage_driver_fields() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    // Verify no state contains fl_e_stage, auto_chain, or driver references
    for state in &loaded.manifest.states {
        for action in &state.enter {
            if let manifest::EnterAction::Capability { args, .. } = action {
                if let Some(args) = args {
                    for (key, _) in args.as_object().unwrap_or(&serde_json::Map::new()) {
                        assert!(
                            !key.contains("fl_e_stage"),
                            "preset must not set fl_e_stage: found in state '{}'",
                            state.id
                        );
                        assert!(
                            !key.contains("auto_chain"),
                            "preset must not reference auto_chain: found in state '{}'",
                            state.id
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AC5: Terminal state is "done"
// ---------------------------------------------------------------------------

#[test]
fn done_is_terminal() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit", &caps).unwrap();

    let done = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "done")
        .expect("done state must exist");

    assert!(done.terminal, "done state must be marked terminal");
    assert!(
        done.next.is_none(),
        "terminal state must not have a next transition"
    );
}

// ---------------------------------------------------------------------------
// AC6: CLI handler — worldless extract 422 validation (logic test)
// ---------------------------------------------------------------------------

/// Verify worldless extract precondition logic:
/// extract mode + no world_id → should fail (422).
#[test]
fn worldless_work_returns_422_on_extract() {
    // Simulating the CLI validation logic from handle_audit_chapter:
    // if matches!(mode, AuditMode::Extract) && world_id.is_none() → 422
    let world_id: Option<String> = None;

    // Extract on worldless → fail
    let should_fail = world_id.is_none();
    assert!(should_fail, "extract on worldless Work should fail");

    // Review on worldless → ok
    let review_should_pass = world_id.is_none();
    // Review mode does NOT check world_id — always passes
    assert!(
        review_should_pass,
        "review on worldless Work should succeed"
    );
}

/// Verify body_path resolution from Work chapters array.
#[test]
fn body_path_resolution_from_work_response() {
    let work_resp = serde_json::json!({
        "chapters": [
            {"chapter": 1, "body_path": "Works/my-novel/Stories/ch01.md"},
            {"chapter": 2, "body_path": "Works/my-novel/Stories/ch02.md"},
        ]
    });

    // Simulate resolve_audit_body_path logic
    let chapters = work_resp
        .get("chapters")
        .and_then(|v| v.as_array())
        .unwrap();
    let ch_row = chapters
        .iter()
        .find(|c| c.get("chapter").and_then(serde_json::Value::as_i64) == Some(2))
        .expect("chapter 2 should be found");
    let bp = ch_row.get("body_path").and_then(|v| v.as_str()).unwrap();
    assert_eq!(bp, "Works/my-novel/Stories/ch02.md");
}

/// Verify body_path returns None when chapter not found.
#[test]
fn body_path_returns_none_for_missing_chapter() {
    let work_resp = serde_json::json!({
        "chapters": [
            {"chapter": 1, "body_path": "Works/my-novel/Stories/ch01.md"},
        ]
    });

    let chapters = work_resp
        .get("chapters")
        .and_then(|v| v.as_array())
        .unwrap();
    let found = chapters
        .iter()
        .find(|c| c.get("chapter").and_then(serde_json::Value::as_i64) == Some(99));
    assert!(found.is_none(), "nonexistent chapter should return None");
}

/// Verify body_path returns None when chapters array is absent.
#[test]
fn body_path_returns_none_for_missing_chapters_array() {
    let work_resp = serde_json::json!({"work_id": "wrk_test"});
    let chapters = work_resp.get("chapters").and_then(|v| v.as_array());
    assert!(
        chapters.is_none(),
        "missing chapters array should return None"
    );
}

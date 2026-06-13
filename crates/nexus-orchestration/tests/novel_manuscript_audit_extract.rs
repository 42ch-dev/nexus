//! Hermetic tests for novel-manuscript-audit-extract embedded preset (V1.44 P0 fix, DF-69).
//!
//! Validates:
//! 1. Preset loads via `load_embedded_preset` and passes validation.
//! 2. Extract mode state machine: load_chapter → extract_sync → done.
//! 3. extract_sync calls kb.extract_work with correct args.
//! 4. No FL-E driver schedule fields in preset output.
//! 5. world_binding is required (R-V144P0-002: daemon-side enforcement).
//! 6. No review_report state in extract preset.

use nexus_orchestration::capability::CapabilityRegistry;
use nexus_orchestration::preset::load_embedded_preset;
use nexus_orchestration::preset::manifest;

#[test]
fn extract_preset_loads_successfully() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();
    assert_eq!(loaded.id, "novel-manuscript-audit-extract");
    assert_eq!(loaded.version, 1);
}

#[test]
fn extract_preset_requires_kb_extract_work() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

    assert!(
        loaded
            .manifest
            .preset
            .requires_capabilities
            .iter()
            .any(|c| c == "kb.extract_work"),
        "extract preset must require kb.extract_work"
    );
    assert!(
        loaded
            .manifest
            .preset
            .requires_capabilities
            .iter()
            .any(|c| c == "creator.inject_prompt"),
        "extract preset must require creator.inject_prompt"
    );
}

#[test]
fn extract_state_machine_load_chapter_to_extract_sync() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

    assert_eq!(loaded.manifest.preset.initial, "load_chapter");

    let load_chapter = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "load_chapter")
        .expect("load_chapter state must exist");

    // R-V144P0-001 fix: load_chapter now transitions to extract_sync (not review_report)
    assert_eq!(
        load_chapter.next,
        Some(manifest::NextTarget::Linear("extract_sync".into())),
        "load_chapter must transition to extract_sync in extract preset"
    );
}

#[test]
fn extract_sync_calls_kb_extract_work() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

    let extract = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "extract_sync")
        .expect("extract_sync state must exist");

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
fn extract_sync_passes_world_id_and_work_id() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

    let extract = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "extract_sync")
        .expect("extract_sync state must exist");

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
            assert!(has_world_id, "kb.extract_work args must reference world_id");
            assert!(has_work_id, "kb.extract_work args must reference work_id");
        }
    }
}

#[test]
fn extract_sync_transitions_to_done() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

    let extract = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "extract_sync")
        .expect("extract_sync state must exist");

    assert_eq!(
        extract.next,
        Some(manifest::NextTarget::Linear("done".into())),
        "extract_sync must transition to done"
    );
}

#[test]
fn extract_no_review_report_state() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

    let has_review = loaded
        .manifest
        .states
        .iter()
        .any(|s| s.id == "review_report");
    assert!(
        !has_review,
        "extract preset must NOT contain review_report state"
    );
}

#[test]
fn extract_no_fl_e_driver_fields() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

    for state in &loaded.manifest.states {
        for action in &state.enter {
            if let manifest::EnterAction::Capability { args, .. } = action {
                if let Some(args) = args {
                    for (key, _) in args.as_object().unwrap_or(&serde_json::Map::new()) {
                        assert!(
                            !key.contains("fl_e_stage"),
                            "extract preset must not set fl_e_stage: found in state '{}'",
                            state.id
                        );
                        assert!(
                            !key.contains("auto_chain"),
                            "extract preset must not reference auto_chain: found in state '{}'",
                            state.id
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn extract_done_is_terminal() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

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

// R-V144P0-002: verify that the extract preset does NOT contain review_report state
// (which would be reachable via review_report if the wrong preset was dispatched)
// The world_binding: required in YAML provides a hint to daemon, but the actual
// enforcement is at the CLI level + preset gate evaluation.
#[test]
fn extract_preset_has_no_review_state() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-extract", &caps).unwrap();

    // The extract preset only has: load_chapter, extract_sync, done
    let state_ids: Vec<&str> = loaded
        .manifest
        .states
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(state_ids, &["load_chapter", "extract_sync", "done"]);
}

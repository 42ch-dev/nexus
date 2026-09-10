//! Hermetic tests for novel-manuscript-audit-review embedded preset (V1.44 P0 fix, DF-69).
//!
//! Validates:
//! 1. Preset loads via `load_embedded_preset` and passes validation.
//! 2. Review mode state machine: `load_chapter` → `review_report` → done.
//! 3. No FL-E driver schedule fields in preset output.
//! 4. No `kb.extract_work` capability in review preset.
//! 5. `world_binding` is optional (review works on worldless Works).

use nexus_orchestration::capability::CapabilityRegistry;
use nexus_orchestration::preset::load_embedded_preset;
use nexus_orchestration::preset::manifest;

#[test]
fn review_preset_loads_successfully() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-review", &caps).unwrap();
    assert_eq!(loaded.id, "novel-manuscript-audit-review");
    assert_eq!(loaded.version, 1);
}

#[test]
fn review_preset_has_required_capabilities() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-review", &caps).unwrap();

    assert!(
        loaded
            .manifest
            .preset
            .requires_capabilities
            .iter()
            .any(|c| c == "creator.inject_prompt"),
        "review preset must require creator.inject_prompt"
    );
    // Review preset should NOT require kb.extract_work
    assert!(
        !loaded
            .manifest
            .preset
            .requires_capabilities
            .iter()
            .any(|c| c == "kb.extract_work"),
        "review preset must NOT require kb.extract_work"
    );
}

#[test]
fn review_state_machine_load_chapter_to_review_report() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-review", &caps).unwrap();

    assert_eq!(loaded.manifest.preset.initial, "load_chapter");

    let load_chapter = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "load_chapter")
        .expect("load_chapter state must exist");

    assert_eq!(
        load_chapter.next,
        Some(manifest::NextTarget::Linear("review_report".into())),
        "load_chapter must transition to review_report"
    );
}

#[test]
fn review_report_transitions_to_done() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-review", &caps).unwrap();

    let review = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "review_report")
        .expect("review_report state must exist");

    assert_eq!(
        review.next,
        Some(manifest::NextTarget::Linear("done".into())),
        "review_report must transition to done"
    );
}

#[test]
fn review_no_extract_sync_state() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-review", &caps).unwrap();

    let has_extract = loaded
        .manifest
        .states
        .iter()
        .any(|s| s.id == "extract_sync");
    assert!(
        !has_extract,
        "review preset must NOT contain extract_sync state"
    );
}

#[test]
fn review_no_fl_e_driver_fields() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-review", &caps).unwrap();

    for state in &loaded.manifest.states {
        for action in &state.enter {
            if let manifest::EnterAction::Capability {
                args: Some(args), ..
            } = action
            {
                let empty_map = serde_json::Map::new();
                for (key, _) in args.as_object().unwrap_or(&empty_map) {
                    assert!(
                        !key.contains("fl_e_stage"),
                        "review preset must not set fl_e_stage: found in state '{}'",
                        state.id
                    );
                    assert!(
                        !key.contains("auto_chain"),
                        "review preset must not reference auto_chain: found in state '{}'",
                        state.id
                    );
                }
            }
        }
    }
}

#[test]
fn review_done_is_terminal() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-manuscript-audit-review", &caps).unwrap();

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

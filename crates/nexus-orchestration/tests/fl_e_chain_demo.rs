//! FL-E stage chain integration test (T3, spec §3–4).
//!
//! Validates the full stage advance chain: intake → research → produce → review → persist.
//! Covers happy path, --force semantics, wrong order rejection, and active schedule uniqueness.

use nexus_contracts::local::orchestration::FL_E_STAGES;
use nexus_orchestration::stage_gates::{
    build_preset_input, check_stage_advance, preset_for_stage, WorkFields, WorkStageState,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn work_state(stage: &str, status: &str, intake: &str) -> WorkStageState {
    WorkStageState {
        current_stage: stage.to_string(),
        stage_status: status.to_string(),
        intake_status: intake.to_string(),
    }
}

fn demo_work_fields(stage: &str) -> WorkFields {
    WorkFields {
        work_id: "wrk_demo_e2e".to_string(),
        fl_e_stage: stage.to_string(),
        creative_brief: r#"{"genre":"sci-fi","tone":"literary","audience":"adult"}"#.to_string(),
        inspiration_log: r#"[{"note":"AI consciousness angle"}]"#.to_string(),
        work_ref: Some("my-novel".to_string()),
        chapter: Some(1),
        chapter_label: Some("01".to_string()),
        outline_path: Some("Works/my-novel/Outlines/chapters/ch01-outline.md".to_string()),
        body_path: Some("Works/my-novel/Stories/ch01-ch01.md".to_string()),
        slug: Some("ch01".to_string()),
        research_artifacts_dir: None,
        workspace_dir: None,
        world_kb_block: None,
        world_id: None,
        volume: None,
    }
}

// ── Case 1: Full happy-path chain ────────────────────────────────────────────

#[test]
fn fl_e_chain_happy_path_all_stages_advance() {
    // Start at intake with complete intake
    let mut current = work_state("intake", "complete", "complete");

    // Collect the full chain of stages
    let chain_stages: Vec<&str> = FL_E_STAGES.iter().copied().collect();

    for i in 1..chain_stages.len() {
        let target = chain_stages[i];

        // Gate check should pass
        assert!(
            check_stage_advance(&current, target, false).is_ok(),
            "Advance from '{}' to '{}' should succeed",
            current.current_stage,
            target,
        );

        // Preset resolution should work
        let _preset =
            preset_for_stage(target).unwrap_or_else(|| panic!("No preset for stage '{target}'"));

        // Preset input should contain Work fields
        let fields = demo_work_fields(target);
        let input = build_preset_input(&fields);
        assert!(
            input.get("work_id").is_some(),
            "preset input for '{target}' must have work_id"
        );

        // Simulate stage completion
        current = work_state(target, "complete", "complete");
    }

    // Final: all 5 stages should be valid
    assert_eq!(FL_E_STAGES.len(), 5);
}

// ── Case 2: Wrong order rejected without --force ────────────────────────────

#[test]
fn fl_e_chain_reject_intake_to_produce_skip() {
    // Cannot skip from intake directly to produce (must go through research)
    let work = work_state("intake", "complete", "complete");
    let err = check_stage_advance(&work, "produce", false).unwrap_err();
    assert!(
        err.message.contains("Cannot skip"),
        "Skip rejection should say 'Cannot skip': {}",
        err.message
    );
}

#[test]
fn fl_e_chain_reject_backwards_produce_to_research() {
    let work = work_state("produce", "complete", "complete");
    let err = check_stage_advance(&work, "research", false).unwrap_err();
    assert!(
        err.message.contains("backwards"),
        "Backwards rejection should say 'backwards': {}",
        err.message
    );
}

#[test]
fn fl_e_chain_reject_same_stage() {
    let work = work_state("research", "complete", "complete");
    let err = check_stage_advance(&work, "research", false).unwrap_err();
    assert!(
        err.message.contains("already at stage"),
        "Same stage rejection should say 'already at stage': {}",
        err.message
    );
}

#[test]
fn fl_e_chain_reject_unknown_stage() {
    let work = work_state("intake", "complete", "complete");
    let err = check_stage_advance(&work, "unknown_stage", false).unwrap_err();
    assert!(
        err.message.contains("Unknown stage"),
        "Unknown stage rejection should say 'Unknown stage': {}",
        err.message
    );
}

// ── Case 3: --force bypasses all gates ──────────────────────────────────────

#[test]
fn fl_e_chain_force_allows_skip() {
    // Force allows skipping from intake directly to persist
    let work = work_state("intake", "pending", "pending");
    assert!(
        check_stage_advance(&work, "persist", true).is_ok(),
        "--force should allow skipping all stages"
    );
}

#[test]
fn fl_e_chain_force_allows_backwards() {
    let work = work_state("produce", "complete", "complete");
    assert!(
        check_stage_advance(&work, "intake", true).is_ok(),
        "--force should allow backwards"
    );
}

#[test]
fn fl_e_chain_force_allows_incomplete_current() {
    let work = work_state("research", "pending", "complete");
    assert!(
        check_stage_advance(&work, "produce", true).is_ok(),
        "--force should allow advancing from incomplete stage"
    );
}

// ── Case 4: Active schedule uniqueness (spec §2 invariant #4) ────────────────

#[test]
fn fl_e_chain_reject_advance_while_active() {
    let work = work_state("research", "active", "complete");
    let err = check_stage_advance(&work, "produce", false).unwrap_err();
    assert!(
        err.message.contains("active stage schedule"),
        "Active schedule rejection should mention 'active stage schedule': {}",
        err.message
    );
}

// ── Case 5: Preset input chain correctness ──────────────────────────────────

#[test]
fn fl_e_chain_preset_input_fields_propagate_across_stages() {
    let fields = demo_work_fields("research");
    let input = build_preset_input(&fields);

    // All stages get work_id + fl_e_stage
    assert_eq!(input["work_id"], "wrk_demo_e2e");
    assert_eq!(input["fl_e_stage"], "research");

    // creative_brief and inspiration_log are always available
    assert!(
        input["creative_brief"].as_str().unwrap().contains("sci-fi"),
        "creative_brief should contain the genre"
    );
    let log: serde_json::Value =
        serde_json::from_str(input["inspiration_log"].as_str().unwrap()).unwrap();
    assert_eq!(log.as_array().map(|a| a.len()), Some(1));
}

// ── Case 6: End-to-end stage → preset → input resolution ────────────────────

#[test]
fn fl_e_chain_e2e_resolve_presets_and_inputs() {
    let stages_and_presets: Vec<(&str, &str)> = vec![
        ("intake", "creative-brief-intake"),
        ("research", "research"),
        ("produce", "novel-writing"),
        ("review", "reflection-loop"),
        ("persist", "kb-extract"),
    ];

    for (stage, expected_preset) in &stages_and_presets {
        let resolved =
            preset_for_stage(stage).unwrap_or_else(|| panic!("No preset for stage '{stage}'"));
        assert_eq!(
            resolved, *expected_preset,
            "Stage '{stage}' should map to preset '{expected_preset}'"
        );

        // Build preset input and verify all required fields are present
        let fields = demo_work_fields(stage);
        let input = build_preset_input(&fields);
        assert!(
            input.get("work_id").is_some(),
            "work_id missing for {stage}"
        );
        assert!(
            input.get("fl_e_stage").is_some(),
            "fl_e_stage missing for {stage}"
        );
        assert!(
            input.get("creative_brief").is_some(),
            "creative_brief missing for {stage}"
        );
        assert!(
            input.get("inspiration_log").is_some(),
            "inspiration_log missing for {stage}"
        );
    }
}

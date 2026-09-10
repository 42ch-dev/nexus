//! E2E smoke tests for novel-brainstorm and novel-review-master embedded presets (V1.39 P2).
//!
//! Validates that both presets load successfully and simulate a happy-path
//! state advance to confirm the state machine wiring is correct.

use nexus_orchestration::capability::CapabilityRegistry;
use nexus_orchestration::preset::load_embedded_preset;
use nexus_orchestration::preset::manifest;

/// AC1: Both presets load via `load_embedded_preset` and pass validator.
#[test]
fn novel_brainstorm_loads_successfully() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-brainstorm", &caps).unwrap();
    assert_eq!(loaded.id, "novel-brainstorm");
}

#[test]
fn novel_review_master_loads_successfully() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-review-master", &caps).unwrap();
    assert_eq!(loaded.id, "novel-review-master");
}

/// AC2: novel-brainstorm consumes open findings in prompt context.
/// Verify that the gather state's enter action references `open_findings` in its template vars.
#[test]
fn novel_brainstorm_gather_references_open_findings() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-brainstorm", &caps).unwrap();

    let gather = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "gather")
        .expect("gather state must exist");

    // The enter action must inject a prompt that references open_findings
    let has_open_findings_var = gather.enter.iter().any(|action| {
        if let manifest::EnterAction::Capability {
            args: Some(args), ..
        } = action
        {
            let vars = args.get("vars");
            if let Some(vars) = vars {
                let open_findings = vars.get("open_findings");
                return open_findings.is_some();
            }
        }
        false
    });
    assert!(
        has_open_findings_var,
        "gather state enter action must reference open_findings in vars"
    );
}

/// AC3: novel-review-master lists findings requiring approval; integrates with P1 API.
/// Verify the present state surfaces `open_findings` and the state machine
/// has a structured `await_decision` state for recording decisions.
#[test]
fn novel_review_master_present_references_open_findings() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-review-master", &caps).unwrap();

    let present = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "present")
        .expect("present state must exist");

    // The enter action must inject a prompt that references open_findings
    let has_open_findings_var = present.enter.iter().any(|action| {
        if let manifest::EnterAction::Capability {
            args: Some(args), ..
        } = action
        {
            let vars = args.get("vars");
            if let Some(vars) = vars {
                let open_findings = vars.get("open_findings");
                return open_findings.is_some();
            }
        }
        false
    });
    assert!(
        has_open_findings_var,
        "present state enter action must reference open_findings in vars"
    );

    // Verify await_decision state exists for recording human decisions
    let await_decision = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "await_decision")
        .expect("await_decision state must exist");

    assert!(
        await_decision.next.is_some(),
        "await_decision must transition to done"
    );
}

/// Happy path: brainstorm state machine flows gather → synthesize → done.
#[test]
fn novel_brainstorm_happy_path_state_flow() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-brainstorm", &caps).unwrap();

    // Verify the graph is wired: gather → synthesize → done
    let gather = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "gather")
        .expect("gather");
    assert_eq!(
        gather.next,
        Some(manifest::NextTarget::Linear("synthesize".into()))
    );

    let synthesize = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "synthesize")
        .expect("synthesize");
    assert_eq!(
        synthesize.next,
        Some(manifest::NextTarget::Linear("done".into()))
    );

    let done = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "done")
        .expect("done");
    assert!(done.terminal);
    assert!(done.next.is_none());
}

/// Happy path: review-master state machine flows present → `await_decision` → `sync_world_kb` → done.
#[test]
fn novel_review_master_happy_path_state_flow() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-review-master", &caps).unwrap();

    // Verify the graph is wired: present → await_decision → sync_world_kb → done
    let present = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "present")
        .expect("present");
    assert_eq!(
        present.next,
        Some(manifest::NextTarget::Linear("await_decision".into()))
    );

    let await_decision = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "await_decision")
        .expect("await_decision");
    assert_eq!(
        await_decision.next,
        Some(manifest::NextTarget::Linear("sync_world_kb".into()))
    );

    let sync_world_kb = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "sync_world_kb")
        .expect("sync_world_kb");
    assert_eq!(
        sync_world_kb.next,
        Some(manifest::NextTarget::Linear("done".into()))
    );

    let done = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "done")
        .expect("done");
    assert!(done.terminal);
    assert!(done.next.is_none());
}

/// Verify brainstorm uses auto-chain compatible exit (`llm_judge`).
#[test]
fn novel_brainstorm_auto_chain_compatible() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-brainstorm", &caps).unwrap();

    // gather and synthesize both use llm_judge for auto-chain
    for state_id in &["gather", "synthesize"] {
        let state = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == *state_id)
            .expect("state must exist");
        assert!(
            matches!(state.exit_when, Some(manifest::ExitWhen::LlmJudge { .. })),
            "{state_id} must use llm_judge exit for auto-chain"
        );
    }
}

/// Verify review-master uses manual exit (human-in-loop).
#[test]
fn novel_review_master_human_in_loop() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("novel-review-master", &caps).unwrap();

    // present and await_decision both use manual for human-in-loop
    for state_id in &["present", "await_decision"] {
        let state = loaded
            .manifest
            .states
            .iter()
            .find(|s| s.id == *state_id)
            .expect("state must exist");
        assert!(
            matches!(state.exit_when, Some(manifest::ExitWhen::Manual)),
            "{state_id} must use manual exit for human-in-loop"
        );
    }
}

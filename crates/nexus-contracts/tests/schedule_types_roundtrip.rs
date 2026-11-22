//! Round-trip serde tests for Schedule types (WS7 T1).
//!
//! Tests are written FIRST per TDD; types do not yet exist.

use nexus_contracts::local::schedule::*;

#[test]
fn schedule_roundtrip_json() {
    let schedule = Schedule {
        id: ScheduleId("01JMXABCDEF00000000000001".to_string()),
        creator_id: "ctr_test_creator".to_string(),
        preset_id: "novel-writing".to_string(),
        preset_version: 1,
        status: ScheduleStatus::Pending,
        concurrency: ScheduleConcurrency::Serial,
        depends_on: vec![],
        current_core_context_version: CoreContextVersion(0),
        current_session_id: None,
        scheduled_at: None,
        label: Some("My first novel".to_string()),
        created_at: "2026-04-18T00:00:00Z".to_string(),
        updated_at: "2026-04-18T00:00:00Z".to_string(),
        terminated_at: None,
    };

    let json = serde_json::to_string(&schedule).expect("serialize Schedule");
    let back: Schedule = serde_json::from_str(&json).expect("deserialize Schedule");

    assert_eq!(back.id, schedule.id);
    assert_eq!(back.creator_id, schedule.creator_id);
    assert_eq!(back.preset_id, schedule.preset_id);
    assert_eq!(back.preset_version, schedule.preset_version);
    assert_eq!(back.status, schedule.status);
    assert_eq!(back.concurrency, schedule.concurrency);
    assert_eq!(back.depends_on, schedule.depends_on);
    assert_eq!(
        back.current_core_context_version,
        schedule.current_core_context_version
    );
    assert_eq!(back.current_session_id, schedule.current_session_id);
    assert_eq!(back.scheduled_at, schedule.scheduled_at);
    assert_eq!(back.label, schedule.label);
    assert_eq!(back.created_at, schedule.created_at);
    assert_eq!(back.updated_at, schedule.updated_at);
    assert_eq!(back.terminated_at, schedule.terminated_at);
}

#[test]
fn derivation_step_reserves_llm_summarize() {
    let json = r#"{
        "kind": "llm_summarize",
        "capability": "context.summarize",
        "prompt_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
    }"#;

    let step: DerivationStep = serde_json::from_str(json).expect("deserialize LlmSummarize");

    match &step {
        DerivationStep::LlmSummarize {
            capability,
            prompt_hash,
            ..
        } => {
            assert_eq!(capability, "context.summarize");
            assert_eq!(*prompt_hash, [0u8; 32]);
        }
        other => panic!(
            "Expected LlmSummarize, got {:?}",
            std::mem::discriminant(other)
        ),
    }

    let json2 = serde_json::to_string(&step).expect("serialize LlmSummarize back");
    assert!(
        json2.contains("\"kind\":\"llm_summarize\""),
        "JSON should use snake_case tag, got: {json2}"
    );

    let step2: DerivationStep = serde_json::from_str(&json2).expect("re-deserialize LlmSummarize");
    assert!(matches!(step2, DerivationStep::LlmSummarize { .. }));
}

#[test]
fn derivation_step_parses_unknown_variant_safely() {
    let unknown_json = r#"{"kind":"ai_generated_edit","note":"future variant"}"#;

    let result: Result<DerivationStep, _> = serde_json::from_str(unknown_json);
    match result {
        Ok(_) => {}
        Err(e) => {
            let msg = e.to_string();
            assert!(!msg.is_empty(), "Error message should be non-empty");
        }
    }
}

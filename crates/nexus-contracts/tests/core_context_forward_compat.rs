//! Forward-compat guard test for `core_context` derivation kinds (`WS7` T10).
//!
//! Per spec §11 / compass `RISK-10`: V1.4 defines `DerivationStep::LlmSummarize`
//! even though it doesn't emit it. This test codifies that:
//! 1. A V1.5+ `llm_summarize` JSON blob deserializes successfully.
//! 2. If a future version removes/renames this variant, CI will break —
//!    which is intentional (guard against silent drift).

use nexus_contracts::local::schedule::DerivationStep;

#[test]
fn core_context_versions_parse_unknown_derivation_kind_safely() {
    // Simulate a V1.5+ row with kind=`llm_summarize` payload.
    // Current V1.4 code must deserialize without panic and either preserve
    // the variant (LlmSummarize is already defined, even if not emitted) or
    // expose the unknown kind as a recoverable error.
    let json = r#"{
        "kind": "llm_summarize",
        "capability": "context.summarize",
        "prompt_hash": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
    }"#;
    let v: DerivationStep = serde_json::from_str(json).expect("must deserialize");
    assert!(
        matches!(v, DerivationStep::LlmSummarize { .. }),
        "LlmSummarize variant must survive roundtrip for V1.5 forward-compat"
    );
}

#[test]
fn core_context_versions_llm_summarize_roundtrip() {
    // Verify the variant roundtrips through JSON serialization.
    let json = r#"{
        "kind": "llm_summarize",
        "capability": "context.summarize",
        "prompt_hash": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32]
    }"#;
    let v: DerivationStep = serde_json::from_str(json).expect("must deserialize");
    let serialized = serde_json::to_string(&v).expect("must serialize");
    let back: DerivationStep = serde_json::from_str(&serialized).expect("must re-deserialize");

    // Both should be LlmSummarize.
    match (&v, &back) {
        (
            DerivationStep::LlmSummarize { capability: c1, .. },
            DerivationStep::LlmSummarize { capability: c2, .. },
        ) => {
            assert_eq!(c1, c2, "capability should survive roundtrip");
        }
        _ => panic!("expected LlmSummarize for both variants"),
    }
}

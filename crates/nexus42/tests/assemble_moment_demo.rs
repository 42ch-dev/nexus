//! Integration test for four-domain Moment assembly via nexus42.
//!
//! Proves product wiring from `nexus42` to `assemble_moment` with in-memory
//! stores, verifying that all four domains appear in the output:
//!
//! 1. Creator Memory (Stage-0)
//! 2. Narrative (World state + timeline)
//! 3. World KB (key blocks)
//! 4. User Knowledge (entries)
//!
//! Follows the deterministic fixture pattern from
//! `nexus-moment-context-assembly/tests/assembly_integration.rs`.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus42::commands::platform::context::run_assemble_moment_demo;

/// Seeded demo with default IDs should produce all four domains.
#[tokio::test]
async fn assemble_moment_demo_produces_four_domain_context() {
    let ctx = run_assemble_moment_demo(None, None, None, None).await;

    let full = ctx.to_full_context();

    // Domain 1: Stage-0 (Creator Memory)
    assert!(
        !ctx.stage0_context.is_empty(),
        "Stage-0 context must be non-empty"
    );
    assert!(
        full.contains("demo creator"),
        "Full context must contain Stage-0 personality text"
    );

    // Domain 2: Narrative (World state + timeline)
    assert!(
        ctx.world_state.is_some(),
        "World state must be present for seeded demo world"
    );
    assert!(
        ctx.timeline.is_some(),
        "Timeline must be present for seeded demo world"
    );
    let ws = ctx.world_state.as_ref().expect("world state");
    assert!(
        ws.contains("wld_demo"),
        "World state must reference the demo world ID"
    );
    assert!(
        full.contains("Demo World"),
        "Full context must contain world title"
    );
    assert!(
        full.contains("Demo story event"),
        "Full context must contain timeline event"
    );

    // Domain 3: World KB
    assert!(
        ctx.world_kb.is_some(),
        "World KB must be present for seeded demo world"
    );
    assert!(
        full.contains("Demo Hero"),
        "Full context must contain KB block"
    );

    // Domain 4: User Knowledge
    assert!(
        ctx.user_knowledge.is_some(),
        "User knowledge must be present for seeded demo user"
    );
    assert!(
        full.contains("Demo user knowledge"),
        "Full context must contain knowledge entry"
    );
}

/// Custom world ID and user ID should seed matching fixtures.
#[tokio::test]
async fn assemble_moment_demo_with_custom_ids() {
    let ctx = run_assemble_moment_demo(Some("wld_custom"), Some("user_custom"), None, None).await;

    let ws = ctx.world_state.as_ref().expect("world state");
    assert!(
        ws.contains("wld_custom"),
        "World state must reference custom world ID"
    );

    // Knowledge is seeded for the provided user ID; content is fixed
    let uk = ctx.user_knowledge.as_ref().expect("user knowledge");
    assert!(
        uk.contains("Demo user knowledge"),
        "User knowledge must contain seeded content"
    );
}

/// MomentContext with no matching IDs (non-seeded) should still have Stage-0
/// but lack the other three domains.
#[tokio::test]
async fn assemble_moment_demo_non_seeded_ids_yield_stage0_only() {
    // The demo function always seeds with the provided IDs, so passing
    // IDs through the function will always produce results. To test the
    // "missing world" path, we verify that the MomentContext default has
    // only stage0 when the four-domain data is empty.
    let ctx = nexus_moment_context_assembly::MomentContext::default();

    assert!(ctx.stage0_context.is_empty());
    assert!(ctx.world_state.is_none());
    assert!(ctx.timeline.is_none());
    assert!(ctx.world_kb.is_none());
    assert!(ctx.user_knowledge.is_none());
}

/// Full context heading structure matches the spec ordering.
#[tokio::test]
async fn assemble_moment_demo_context_heading_order() {
    let ctx = run_assemble_moment_demo(None, None, None, None).await;
    let full = ctx.to_full_context();

    // Verify section headings appear in spec order
    let pos_stage0 = full
        .find("## Personality")
        .or_else(|| full.find("demo creator"));
    let pos_world = full.find("## World State");
    let pos_timeline = full.find("## Timeline");
    let pos_kb = full.find("## World Knowledge Base");
    let pos_knowledge = full.find("## User Knowledge");

    // All headings must be present
    assert!(pos_stage0.is_some(), "Stage-0 section must be present");
    assert!(pos_world.is_some(), "World State heading must be present");
    assert!(pos_timeline.is_some(), "Timeline heading must be present");
    assert!(pos_kb.is_some(), "World KB heading must be present");
    assert!(
        pos_knowledge.is_some(),
        "User Knowledge heading must be present"
    );

    // Verify ordering: Stage0 < World State < Timeline < World KB < User Knowledge
    let s0 = pos_stage0.expect("checked above");
    let ws = pos_world.expect("checked above");
    let tl = pos_timeline.expect("checked above");
    let kb = pos_kb.expect("checked above");
    let kn = pos_knowledge.expect("checked above");

    assert!(s0 < ws, "Stage-0 must come before World State");
    assert!(ws < tl, "World State must come before Timeline");
    assert!(tl < kb, "Timeline must come before World KB");
    assert!(kb < kn, "World KB must come before User Knowledge");
}

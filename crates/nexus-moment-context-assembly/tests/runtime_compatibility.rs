//! T6.11: Two-Stage Assembly Mock Tests
//!
//! Integration tests for TwoStageAssembly with mocked platform responses.
//!
//! These tests require the `cloud-stage` feature because they import
//! `cloud_stage` module types that are gated behind `#[cfg(feature = "cloud-stage")]`.
//! Run with: `cargo test -p nexus-moment-context-assembly --features cloud-stage --test runtime_compatibility`

#![cfg(feature = "cloud-stage")]
use nexus_contracts::local::domain::RuntimeMode;
use nexus_creator_memory::LongTermMemory;
use nexus_moment_context_assembly::cloud_stage::{
    AssembleMetadata, AssembleResponse, AssemblyRuntimeMode, KbEntry, MemoryItemRef,
    TwoStageAssembly,
};

/// Helper to create a LongTermMemory for testing.
fn make_memory(kind: &str, body: &str, updated_at: &str) -> LongTermMemory {
    let mut mem = LongTermMemory::new(kind);
    mem.set_body(body);
    mem.frontmatter.updated_at = updated_at.to_string();
    mem
}

#[test]
fn two_stage_assembly_with_mock_platform_response() {
    let mock_response = AssembleResponse {
        memory_items: vec![MemoryItemRef {
            memory_id: "mem-platform-1".into(),
            content_summary: "Platform memory".into(),
            relevance_score: Some(0.9),
        }],
        kb: vec![KbEntry {
            entry_id: "kb-1".into(),
            title: "KB entry".into(),
            content: "KB content".into(),
        }],
        timeline: vec![],
        metadata: AssembleMetadata {
            assembled_at: chrono::Utc::now().to_rfc3339(),
            token_count_estimate: Some(100),
        },
    };

    let assembly = TwoStageAssembly {
        stage1_response: Some(mock_response),
        personality: "Creative writer".into(),
        experience: "10 years experience".into(),
        long_term_memories: vec![],
        fragment_keywords: vec!["plot".into()],
        user_prompt: "Write chapter 1".into(),
        system_prefix: "".into(),
        max_tokens: None,
        runtime_mode: AssemblyRuntimeMode::new(RuntimeMode::CloudEnhanced),
    };

    let output = assembly.assemble();
    assert!(output.contains("Creative writer"));
    assert!(output.contains("Platform memory"));
    assert!(output.contains("KB content"));
    assert!(output.contains("Write chapter 1"));
}

#[test]
fn two_stage_fallback_empty_response() {
    let empty_response = AssembleResponse {
        memory_items: vec![],
        kb: vec![],
        timeline: vec![],
        metadata: AssembleMetadata {
            assembled_at: chrono::Utc::now().to_rfc3339(),
            token_count_estimate: Some(0),
        },
    };

    let assembly = TwoStageAssembly {
        stage1_response: Some(empty_response),
        personality: "Test personality".into(),
        experience: "Test experience".into(),
        long_term_memories: vec![make_memory("story_summary", "Local memory", "2026-04-15")],
        fragment_keywords: vec!["keyword".into()],
        user_prompt: "Task".into(),
        system_prefix: "".into(),
        max_tokens: None,
        runtime_mode: AssemblyRuntimeMode::new(RuntimeMode::LocalFirst),
    };

    let output = assembly.assemble();
    assert!(output.contains("Local memory"));
    assert!(output.contains("Test personality"));
}

#[test]
fn dedup_platform_memory_with_local_priority() {
    let mut local_mem = make_memory("note", "Local content", "2026-04-15");
    local_mem.frontmatter.memory_id = "mem-1".into();

    let platform_item = MemoryItemRef {
        memory_id: "mem-1".into(),
        content_summary: "Platform content".into(),
        relevance_score: Some(0.8),
    };

    let assembly = TwoStageAssembly {
        stage1_response: Some(AssembleResponse {
            memory_items: vec![platform_item],
            kb: vec![],
            timeline: vec![],
            metadata: AssembleMetadata {
                assembled_at: chrono::Utc::now().to_rfc3339(),
                token_count_estimate: None,
            },
        }),
        personality: "".into(),
        experience: "".into(),
        long_term_memories: vec![local_mem],
        fragment_keywords: vec![],
        user_prompt: "".into(),
        system_prefix: "".into(),
        max_tokens: None,
        runtime_mode: AssemblyRuntimeMode::new(RuntimeMode::CloudEnhanced),
    };

    let output = assembly.assemble();
    assert!(output.contains("Local content"));
    assert!(!output.contains("Platform content"));
}

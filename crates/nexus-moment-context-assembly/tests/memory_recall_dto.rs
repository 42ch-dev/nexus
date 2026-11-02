//! NXM-7 acceptance: the E10 memory inputs are the upstream contract DTO.
//!
//! Run with `cargo test -p nexus-moment-context-assembly --test memory_recall_dto`.
//!
//! Covered here (no feature flag):
//! * the Stage-0 `long_term_memories` vector is `Vec<RetainedRecord>` and maps
//!   `kind` → sort key, `updated_at` → recency key, `content.text` → body and
//!   `nexus_legacy.memory_id` (else `record_id`) → the `### Memory:` title;
//! * the recall partition separates the nine authored `nexus.<MemoryKind>`
//!   records, the `fact` fragment family and the derived/provenance records.
//!
//! The cloud-stage (`TwoStageAssembly`) DTO mapping requires
//! `--features cloud-stage` and is covered by `tests/runtime_compatibility.rs`.

use nexus_moment_context_assembly::recall::{
    memory_body, memory_display_id, memory_kind, memory_updated_at, partition,
};
use nexus_moment_context_assembly::stage0::Stage0Assembly;

mod common;

const fn stage0(
    long_term_memories: Vec<nexus_spoke_adapter::spoke_schemas::RetainedRecord>,
) -> Stage0Assembly {
    Stage0Assembly {
        personality: String::new(),
        experience: String::new(),
        long_term_memories,
        fragment_keywords: Vec::new(),
        system_prefix: String::new(),
        user_prompt: String::new(),
        max_tokens: None,
    }
}

#[test]
fn stage0_renders_the_legacy_memory_id_as_the_title() {
    let record = common::memory(
        "mem_alpha",
        "story_summary",
        "The hero answers the call.",
        "2026-04-14T00:00:00Z",
    );
    assert_eq!(memory_display_id(&record), "mem_alpha");
    assert_eq!(memory_body(&record), "The hero answers the call.");
    assert_eq!(memory_kind(&record), "nexus.story_summary");

    let output = stage0(vec![record]).assemble();
    assert!(
        output.contains("### Memory: mem_alpha\n\nThe hero answers the call."),
        "the DTO's display id and body are the rendered memory section: {output}"
    );
}

#[test]
fn stage0_falls_back_to_the_record_id_without_legacy_metadata() {
    let record = common::native(
        "rec_native_1",
        "nexus.world_building",
        "A city on the tide line.",
        "2026-04-14T00:00:00Z",
    );
    assert_eq!(memory_display_id(&record), "rec_native_1");

    let output = stage0(vec![record]).assemble();
    assert!(
        output.contains("### Memory: rec_native_1"),
        "a provider-native record titles itself by record id: {output}"
    );
}

#[test]
fn stage0_orders_by_namespaced_kind_then_updated_at_desc() {
    let older_character = common::memory(
        "mem_char_old",
        "character_note",
        "Alice is kind.",
        "2026-04-12T00:00:00Z",
    );
    let newer_character = common::memory(
        "mem_char_new",
        "character_note",
        "Bob is brave.",
        "2026-04-14T00:00:00Z",
    );
    let summary = common::memory(
        "mem_summary",
        "story_summary",
        "A summary.",
        "2026-04-13T00:00:00Z",
    );
    let world = common::memory(
        "mem_world",
        "world_building",
        "A world.",
        "2026-04-11T00:00:00Z",
    );
    // The DTO's `updated_at` is the recency key (an instant, not a string).
    assert!(memory_updated_at(&newer_character) > memory_updated_at(&older_character));

    let output = stage0(vec![summary, world, newer_character, older_character]).assemble();
    let new_character = output.find("Bob is brave.").expect("newer character");
    let old_character = output.find("Alice is kind.").expect("older character");
    let summary_at = output.find("A summary.").expect("summary");
    let world_at = output.find("A world.").expect("world");
    assert!(new_character < old_character, "newer first within a kind");
    assert!(
        old_character < summary_at,
        "character_note before story_summary"
    );
    assert!(summary_at < world_at, "story_summary before world_building");
}

#[test]
fn partition_separates_authored_memory_fragments_and_derived_records() {
    let records = vec![
        common::memory(
            "mem_1",
            "story_summary",
            "An authored summary.",
            "2026-04-10T00:00:00Z",
        ),
        common::fragment(
            "frag_1",
            "The gate opens at dusk.",
            &["gate", "dusk"],
            "2026-04-11T00:00:00Z",
        ),
        common::native(
            "rec_cache",
            "nexus.soul_narrative",
            "A derived cache.",
            "2026-04-12T00:00:00Z",
        ),
        common::native(
            "rec_event",
            "event",
            "A run capture.",
            "2026-04-13T00:00:00Z",
        ),
        common::native(
            "rec_note",
            "note",
            "A plain provider note.",
            "2026-04-14T00:00:00Z",
        ),
    ];

    let partition = partition(records);

    assert_eq!(partition.long_term_memories.len(), 1);
    assert_eq!(
        memory_kind(&partition.long_term_memories[0]),
        "nexus.story_summary"
    );
    assert_eq!(partition.fragments.len(), 1);
    assert_eq!(partition.fragments[0].fragment_id, "frag_1");
    assert_eq!(partition.fragments[0].summary, "The gate opens at dusk.");
    assert_eq!(
        partition.fragments[0].keywords,
        Some(vec!["gate".to_string(), "dusk".to_string()])
    );
    assert_eq!(
        partition.fragment_keywords,
        vec!["dusk".to_string(), "gate".to_string()],
        "the keyword union is de-duplicated and sorted"
    );
}

#[test]
fn partition_orders_fragments_newest_first_then_by_fragment_id_desc() {
    let records = vec![
        common::fragment("frag_b", "Older.", &[], "2026-04-10T00:00:00Z"),
        common::fragment("frag_c", "Newest.", &[], "2026-04-12T00:00:00Z"),
        common::fragment(
            "frag_a",
            "Same instant, smaller id.",
            &[],
            "2026-04-10T00:00:00Z",
        ),
    ];
    let partition = partition(records);
    let summaries: Vec<&str> = partition
        .fragments
        .iter()
        .map(|fragment| fragment.summary.as_str())
        .collect();
    assert_eq!(
        summaries,
        ["Newest.", "Older.", "Same instant, smaller id."]
    );
}

#[test]
fn partition_feeds_the_stage0_inputs_end_to_end() {
    let partition = partition(vec![
        common::memory(
            "mem_1",
            "plot_outline",
            "Act one: the call.",
            "2026-04-10T00:00:00Z",
        ),
        common::fragment(
            "frag_1",
            "Rain on the harbour.",
            &["Rain", "harbour"],
            "2026-04-11T00:00:00Z",
        ),
        common::fragment(
            "frag_2",
            "The lamp gutters.",
            &["lamp"],
            "2026-04-12T00:00:00Z",
        ),
    ]);

    let assembly = Stage0Assembly {
        personality: String::new(),
        experience: String::new(),
        long_term_memories: partition.long_term_memories,
        fragment_keywords: partition.fragment_keywords,
        system_prefix: String::new(),
        user_prompt: String::new(),
        max_tokens: None,
    };
    let output = assembly.assemble();
    assert!(output.contains("### Memory: mem_1\n\nAct one: the call."));
    assert!(
        output.contains("### Fragment keywords (deduped)\n\nharbour, lamp, rain"),
        "the keyword union renders in the retained deduped form: {output}"
    );
}

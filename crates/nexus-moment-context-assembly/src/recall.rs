//! Contract-DTO recall consumption (NXM-7).
//!
//! E10 (this crate's Stage-0 / cloud-stage assembly) and E6 (`nexus-core`'s
//! Character mind projection) both consume the provider's `memory.recall`
//! result through the upstream [`RetainedRecord`] DTO — never through the
//! legacy `LongTermMemory` file type this crate used to depend on.
//!
//! [`partition`] is the one place that decides what a recall record *is* for
//! those two consumers, so the two entry points cannot drift apart:
//!
//! * the nine authored kinds — `nexus.<MemoryKind>`, the provider's D-7
//!   namespaced mapping — are the long-term-memory vector;
//! * `fact` records are the fragment family (summary + keywords);
//! * everything else is not memory for MCA: the derived `nexus.soul_narrative`
//!   cache and the `event` provenance rows fall outside the nine-kind taxonomy
//!   (and any future product kind does too until it joins that taxonomy).
//!
//! Legacy-only metadata (`nexus_legacy`) is read here, never exposed as an MCA
//! input: the display id, the slug, the per-fragment keywords and the memory
//! file's frontmatter are the only members the projections need, and the rest
//! stays provider/CLI provenance.

use nexus_contracts::generated::domain::memory::MemoryMemoryKind;
use nexus_spoke_adapter::spoke_schemas::data::retained_record::PlacementScope;
use nexus_spoke_adapter::spoke_schemas::RetainedRecord;
use serde_json::Value;
use std::collections::BTreeSet;

/// Record kind of a fragment: the provider's core `fact` vocabulary.
pub const FACT_KIND: &str = "fact";

/// The legacy memory-file format version (`nexus_creator_memory::MEMORY_FILE_VERSION`),
/// repeated because the DTO is what a provider-native record offers.
const DEFAULT_MEMORY_VERSION: u32 = 1;

/// Prefix of a namespaced Nexus `MemoryKind`. The provider-side source of this
/// string is `nexus_core::memory_provider::policy::NEXUS_KIND_PREFIX`; it is
/// repeated here because this crate is a dependency of `nexus-core`, never the
/// reverse.
const NEXUS_KIND_PREFIX: &str = "nexus.";

/// One recall `fact` record as E6 renders a fragment line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentMemory {
    /// Legacy `fragment_id` when the record carries one, else its record id.
    pub fragment_id: String,
    /// The fragment summary (`content.text`).
    pub summary: String,
    /// `content.structured.nexus_legacy.keywords`; `None` when the record
    /// carries no keyword member at all (the distinction the projection's
    /// rendered line preserves).
    pub keywords: Option<Vec<String>>,
    /// The record's legacy selection key: the original `created_at` TEXT a
    /// migrated row carried, else the record's own rendering. The legacy
    /// per-scope read ordered by this text (not by the parsed instant), so the
    /// projection keeps the same key (fix round 4, finding R4-F1).
    pub created_at: String,
    /// The admitted scope the fragment sits in. E6 bounds each scope's
    /// fragments separately, exactly as the legacy per-scope queries did.
    pub scope: FragmentScope,
}

/// A recall record's placement as E6's per-scope fragment bounds need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentScope {
    /// The bearer-shared scope.
    Shared,
    /// One binding-local scope (`context` placement).
    Context(String),
}

/// A legacy memory file's frontmatter plus its body.
///
/// A migrated record carries these fields in `nexus_legacy`; a provider-native
/// record never had a file, so its frontmatter is derived from the retained
/// record itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryDocument {
    /// `nexus_memory_version` — the legacy file format version.
    pub nexus_memory_version: u32,
    /// `memory_id` — the logical id (`mem_<hex>`).
    pub memory_id: String,
    /// `memory_kind` — the bare authored kind (`story_summary`, …).
    pub memory_kind: String,
    /// `updated_at` — the file's own timestamp text.
    pub updated_at: String,
    /// `source_session_ids` — the ACP sessions that produced the memory.
    pub source_session_ids: Vec<String>,
    /// The markdown body (`content.text`).
    pub body: String,
}

/// One recall result set split into the inputs E6 and E10 consume.
#[derive(Debug, Clone, Default)]
pub struct RecallPartition {
    /// The authored `nexus.<MemoryKind>` records, in recall order.
    pub long_term_memories: Vec<RetainedRecord>,
    /// `fact` records in the legacy order (`created_at` TEXT DESC,
    /// `fragment_id` DESC) — E6's deterministic merge order.
    pub fragments: Vec<FragmentMemory>,
    /// The union of every fragment's keywords (de-duplicated, sorted).
    pub fragment_keywords: Vec<String>,
}

/// Split a recall result set into the E10 long-term-memory vector, the E6
/// fragment signals and the MCA keyword union.
#[must_use]
pub fn partition(records: Vec<RetainedRecord>) -> RecallPartition {
    let mut partition = RecallPartition::default();
    let mut fragments: Vec<FragmentMemory> = Vec::new();
    let mut keywords = BTreeSet::new();
    for record in records {
        let kind = memory_kind(&record);
        if kind == FACT_KIND {
            let legacy = legacy_metadata(&record);
            let fragment_id = legacy
                .and_then(|legacy| legacy.get("fragment_id"))
                .and_then(Value::as_str)
                .map_or_else(|| record_id(&record).to_string(), str::to_string);
            let fragment_keywords: Option<Vec<String>> = legacy
                .and_then(|legacy| legacy.get("keywords"))
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                });
            keywords.extend(fragment_keywords.iter().flatten().cloned());
            fragments.push(FragmentMemory {
                fragment_id,
                summary: memory_body(&record).to_string(),
                keywords: fragment_keywords,
                created_at: created_at_key(&record),
                scope: placement(&record),
            });
        } else if is_authored_kind(kind) {
            partition.long_term_memories.push(record);
        }
    }
    fragments.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.fragment_id.cmp(&left.fragment_id))
    });
    partition.fragments = fragments;
    partition.fragment_keywords = keywords.into_iter().collect();
    partition
}

/// Whether `kind` is one of the nine authored `nexus.<MemoryKind>` record kinds.
/// The derived `nexus.soul_narrative` kind and the `event` provenance rows are
/// outside that taxonomy, so they are not long-term memory.
fn is_authored_kind(kind: &str) -> bool {
    kind.strip_prefix(NEXUS_KIND_PREFIX)
        .is_some_and(|bare| bare.parse::<MemoryMemoryKind>().is_ok())
}

/// The record's namespaced kind (`nexus.story_summary`, `fact`, `event`, …).
#[must_use]
pub fn memory_kind(record: &RetainedRecord) -> &str {
    match record {
        RetainedRecord::LiveRecord(live) => live.kind.as_str(),
        RetainedRecord::TombstonedRecord(tombstoned) => tombstoned.kind.as_str(),
    }
}

/// The rendered body (`content.text`): what both entry points project.
#[must_use]
pub const fn memory_body(record: &RetainedRecord) -> &str {
    match record {
        RetainedRecord::LiveRecord(live) => live.content.text.as_str(),
        RetainedRecord::TombstonedRecord(tombstoned) => tombstoned.content.text.as_str(),
    }
}

/// The `### Memory:` title and cloud-dedup id: the legacy `memory_id` when the
/// record carries one, else the provider's record id.
#[must_use]
pub fn memory_display_id(record: &RetainedRecord) -> String {
    legacy_metadata(record)
        .and_then(|legacy| legacy.get("memory_id"))
        .and_then(Value::as_str)
        .map_or_else(|| record_id(record).to_string(), str::to_string)
}

/// The legacy slug of a projected long-term memory (`None` for a record that
/// did not come from a bearer file). E6 orders its memory lines by this slug.
#[must_use]
pub fn memory_slug(record: &RetainedRecord) -> Option<&str> {
    legacy_metadata(record)
        .and_then(|legacy| legacy.get("slug"))
        .and_then(Value::as_str)
}

/// The memory-file document a promoted record stands for.
///
/// The frontmatter is the one the migration preserves verbatim in
/// `nexus_legacy`, plus the body. A record the provider authored natively never
/// had a file, so its frontmatter is derived from the record (record id as the
/// memory id, the bare kind, its own `updated_at`).
#[must_use]
pub fn memory_document(record: &RetainedRecord) -> MemoryDocument {
    let legacy = legacy_metadata(record);
    let member = |name: &str| legacy.and_then(|legacy| legacy.get(name));
    let kind = memory_kind(record);
    MemoryDocument {
        nexus_memory_version: member("nexus_memory_version")
            .and_then(Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .unwrap_or(DEFAULT_MEMORY_VERSION),
        memory_id: memory_display_id(record),
        memory_kind: member("memory_kind").and_then(Value::as_str).map_or_else(
            || {
                kind.strip_prefix(NEXUS_KIND_PREFIX)
                    .unwrap_or(kind)
                    .to_string()
            },
            str::to_string,
        ),
        updated_at: member("updated_at")
            .and_then(Value::as_str)
            .map_or_else(|| memory_updated_at(record).to_rfc3339(), str::to_string),
        source_session_ids: member("source_session_ids")
            .and_then(Value::as_array)
            .map_or_default(|sessions| {
                sessions
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            }),
        body: memory_body(record).to_string(),
    }
}

/// The record's placement as E6's per-scope fragment bounds need it.
fn placement(record: &RetainedRecord) -> FragmentScope {
    let scope = match record {
        RetainedRecord::LiveRecord(live) => &live.scope,
        RetainedRecord::TombstonedRecord(tombstoned) => &tombstoned.scope,
    };
    match scope {
        PlacementScope::Shared => FragmentScope::Shared,
        PlacementScope::Context(context_id) => {
            FragmentScope::Context(context_id.as_str().to_string())
        }
    }
}

/// The `updated_at` recency key of the `nexus.<MemoryKind>` sort.
#[must_use]
pub fn memory_updated_at(record: &RetainedRecord) -> &chrono::DateTime<chrono::Utc> {
    match record {
        RetainedRecord::LiveRecord(live) => &live.updated_at,
        RetainedRecord::TombstonedRecord(tombstoned) => &tombstoned.updated_at,
    }
}

/// The record's legacy selection key: the original `created_at` TEXT a
/// migrated row carries verbatim in `nexus_legacy`. The provider only keeps
/// the normalized instant, whose order can differ from that text, so the
/// bridge's raw column is what the legacy read ordered by. A provider-native
/// record never had a legacy column and is keyed by its own rendering.
fn created_at_key(record: &RetainedRecord) -> String {
    legacy_metadata(record)
        .and_then(|legacy| legacy.get("created_at"))
        .and_then(Value::as_str)
        .map_or_else(
            || created_at(record).to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
            str::to_string,
        )
}

fn created_at(record: &RetainedRecord) -> &chrono::DateTime<chrono::Utc> {
    match record {
        RetainedRecord::LiveRecord(live) => &live.created_at,
        RetainedRecord::TombstonedRecord(tombstoned) => &tombstoned.created_at,
    }
}

fn record_id(record: &RetainedRecord) -> &str {
    match record {
        RetainedRecord::LiveRecord(live) => live.record_id.as_str(),
        RetainedRecord::TombstonedRecord(tombstoned) => tombstoned.record_id.as_str(),
    }
}

/// `content.structured.nexus_legacy` — the legacy metadata the provider
/// preserves for CLI/provenance, never an MCA input as a whole.
fn legacy_metadata(record: &RetainedRecord) -> Option<&serde_json::Map<String, Value>> {
    let content = match record {
        RetainedRecord::LiveRecord(live) => &live.content,
        RetainedRecord::TombstonedRecord(tombstoned) => &tombstoned.content,
    };
    content
        .structured
        .as_ref()?
        .get("nexus_legacy")?
        .as_object()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fragment, memory, native};

    #[test]
    fn partition_separates_authored_memory_from_fragments_and_derived_kinds() {
        let records = vec![
            memory(
                "mem_1",
                "story_summary",
                "a summary",
                "2026-01-02T00:00:00Z",
            ),
            fragment("frag_1", "a fragment", &["gate"], "2026-01-03T00:00:00Z"),
            native(
                "rec_3",
                "nexus.soul_narrative",
                "a cache",
                "2026-01-04T00:00:00Z",
            ),
            native("rec_4", "event", "a capture", "2026-01-05T00:00:00Z"),
            native("rec_5", "note", "a provider note", "2026-01-06T00:00:00Z"),
        ];
        let partition = partition(records);
        assert_eq!(partition.long_term_memories.len(), 1);
        assert_eq!(
            memory_kind(&partition.long_term_memories[0]),
            "nexus.story_summary"
        );
        assert_eq!(partition.fragments.len(), 1);
        assert_eq!(partition.fragments[0].summary, "a fragment");
        assert_eq!(partition.fragments[0].scope, FragmentScope::Shared);
        assert_eq!(partition.fragment_keywords, vec!["gate".to_string()]);
    }

    #[test]
    fn memory_document_derives_the_frontmatter_a_record_never_had() {
        let record = memory(
            "mem_abc",
            "story_summary",
            "body text",
            "2026-01-01T00:00:00Z",
        );
        let document = memory_document(&record);
        assert_eq!(document.nexus_memory_version, 1);
        assert_eq!(document.memory_id, "mem_abc");
        assert_eq!(document.memory_kind, "story_summary");
        assert_eq!(document.updated_at, memory_updated_at(&record).to_rfc3339());
        assert_eq!(document.source_session_ids.len(), 0);
        assert_eq!(document.body, "body text");
    }

    #[test]
    fn fragments_sort_newest_first_then_by_fragment_id_desc() {
        let records = vec![
            fragment("frag_a", "older", &[], "2026-01-01T00:00:00Z"),
            fragment("frag_b", "newer", &[], "2026-01-02T00:00:00Z"),
            fragment(
                "frag_c",
                "same instant, larger id",
                &[],
                "2026-01-01T00:00:00Z",
            ),
        ];
        let summaries: Vec<String> = partition(records)
            .fragments
            .into_iter()
            .map(|fragment| fragment.summary)
            .collect();
        assert_eq!(summaries, ["newer", "same instant, larger id", "older"]);
    }

    #[test]
    fn display_id_prefers_the_legacy_memory_id() {
        let projected = memory("mem_abc", "story_summary", "body", "2026-01-01T00:00:00Z");
        assert_eq!(memory_display_id(&projected), "mem_abc");
        assert_eq!(memory_slug(&projected), None);
        let provider_native = native("rec_1", "nexus.custom", "body", "2026-01-01T00:00:00Z");
        assert_eq!(memory_display_id(&provider_native), "rec_1");
    }
}

//! World↔spoke conversion seams.
//!
//! Since V1.145 P1a the `KnowledgeEntryRecord ↔ spoke KnowledgeEntry` conversion seam
//! (spec `spoke-adapter-architecture.md` §7.1) and the `KnowledgeEntryRecord` lifecycle
//! delegation to spoke-operations live **here** (free functions + a local
//! extension trait), not in `nexus-knowledge`.
//!
//! # Why free functions / a trait (not `From` impls)?
//!
//! Both `KnowledgeEntryRecord` (defined in `nexus-knowledge`) and `KnowledgeEntry`
//! (defined in `spoke-schemas`) are foreign to this crate, so the orphan rule
//! (E0117) forbids `impl From<KnowledgeEntryRecord> for KnowledgeEntry` here. The seam
//! is therefore expressed as the free functions [`knowledge_record_to_spoke`] /
//! [`spoke_to_knowledge_record`] (the compiler's own E0117 suggestion), and the nexus
//! lifecycle methods that delegate status transitions to spoke-operations live
//! on the local [`KnowledgeEntryRecordSpokeExt`] trait (a local trait on a foreign
//! type is permitted by the orphan rule).
//!
//! # Dependency direction
//!
//! Housing the seam here reverses the former `nexus-knowledge →
//! nexus-spoke-adapter` edge to `nexus-spoke-adapter → nexus-knowledge`
//! (spec §8 dep-graph reversal), which breaks the cycle that previously
//! blocked aggregating spoke capability against storage in this crate.

pub mod knowledge_entry;
pub mod relation;

pub use knowledge_entry::{spoke_to_knowledge_record, knowledge_record_to_spoke, KnowledgeEntryRecordSpokeExt};
pub use relation::kb_relationship_row_to_spoke;

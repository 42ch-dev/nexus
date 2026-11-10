//! In-memory `KnowledgeEntryPort` stub — answers from a map, records every intent.
//!
//! Dry-run / preview callers need a `KnowledgeEntryPort` that mutates nothing:
//! this type keeps entries in memory, never opens a pool, never calls `KbStore`
//! and never touches `kb_key_blocks`. Each `get` / `put` is recorded in a
//! mutation log the caller inspects to prove what a run *would* have written
//! (K2 §5.1 P0).

use crate::{KnowledgeEntry, KnowledgeEntryPort, SpokeReject, SpokeRejectCode, SpokeResult};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// One recorded port intent, in call order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StubKnowledgeEntryMutation {
    /// A `get_knowledge_entry` call.
    Get { entry_id: String },
    /// A `put_knowledge_entry` call, with the caller's CAS expectation.
    Put {
        entry_id: String,
        expected_base_revision: Option<u64>,
    },
}

#[derive(Debug, Default)]
struct StubState {
    entries: BTreeMap<String, KnowledgeEntry>,
    mutations: Vec<StubKnowledgeEntryMutation>,
}

/// In-memory `KnowledgeEntryPort`: entries live in a map, every call is logged.
#[derive(Debug, Default)]
pub struct StubKnowledgeEntryPort {
    state: Mutex<StubState>,
}

impl StubKnowledgeEntryPort {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every intent recorded so far, in call order.
    #[must_use]
    pub fn mutations(&self) -> Vec<StubKnowledgeEntryMutation> {
        self.lock().mutations.clone()
    }

    /// Takes the mutation log, leaving it empty.
    pub fn drain_mutations(&self) -> Vec<StubKnowledgeEntryMutation> {
        std::mem::take(&mut self.lock().mutations)
    }

    /// Number of entries currently held in memory.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.lock().entries.len()
    }

    fn lock(&self) -> MutexGuard<'_, StubState> {
        // A panicking caller must not destroy the log the assertion reads.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl KnowledgeEntryPort for StubKnowledgeEntryPort {
    async fn get_knowledge_entry(&self, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        let mut state = self.lock();
        state.mutations.push(StubKnowledgeEntryMutation::Get {
            entry_id: entry_id.to_owned(),
        });
        match state.entries.get(entry_id) {
            Some(entry) => SpokeResult::Ok(entry.clone()),
            None => SpokeResult::Reject(SpokeReject {
                code: SpokeRejectCode::KnowledgeEntryNotFound,
                message: format!("KnowledgeEntry not found: {entry_id}"),
                details: None,
            }),
        }
    }

    // shortcut: no CAS — a put always replaces by `entry_id`. Upgrade when P2
    // binds the stub somewhere a revision conflict must be observable.
    async fn put_knowledge_entry(
        &self,
        entry: KnowledgeEntry,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<KnowledgeEntry> {
        let mut state = self.lock();
        state.mutations.push(StubKnowledgeEntryMutation::Put {
            entry_id: entry.entry_id.clone(),
            expected_base_revision,
        });
        state.entries.insert(entry.entry_id.clone(), entry.clone());
        SpokeResult::Ok(entry)
    }
}

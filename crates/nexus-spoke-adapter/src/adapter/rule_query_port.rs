//! Stub `RuleQueryPort` impl — see spec §7.4 production-vs-stub matrix.
//!
//! There is no persisted spoke [`Rule`] storage in nexus today: rules that
//! govern the quality loop come from per-Work config (the
//! `novel-quality-loop` engine and Works-level AGENTS.md), not the spoke
//! `Rule` wire type. Until a future iteration adds a `rules` table that
//! mirrors the spoke shape, the query surface returns the documented empty
//! set — it does **not** fabricate rules from Work config, because that
//! would silently invent a parallel source of truth (call-boundary
//! invariant §7).
//!
//! # Roadmap trigger (spec §7.4 stub matrix)
//!
//! **Trigger:** when a feature owner supplies a spoke `Rule` DTO shape
//! from the quality-loop engine or when the `novel-quality-loop` engine
//! is refactored to serialize rules as spoke `Rule` wire objects. Either
//! path implies a `rules` table + CRUD surface + wire-envelope adoption.
//!
//! **Upgrade path:** add a `rules` table mirroring the spoke `Rule` shape;
//! teach `RuleQueryPort::list_rules` to map `rule_refs` → row lookup;
//! expose rule authoring through the daemon API. Until a trigger fires,
//! this stub is the entire impl.
//!
//! **Residual:** tracked as `R-V1143P0-STRETCH` (closed V1.146 P5 — deferred;
//! precedes ordering is Relation-DAG not fork-port, rules are quality-loop
//! not spoke-core).

use super::NexusAdapter;
use crate::{Rule, RuleQueryPort, SpokeResult};

impl RuleQueryPort for NexusAdapter<'_> {
    /// Stub — returns the documented empty set (spec §7.4).
    ///
    /// Nexus has no persisted spoke `Rule` rows today; rules come from
    /// per-Work config, not the spoke `Rule` wire type. The full impl
    /// is a roadmap item — see the module-level docs.
    fn list_rules(&self, _rule_refs: &[String]) -> SpokeResult<Vec<Rule>> {
        SpokeResult::Ok(Vec::new())
    }
}

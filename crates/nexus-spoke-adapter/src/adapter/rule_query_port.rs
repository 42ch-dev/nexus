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
//! # Roadmap trigger
//!
//! Spec §7.4 stub matrix — upgrade is a roadmap item tracked via the
//! iteration compass "Roadmap Next" rows. The upgrade path is: add a
//! `rules` table; teach `RuleQueryPort::list_rules` to map `rule_refs`
//! → row lookup; expose rule authoring through the daemon API. Until
//! then this stub is the entire impl.

use super::NexusBaselineAdapter;
use crate::{Rule, RuleQueryPort, SpokeResult};

impl RuleQueryPort for NexusBaselineAdapter<'_> {
    /// Stub — returns the documented empty set (spec §7.4).
    ///
    /// Nexus has no persisted spoke `Rule` rows today; rules come from
    /// per-Work config, not the spoke `Rule` wire type. The full impl
    /// is a roadmap item — see the module-level docs.
    fn list_rules(&self, _rule_refs: &[String]) -> SpokeResult<Vec<Rule>> {
        SpokeResult::Ok(Vec::new())
    }
}

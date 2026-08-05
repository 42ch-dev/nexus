//! Generation-type discriminator for slot gating (V1.150 P2, DF-75 — spec
//! `fl-l-w5-prompt-control-plane.md` §4 / Q4 lock).
//!
//! [`MomentRequest`](crate::moment::MomentRequest) carries
//! `generation_stage: Option<GenerationStage>`; the slot engine
//! ([`slots`](crate::slots)) consults it to apply the spec §4 fill matrix.
//!
//! The enum mirrors the shipped creator-workflow `stage` identifiers
//! (`.mstar/specs/creator-workflow.md` §3.1 — `intake`/`research`/`produce`/
//! `review`/`persist`) **plus** the maintenance preset `run_intents`
//! (`.mstar/specs/work-experience-model.md` §5.1 — `work_maintenance` /
//! `system_maintenance`, which have no workflow-stage counterpart and must
//! be expressible for the spec §4 matrix rows), plus `Unspecified`.
//!
//! `run_intent` is **derivable** from the narrative stages (creator-workflow
//! §3.1 mapping) — there is deliberately NO separate `run_intent` field on
//! `MomentRequest` (guide `mca-section-audit.md` Q4 lock).

use std::fmt;

/// Generation-type discriminator for spec §4 slot gating.
///
/// `None` on [`MomentRequest`](crate::moment::MomentRequest) (the default)
/// is treated as [`Self::Unspecified`] — every slot fills, current behavior
/// (neutral golden + direct CLI / inspector path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenerationStage {
    /// Creative Brief Intake (`work_init`).
    Intake,
    /// Reference / KB gathering (`knowledge_ingest` / `work_continue`).
    Research,
    /// Primary drafting / generation (`work_continue`).
    Produce,
    /// Quality loop / revision (`work_continue`).
    Review,
    /// Memory + KB promotion (`knowledge_ingest` / `work_continue`).
    Persist,
    /// Work-adjacent non-narrative upkeep preset run-intent (`work_maintenance`).
    WorkMaintenance,
    /// `_system.*` maintenance preset run-intent (`system_maintenance`) —
    /// no lore slots at all (spec §4, `_system.*` isolation invariant).
    SystemMaintenance,
    /// No generation context (direct `assemble-moment` CLI / inspector
    /// path) — all slots on (spec §4).
    #[default]
    Unspecified,
}

impl GenerationStage {
    /// Stable string form — the CLI `assemble-moment --stage` values.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intake => "intake",
            Self::Research => "research",
            Self::Produce => "produce",
            Self::Review => "review",
            Self::Persist => "persist",
            Self::WorkMaintenance => "work_maintenance",
            Self::SystemMaintenance => "system_maintenance",
            Self::Unspecified => "unspecified",
        }
    }

    /// Parse the CLI string form.
    ///
    /// Returns `None` for unknown values — the caller treats an unknown
    /// value as [`Self::Unspecified`] (safe default, all slots on) and logs;
    /// an unknown stage must never fail or panic the assembly (T3).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "intake" => Some(Self::Intake),
            "research" => Some(Self::Research),
            "produce" => Some(Self::Produce),
            "review" => Some(Self::Review),
            "persist" => Some(Self::Persist),
            "work_maintenance" => Some(Self::WorkMaintenance),
            "system_maintenance" => Some(Self::SystemMaintenance),
            "unspecified" => Some(Self::Unspecified),
            _ => None,
        }
    }
}

impl fmt::Display for GenerationStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_parse_round_trip_for_every_variant() {
        for stage in [
            GenerationStage::Intake,
            GenerationStage::Research,
            GenerationStage::Produce,
            GenerationStage::Review,
            GenerationStage::Persist,
            GenerationStage::WorkMaintenance,
            GenerationStage::SystemMaintenance,
            GenerationStage::Unspecified,
        ] {
            assert_eq!(
                GenerationStage::parse(stage.as_str()),
                Some(stage),
                "parse(as_str()) must round-trip for {stage}"
            );
        }
    }

    #[test]
    fn unknown_stage_parses_to_none_safe_default() {
        // T3: an unknown stage value must degrade to the safe default
        // (`None` ⇒ `Unspecified` ⇒ all slots on) at the caller — never a
        // panic or hard error.
        for bad in ["draft", "revise", "continue", "summarize", "", "PRODUCE"] {
            assert_eq!(
                GenerationStage::parse(bad),
                None,
                "unknown stage {bad:?} must parse to None"
            );
        }
    }

    #[test]
    fn default_is_unspecified() {
        assert_eq!(
            GenerationStage::default(),
            GenerationStage::Unspecified,
            "the default generation type is unspecified (all slots on)"
        );
    }
}

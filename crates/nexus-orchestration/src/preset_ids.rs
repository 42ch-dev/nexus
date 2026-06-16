//! Preset-id constants (single source of truth).
//!
//! Each preset id that is **referenced from more than one module** MUST live
//! here as a `&'static str` const so the three call sites — auto-chain hook,
//! `STAGE_PRESET_ALLOWLIST`, and supervisor guard — share one definition
//! (R-V147P0-06 / V1.48 P0 T3).
//!
//! Literal preset ids used only in a single module (or only in tests) do not
//! need to be hoisted here; the SSOT rule applies to values that are read by
//! runtime logic in ≥2 modules.

/// FL-E `review` stage preset id — `novel-chapter-review` (V1.47 P0).
///
/// Consumed by:
/// - [`crate::auto_chain::persist_review_findings_for_schedule`] (findings hook)
/// - [`crate::schedule::supervisor::ScheduleSupervisor::on_schedule_terminal`]
///   (terminal guard)
/// - [`crate::preset::validation::STAGE_PRESET_ALLOWLIST`] (review stage
///   allowlist entry)
///
/// See `.mstar/knowledge/specs/novel-writing/quality-loop.md` §3 for the normative
/// preset table.
pub const NOVEL_CHAPTER_REVIEW_PRESET_ID: &str = "novel-chapter-review";

#[cfg(test)]
mod tests {
    use super::NOVEL_CHAPTER_REVIEW_PRESET_ID;

    /// Guard against accidental rename: the wire value is part of the
    /// persisted `creator_schedules.preset_id` column and the embedded
    /// preset directory name. Bumping it requires a migration + preset
    /// rename — never a silent edit.
    #[test]
    fn novel_chapter_review_preset_id_value_is_frozen() {
        assert_eq!(NOVEL_CHAPTER_REVIEW_PRESET_ID, "novel-chapter-review");
    }
}

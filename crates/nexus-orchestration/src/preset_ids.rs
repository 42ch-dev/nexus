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

/// FL-E `produce` stage preset id — `novel-writing` (V1.36+).
///
/// Consumed by:
/// - [`crate::auto_chain::preset_version_for_id`] (version map)
/// - [`crate::auto_chain::promote_foreshadowing_for_schedule`] (V1.49 P1
///   narrative-index promotion hook)
/// - [`crate::schedule::supervisor::ScheduleSupervisor::on_schedule_terminal`]
///   (terminal guard for the promotion hook)
///
/// See `.mstar/knowledge/specs/novel-writing/workflow-profile.md` for the
/// normative preset table.
pub const NOVEL_WRITING_PRESET_ID: &str = "novel-writing";

/// Cron-triggered `brainstorm` role preset id — `novel-brainstorm` (V1.50 T-A P1).
///
/// The daemon-side cron evaluator (`schedule::cron_supervisor`) enqueues a
/// pending Schedule with this preset id when the per-Work `brainstorm` role
/// cron fires (spec `cron-staggering.md` §2.1 / §4.1). The existing
/// `ScheduleSupervisor::tick()` then admits it; the existing executor runs it.
/// Out-of-band fire (does NOT touch `driver_schedule_id`), mirroring
/// `enqueue_review_master_schedule`.
pub const NOVEL_BRAINSTORM_PRESET_ID: &str = "novel-brainstorm";

/// Cron-triggered `write` role preset id — `novel-write` (V1.50 T-A P1).
///
/// Enqueued by the cron evaluator when the per-Work `write` role cron fires
/// (spec `cron-staggering.md` §2.1 / §4.1). Out-of-band like brainstorm.
///
/// **Note (R-V150P1CRONBW-01):** the `novel-write` embedded preset is not yet
/// authored as of T-A P1; the cron evaluator enqueues the correct preset id
/// string per spec, and the schedule is persisted + admitted normally, but the
/// executor will fail to load the preset until it is authored in a follow-up
/// plan. This is a preset-authoring gap, not an evaluator gap.
pub const NOVEL_WRITE_PRESET_ID: &str = "novel-write";

#[cfg(test)]
mod tests {
    use super::{
        NOVEL_BRAINSTORM_PRESET_ID, NOVEL_CHAPTER_REVIEW_PRESET_ID, NOVEL_WRITE_PRESET_ID,
        NOVEL_WRITING_PRESET_ID,
    };

    /// Guard against accidental rename: the wire value is part of the
    /// persisted `creator_schedules.preset_id` column and the embedded
    /// preset directory name. Bumping it requires a migration + preset
    /// rename — never a silent edit.
    #[test]
    fn novel_chapter_review_preset_id_value_is_frozen() {
        assert_eq!(NOVEL_CHAPTER_REVIEW_PRESET_ID, "novel-chapter-review");
    }

    #[test]
    fn novel_writing_preset_id_value_is_frozen() {
        assert_eq!(NOVEL_WRITING_PRESET_ID, "novel-writing");
    }

    #[test]
    fn novel_brainstorm_preset_id_value_is_frozen() {
        assert_eq!(NOVEL_BRAINSTORM_PRESET_ID, "novel-brainstorm");
    }

    #[test]
    fn novel_write_preset_id_value_is_frozen() {
        assert_eq!(NOVEL_WRITE_PRESET_ID, "novel-write");
    }
}

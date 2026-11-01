//! Profile admission and content-kind discipline (spoke-memory §4.2/§6.8,
//! MEM-R005, adoption plan D-12).
//!
//! Profiles are the only **trusted** documents the provider stores: authored
//! subject content the product may condition execution on. Two rules shape this
//! module and nothing else does:
//!
//! * **Content kinds only.** A `ProfileDocument` carries authored content — the
//!   core kinds are `personality` and `preferences`. Provider policy (limits,
//!   policy version, freeze allowlist) is configuration and never becomes a
//!   profile body, and no governance/transfer/reflection profile document is
//!   ever fabricated (§7.23 reads honest-empty instead).
//! * **SOUL.md is `personality`** (D-12). The existing Creator SOUL document is
//!   the authored personality profile, so a consumer reading either surface sees
//!   the same content kind.
//!
//! The admission seam distinguishes the three outcomes MEM-R005 keeps apart:
//! a present kind is served, an absent optional kind is honest-empty, and a
//! *required* authored kind that is missing is reported as such so the consumer
//! that needs it can fail closed with `PROFILE_INCOMPLETE`. N1 has no operation
//! that requires an authored profile before acting (profile reads are
//! honest-empty by §7.23 and generation is a sub-capability it never serves), so
//! this module reports the missing set and the refusal is raised by the consumer
//! that needs it — Task 5 wires and tests that refusal.

use crate::memory_provider::storage::ProfileRow;

/// D-12: SOUL.md ↔ the authored `personality` content kind.
pub const SOUL_PROFILE_KIND: &str = "personality";

/// MEM-R005 seam result: what the subject has authored and what a consumer that
/// requires authored profiles is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileAdmission {
    /// Present authored content kinds, sorted.
    pub present: Vec<String>,
    /// §4.2 required authored kinds that are absent; empty = admitted.
    pub missing_required: Vec<String>,
}

impl ProfileAdmission {
    /// `false` iff a required authored kind is missing — the state a consumer
    /// maps to `PROFILE_INCOMPLETE` rather than an empty success.
    #[must_use]
    pub const fn is_admitted(&self) -> bool {
        self.missing_required.is_empty()
    }
}

/// §4.2/MEM-R005: the admission view over a subject's authored profiles.
pub(super) fn admission(rows: &[ProfileRow], required: &[&str]) -> ProfileAdmission {
    let mut present: Vec<String> = rows
        .iter()
        .map(|row| row.profile_kind.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    present.dedup();
    let missing_required = required
        .iter()
        .filter(|kind| !present.iter().any(|present| present == *kind))
        .map(|kind| (*kind).to_string())
        .collect();
    ProfileAdmission {
        present,
        missing_required,
    }
}

/// §7.23 honest-empty read: the profiles the caller named, or every present one
/// when it named none. A kind that is absent is simply not in the result — an
/// authored profile is never invented and absence is never an error.
pub(super) fn select(rows: Vec<ProfileRow>, requested: &[String]) -> Vec<ProfileRow> {
    if requested.is_empty() {
        return rows;
    }
    rows.into_iter()
        .filter(|row| requested.contains(&row.profile_kind))
        .collect()
}

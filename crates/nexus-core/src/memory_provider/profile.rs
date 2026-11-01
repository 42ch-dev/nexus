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
//! The admission seam keeps the three MEM-R005 outcomes apart instead of
//! collapsing them: a required authored kind that is absent is an `Err` the
//! consumer turns into `PROFILE_INCOMPLETE` (never an empty success), an absent
//! *optional* kind is honest-empty and simply not listed, and a missing provider
//! capability is decided at provider selection — it never reaches this module.

use crate::memory_provider::storage::ProfileRow;

/// D-12: SOUL.md ↔ the authored `personality` content kind.
pub const SOUL_PROFILE_KIND: &str = "personality";

/// MEM-R005 admission view: what the subject has authored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileAdmission {
    /// Present authored content kinds, sorted.
    pub present: Vec<String>,
}

/// §4.2/MEM-R005: the admission view over a subject's authored profiles.
///
/// `Err` carries the §4.2 required kinds that are absent — the fail-closed
/// state a consumer reports as `PROFILE_INCOMPLETE`. It is never a success
/// value, so a missing required kind cannot be read as an empty success.
pub(super) fn admission(
    rows: &[ProfileRow],
    required: &[&str],
) -> Result<ProfileAdmission, Vec<String>> {
    let present: Vec<String> = rows
        .iter()
        .map(|row| row.profile_kind.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let missing: Vec<String> = required
        .iter()
        .filter(|kind| !present.iter().any(|present| present == *kind))
        .map(|kind| (*kind).to_string())
        .collect();
    if missing.is_empty() {
        Ok(ProfileAdmission { present })
    } else {
        Err(missing)
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

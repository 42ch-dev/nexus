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
//! collapsing them: a required authored kind that is absent **or empty** is an
//! `Err` the consumer turns into `PROFILE_INCOMPLETE` (never an empty success),
//! an absent or empty *optional* kind is honest-empty and simply not listed, and
//! a missing provider capability is decided at provider selection — it never
//! reaches this module.

use serde_json::Value;

use crate::memory_provider::storage::ProfileRow;

/// D-12: SOUL.md ↔ the authored `personality` content kind.
pub const SOUL_PROFILE_KIND: &str = "personality";

/// MEM-R005 admission view: what the subject has authored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileAdmission {
    /// Authored content kinds carrying content, sorted.
    pub present: Vec<String>,
}

/// §4.2: does this stored profile document carry authored content?
///
/// Emptiness is judged in the representation the document declares:
///
/// * `markdown` — empty or whitespace-only body;
/// * `json` — whitespace-only body, or a body that parses to a structurally
///   empty value (`null`, `""`, `[]`, `{}`). A body that does not parse is
///   authored content, merely malformed: this is emptiness detection, not
///   schema validation.
///
/// A stored body outside the `{ body, format }` shape authors nothing.
///
/// shortcut: only the top level is inspected, so `{"a":{}}` counts as content;
/// upgrade when nested-empty JSON bodies must also fail admission closed.
fn has_content(row: &ProfileRow) -> bool {
    let Ok(document) = serde_json::from_str::<Value>(&row.document_json) else {
        return false;
    };
    let Some(body) = document.get("body").and_then(Value::as_str) else {
        return false;
    };
    let body = body.trim();
    if body.is_empty() {
        return false;
    }
    if document.get("format").and_then(Value::as_str) != Some("json") {
        return true;
    }
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Null) => false,
        Ok(Value::String(text)) => !text.trim().is_empty(),
        Ok(Value::Array(items)) => !items.is_empty(),
        Ok(Value::Object(members)) => !members.is_empty(),
        Ok(_) | Err(_) => true,
    }
}

/// §4.2/MEM-R005: the admission view over a subject's authored profiles.
///
/// A kind is present only when its stored document carries content, so `Err`
/// carries the §4.2 required kinds that are absent **or empty** — the
/// fail-closed state a consumer reports as `PROFILE_INCOMPLETE`. It is never a
/// success value, so a missing or empty required kind cannot be read as an empty
/// success.
pub(super) fn admission(
    rows: &[ProfileRow],
    required: &[&str],
) -> Result<ProfileAdmission, Vec<String>> {
    let present: Vec<String> = rows
        .iter()
        .filter(|row| has_content(row))
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

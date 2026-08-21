//! Shared per-Work cron schedule model + validation core (V1.171 P2 AR-29).
//!
//! The `WorkSchedule` JSON shape (spec §2.1 of `cron-staggering.md`) is written
//! and read by three surfaces that must agree on the model, the accepted cron
//! expression grammar, and the stable validation error codes:
//!
//! - the CLI (`nexus42 creator works cron`) — authoring surface;
//! - the daemon cron evaluator ([`super::cron_supervisor`]) — firing surface;
//! - the daemon HTTP surface (`GET/PUT /v1/daemon/works/{work_id}/cron`,
//!   V1.171 P2 AR-29) — in-product editing surface.
//!
//! Previously the model and validators lived in `nexus42` (circular dependency
//! boundary: the daemon cannot depend on the CLI crate), forcing a hand-rolled
//! mirror in `cron_supervisor`. This module is the unification point: the CLI
//! re-imports from here (behavior-preserving), the evaluator's minimal
//! `CronConfig` mirror stays untouched (its `Option` fields keep partial
//! configs robust — parse tolerance unchanged), and the HTTP handlers validate
//! through the same functions + stable codes the CLI uses.
//!
//! Error codes (stable, AC #5 of the cron foundation spec):
//! - `E_CRON_INVALID_EXPR` — cron expression does not parse
//! - `E_CRON_INVALID_TZ` — not a known IANA timezone
//! - `E_CRON_ALL_ROLES_DISABLED` — every role disabled without an explicit
//!   all-off carve-out (CLI set surface)
//! - `E_CRON_CONFLICTING_FLAGS` — role given both a cron expression and a
//!   disable flag (CLI set surface)

use std::fmt;
use std::str::FromStr;

/// Stable error code constants (used by both the CLI and the daemon handlers).
pub const ERR_INVALID_CRON: &str = "E_CRON_INVALID_EXPR";
pub const ERR_INVALID_TZ: &str = "E_CRON_INVALID_TZ";

/// R-V150-WLA-04 (V1.50 P-last WL-A / cron-foundation qc3 S-004): stable
/// error code for the `--<role> EXPR --no-<role>` self-contradiction.
pub const ERR_ALL_DISABLED: &str = "E_CRON_ALL_ROLES_DISABLED";

/// Stable error code for the "every role disabled without `--all-off`" guard
/// (spec §3.1, R-V150P0-W2).
pub const ERR_CONFLICTING_FLAGS: &str = "E_CRON_CONFLICTING_FLAGS";

/// Canonical default cron expressions per role (spec §2.1 / §2.2).
///
/// These match the novels-system reference table. When `works.schedule_json`
/// is empty/NULL, every consumer uses these defaults.
pub const DEFAULT_BRAINSTORM_CRON: &str = "0 3,9,15,21 * * *";
pub const DEFAULT_WRITE_CRON: &str = "0 4,10,16,22 * * *";
pub const DEFAULT_REVIEW_CRON: &str = "0,30 * * * *";
pub const DEFAULT_TZ: &str = "UTC";

/// Cron-config validation failure carrying the stable error code + a
/// display-ready message.
///
/// Kept independent of any crate's error enum so both the CLI
/// (`CliError::Config`) and the daemon (`NexusApiError`) can map it without
/// coupling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronValidationError {
    /// Stable public code, e.g. `E_CRON_INVALID_EXPR`.
    pub code: &'static str,
    /// Human-readable message (includes the code prefix, matching the CLI's
    /// historical formatting so rerendered output is byte-identical).
    pub message: String,
}

impl CronValidationError {
    #[must_use]
    pub const fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }
}

impl fmt::Display for CronValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CronValidationError {}

/// Per-role cron entry (spec §2.1).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RoleSchedule {
    /// 5-field cron expression (author local TZ).
    pub cron: String,
    /// Per-role opt-out without removing the schedule.
    pub enabled: bool,
}

/// The three-role staggering set (spec §2.1 `roles`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RolesSchedule {
    /// `brainstorm` → `novel-brainstorm` preset.
    pub brainstorm: RoleSchedule,
    /// `write` → `novel-write` preset.
    pub write: RoleSchedule,
    /// `review` → `novel-review-master` preset.
    pub review: RoleSchedule,
}

/// Full per-Work cron configuration (spec §2.1 top-level shape).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkSchedule {
    /// IANA timezone string. Daemon converts to UTC for cron firing.
    pub tz: String,
    /// Per-role cron entries.
    pub roles: RolesSchedule,
}

impl WorkSchedule {
    /// Build the all-defaults schedule (spec §2.3: empty/NULL → defaults).
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            tz: DEFAULT_TZ.to_string(),
            roles: RolesSchedule {
                brainstorm: RoleSchedule {
                    cron: DEFAULT_BRAINSTORM_CRON.to_string(),
                    enabled: true,
                },
                write: RoleSchedule {
                    cron: DEFAULT_WRITE_CRON.to_string(),
                    enabled: true,
                },
                review: RoleSchedule {
                    cron: DEFAULT_REVIEW_CRON.to_string(),
                    enabled: true,
                },
            },
        }
    }

    /// True when every role is disabled. Enforced by the edge surfaces
    /// (CLI `--all-off` / explicit API PUT) rather than here, so callers can
    /// decide whether an all-disabled schedule is acceptable.
    #[must_use]
    pub const fn all_roles_disabled(&self) -> bool {
        !self.roles.brainstorm.enabled && !self.roles.write.enabled && !self.roles.review.enabled
    }
}

/// Validate a 5-field cron expression via the `cron` crate.
///
/// The `cron` crate (zslayton) requires ≥6 fields (seconds first). Standard
/// crontab expressions are 5-field, so we prepend `0 ` (seconds=0) when the
/// input has exactly 5 fields. This preserves semantics:
/// `"0 3,9,15,21 * * *"` (min=0, hour=3/9/15/21) → `"0 0 3,9,15,21 * * *"`
/// (sec=0, min=0, hour=3/9/15/21).
///
/// # Errors
///
/// Returns a [`CronValidationError`] with stable code `E_CRON_INVALID_EXPR`
/// when the expression does not parse.
pub fn validate_cron_expr(expr: &str) -> Result<(), CronValidationError> {
    let normalized = normalize_cron_fields(expr);
    if cron::Schedule::from_str(&normalized).is_err() {
        return Err(CronValidationError::new(
            ERR_INVALID_CRON,
            format!("[{ERR_INVALID_CRON}] invalid cron expression: '{expr}'"),
        ));
    }
    Ok(())
}

/// Normalize a cron expression to the `cron` crate's ≥6-field format.
///
/// 5-field input → prepend `0 ` (seconds=0). 6/7-field input is returned
/// unchanged. Whitespace-only or empty input is left as-is so the parser
/// produces a meaningful error.
#[must_use]
pub fn normalize_cron_fields(expr: &str) -> String {
    let trimmed = expr.trim();
    let field_count = trimmed.split_whitespace().count();
    if field_count == 5 {
        format!("0 {trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Validate an IANA timezone string via `chrono-tz`.
///
/// # Errors
///
/// Returns a [`CronValidationError`] with stable code `E_CRON_INVALID_TZ` when
/// the timezone string is not a known IANA zone.
pub fn validate_tz(tz: &str) -> Result<(), CronValidationError> {
    if chrono_tz::Tz::from_str(tz).is_err() {
        return Err(CronValidationError::new(
            ERR_INVALID_TZ,
            format!("[{ERR_INVALID_TZ}] invalid IANA timezone: '{tz}'"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_build_all_enabled_spec_table() {
        let s = WorkSchedule::defaults();
        assert_eq!(s.tz, DEFAULT_TZ);
        assert_eq!(s.roles.brainstorm.cron, DEFAULT_BRAINSTORM_CRON);
        assert_eq!(s.roles.write.cron, DEFAULT_WRITE_CRON);
        assert_eq!(s.roles.review.cron, DEFAULT_REVIEW_CRON);
        assert!(s.roles.brainstorm.enabled);
        assert!(s.roles.write.enabled);
        assert!(s.roles.review.enabled);
        assert!(!s.all_roles_disabled());
    }

    #[test]
    fn all_roles_disabled_detects_empty_schedule() {
        let mut s = WorkSchedule::defaults();
        s.roles.brainstorm.enabled = false;
        s.roles.write.enabled = false;
        s.roles.review.enabled = false;
        assert!(s.all_roles_disabled());
    }

    #[test]
    fn validate_cron_accepts_spec_defaults_and_six_field() {
        validate_cron_expr(DEFAULT_BRAINSTORM_CRON).unwrap();
        validate_cron_expr(DEFAULT_WRITE_CRON).unwrap();
        validate_cron_expr(DEFAULT_REVIEW_CRON).unwrap();
        validate_cron_expr("0 0 3,9,15,21 * * *").unwrap();
    }

    #[test]
    fn validate_cron_rejects_garbage() {
        assert!(validate_cron_expr("not a cron").is_err());
        assert!(validate_cron_expr("99 99 99 99 99").is_err());
        assert!(validate_cron_expr("").is_err());
    }

    #[test]
    fn validate_cron_error_carries_stable_code() {
        let err = validate_cron_expr("garbage").unwrap_err();
        assert_eq!(err.code, ERR_INVALID_CRON);
        assert!(err.message.contains(ERR_INVALID_CRON));
    }

    #[test]
    fn validate_tz_accepts_iana_zones_and_rejects_garbage() {
        validate_tz("Asia/Shanghai").unwrap();
        validate_tz("UTC").unwrap();
        validate_tz("America/New_York").unwrap();
        assert!(validate_tz("Mars/Olympus").is_err());
        assert!(validate_tz("not-a-zone").is_err());
    }

    #[test]
    fn validate_tz_error_carries_stable_code() {
        let err = validate_tz("Mars/Olympus").unwrap_err();
        assert_eq!(err.code, ERR_INVALID_TZ);
        assert!(err.message.contains(ERR_INVALID_TZ));
    }

    #[test]
    fn normalize_five_field_prepends_seconds_six_field_unchanged() {
        assert_eq!(
            normalize_cron_fields("0 3,9,15,21 * * *"),
            "0 0 3,9,15,21 * * *"
        );
        assert_eq!(normalize_cron_fields("0 0 3 * * *"), "0 0 3 * * *");
    }
}

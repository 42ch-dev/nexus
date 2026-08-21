---
module: nexus-orchestration + nexus42 CLI + nexus-daemon-runtime
date: 2026-08-21
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
applies_when:
  - "Moving a validation/model core out of a CLI binary so a daemon can consume it"
  - "Two surfaces (CLI + HTTP) must share one validator with byte-identical error messages"
  - "A daemon crate cannot depend on the CLI crate (dependency direction is CLI → daemon)"
tags: [shared-core, validation, cli, daemon, dependency-direction, stable-error-codes, re-export]
---

# Shared Validation Core Migration (CLI → Library Crate)

## Context

V1.171 P2 (AR-29) added daemon HTTP endpoints for per-Work cron config
(`GET/PUT /v1/daemon/works/{work_id}/cron`). The cron/TZ validation and the
`WorkSchedule` model lived in the **CLI binary**
(`apps/nexus42/src/commands/creator/works/cron.rs`) with stable error codes
(`E_CRON_INVALID_EXPR`, `E_CRON_INVALID_TZ`, `E_CRON_ALL_ROLES_DISABLED`,
`E_CRON_CONFLICTING_FLAGS`). The daemon cannot depend on `nexus42`
(nexus42 → daemon-runtime direction), so the validation core had to move to a
library crate both can import.

## Guidance

1. **Move the pure core (model + defaults + normalizer + validators + stable
   code constants) into the shared library crate** — here
   `nexus-orchestration::schedule::work_schedule`. The daemon's evaluator
   already had a minimal `CronConfig` mirror; unify behind the shared model
   where feasible **without changing the evaluator's parse tolerance**.
2. **CLI re-imports via `pub use`** — the CLI keeps its public surface
   (`WorkSchedule`, `validate_cron_expr`, …) by re-exporting the shared
   module, plus a `From<CronValidationError> for CliError` impl so existing
   CLI error rendering carries the same `[E_CRON_INVALID_EXPR]` prefix bytes.
   CLI behavior stays byte-identical; all existing CLI tests must stay green.
3. **Fold the resolver too** — the "unset/empty/malformed → defaults"
   decision (`WorkSchedule::resolve(stored: Option<&str>) -> (Self, bool)`)
   belongs in the shared core. Two local copies of the same resolver are a
   drift point (V1.171 QC1 S-1 caught exactly this).
4. **Stable codes live in ONE place** — the constants are part of the wire
   contract (HTTP 400 messages carry them); never fork them per surface.

```rust
// nexus-orchestration/src/schedule/work_schedule.rs
pub struct CronValidationError { pub code: &'static str, pub message: String }
pub fn validate_cron_expr(expr: &str) -> Result<(), CronValidationError> { /* … */ }
impl WorkSchedule {
    pub fn defaults() -> Self { /* … */ }
    pub fn resolve(stored: Option<&str>) -> (Self, bool) { /* unset/empty/malformed → defaults */ }
}

// nexus42 CLI re-export
pub use nexus_orchestration::schedule::work_schedule::{
    WorkSchedule, validate_cron_expr, validate_tz, normalize_cron_fields,
};
impl From<CronValidationError> for CliError { /* byte-identical message prefix */ }
```

## Why This Matters

- **Dependency direction is the constraint**: a daemon HTTP handler cannot
  import from the CLI crate. The shared core is the only way to guarantee the
  HTTP 400 message and the CLI error message carry the same stable code.
- **Byte-identical CLI messages are a contract**: scripts parse
  `[E_CRON_INVALID_EXPR] …`; a re-implementation that changes the prefix
  breaks them silently.
- **One resolver, one truth**: two "defaults-or-parse" copies drift (one
  treats empty as unset, the other doesn't) — the V1.171 `is_default` marker
  bug (QC3 W-2) was exactly this class.

## When to Apply

- Adding a daemon endpoint that must validate the same input a CLI command
  already validates.
- Any two surfaces (CLI/HTTP/worker) that must share validation semantics.
- Moving a model out of a binary crate into a library crate for reuse.

## Examples

### Before
```rust
// CLI only — daemon cannot reach it
// apps/nexus42/src/commands/creator/works/cron.rs
pub fn validate_cron_expr(expr: &str) -> Result<(), CliError> { /* … */ }
// daemon handler re-implements or skips validation
```

### After
```rust
// Shared core in nexus-orchestration; CLI re-exports; daemon imports directly
// daemon handler
let err = work_schedule::validate_cron_expr(&body.roles.brainstorm.cron)
    .map_err(|e| NexusApiError::BadRequest { code: e.code.into(), message: e.message })?;
```

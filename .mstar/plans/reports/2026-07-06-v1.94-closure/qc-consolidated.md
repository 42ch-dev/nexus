# QC Consolidated Decision — V1.94 P-last

**plan_id**: `2026-07-06-v1.94-closure`
**Integrated HEAD under review**: `0e75931b` (iteration/v1.94 post fix-wave `4f4b468b`)
**Diff basis**: `merge-base bf0e60cc` (main HEAD pre-V1.94) + `tip 0e75931b`
**Consolidated verdict**: **Approve (3/3)**

## Tri-review outcomes

| Reviewer | Verdict (initial) | Verdict (post fix-wave) | Report |
|----------|-------------------|-------------------------|--------|
| qc1 — IA + structure lens | Request Changes (5W + 6S) | **Approve** (revalidation `b5ae306b`) | [qc1.md](qc1.md) |
| qc2 — security + correctness lens | Approve (0W + 2S) | Approve (unchanged) | [qc2.md](qc2.md) |
| qc3 — reliability + regression lens | Approve (0W + 0S) | Approve (unchanged) | [qc3.md](qc3.md) |

## Fix-wave triage (qc1 W001-W005 → all fixed)

| # | Warning | Fix (commit `4f4b468b`) | qc1 revalidation |
|---|---------|-------------------------|------------------|
| F-001 | Wizard agent / launch_command not persisted | Tauri command `set_agent_profile` + wizard `finish()` awaits it before `markCompleted()`; `~/.nexus42/agent-host/config.toml` written via `toml_edit` | Fixed + verified (round-trip + order-of-operations tests) |
| F-002 | Dead `presets-page.tsx` | Deleted (−159 lines); no remaining imports; `/presets` falls through to NotFound | Fixed + verified |
| F-003 | Zero unit tests for new primary surfaces | +5 test files (sidebar, setup-wizard-page, strategy-page, active-creator-context, setup-completed-context); 470 → 491 tests | Fixed + verified (meaningful coverage, not smoke-only) |
| F-004 | No button contrast snapshot | New `button.test.tsx` (light + dark primary snapshots); regression-safe via snapshot pin | Fixed + verified |
| F-005 | Footer switcher missing roving-tabindex | Rewritten with `role="toolbar"` + ArrowLeft/Right/Home/End + forwardRef + tabIndex management | Fixed + verified (3 new keyboard tests) |

## Deferred to V1.95 (qc1 Suggestions F-101..F-106 — PM residual registration)

PM will register these 6 suggestions as low-severity open residuals targeting V1.95 (or appropriate target). They are non-blocking; the V1.94 ship is clean.

- F-101: auto-select effect over-fires on every state change (setup-step-agent).
- F-102: health-probe subscription duplicated between `SetupGate` and `SetupStepDaemon`.
- F-103: `SetupStepWelcome` swallows Tauri errors silently.
- F-104: browser-fallback string uses `~/…` while Tauri returns absolute path.
- F-105: `/strategy` redirect drops preset ID (graceful fallback to `/strategies` list).
- F-106: sidebar nav data is open-ended with no compile-time guard.
- (qc1 soft suggestion, revalidation): `set_agent_profile` hardcodes the agent-host config path rather than reusing `nexus_home_layout` helper — matches existing `boot.rs:978` pattern, V1.95 hygiene.

Plus qc2's 2 Suggestions (deferred to V1.95):
- (qc2 S-001): scan endpoint could expose optional `force_refresh` query param (currently always uses cache).
- (qc2 S-002): agent scan could batch `--version` probes via a single async joinset rather than sequential per-entry.

## Decision

**Consolidated Approve (3/3).** Proceed to QA dispatch (P-last step 2).

## Verification snapshot (integrated HEAD `0e75931b`)

- `cargo +nightly-2026-06-26 fmt --all --check`: clean.
- `cargo clippy --all -- -D warnings`: green.
- `cargo test --all`: green (160 acp-host + 421 daemon-runtime + 771 nexus42 + 29 desktop tauri).
- `pnpm --filter web run test`: 491 pass (68 files; +21 from fix-wave).
- `pnpm --filter web run typecheck`: clean.
- `pnpm --filter web run build`: green (3.57s).
- `pnpm run validate-schemas`: 201 valid, 0 invalid.

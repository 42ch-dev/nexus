---
report_kind: qa
plan_id: "2026-06-26-v1.67-frontend-scope-gaps + 2026-06-26-v1.67-overdue-debt-closure"
verdict: Pass
generated_at: "2026-06-27"
reviewer: qa-engineer
---

# QA Report — Wave 2 (P1 + P2)

**Scope**: V1.67 Wave 2 full delivery on `iteration/v1.67` at HEAD (`be11accf`).
- P1: `2026-06-26-v1.67-frontend-scope-gaps` (frontend scope gaps)
- P2: `2026-06-26-v1.67-overdue-debt-closure` (9 overdue residuals)
- Review range: Wave 2 P1+P2 implement + fix-waves (diff basis `26e477ee` / `origin/main`).
- Working branch (verified): `iteration/v1.67`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- QC consolidated: initial Request Changes (C1/W1 for P1; W-001/W-002 for P2) → fix-waves delivered → revalidation sections present in `qc1.md` (P1) and `qc3.md` (P2).

## AC Mapping & Evidence

### P1 G1 — Create-Work `work_profile` selector (R-V164-P2-G1)
- **Canonical values**: `WORK_PROFILES` in `apps/web/src/pages/dialogs/create-work-dialog.tsx:20-25`:
  ```ts
  { value: 'novel' }, { value: 'essay' }, { value: 'game_bible' }, { value: 'script' }
  ```
  `game_bible` uses underscore (matches DB CHECK + Rust `is_game_bible_profile` + daemon handlers).
- **Untouched form omits field (V1.66 NULL semantics)**: `workProfileTouched` guard (line 54-56, 84):
  ```ts
  ...(workProfileTouched ? { work_profile: workProfile } : {}),
  ```
- Tests (create-work-dialog.test.tsx):
  - "submits a well-formed POST /v1/local/works and omits work_profile when untouched (W1)"
  - "sends the selected work_profile when the author changes it (V1.67 G1)"
- Wire drift: `check-wire-drift.sh` clean (no schema change).

### P1 G2 — NexusClient 24 methods (transport half of R-V164-P2-G2)
- `NexusClient` interface (`apps/web/src/lib/nexus/types.ts`): `getPreset` / `updatePreset` / `deletePreset` present (21 → 24).
- Implemented in `BrowserClient` (`browser-client.ts:159-168`); `TauriClient` inherits.
- Tests: `browser-client.test.ts` has dedicated block ("BrowserClient preset CRUD") covering get/update/delete-on-204.
- Adapter contract test updated; contracts package rebuilt (82.09 KB .d.ts).
- No form UI built (deferred to V1.68 canvas per plan §0 Q6).

### P2 — 9 overdue residuals closed (R-V152TA-S001/S006, R-V160P0-*, R-V160P1-*)
- All 9 listed in plan closed with rationale + regression tests (see plan Completion Report table).
- `script.section_status.update` capability: registered in `crates/nexus-orchestration/src/capability/mod.rs` (lines 194-195, 263-264) inside `with_builtins()` and `with_builtins_and_pool()`. Capability count now 34 builtins.
- Migration `PRAGMA foreign_key_check` fail-closed (fix-wave-1): `nexus-local-db/src/lib.rs` post-migrate check + `migrations_fail_on_foreign_key_violation` test.
- `world.delta.apply` pre-fetch chunked (fix-wave-1): `KB_PREFETCH_CHUNK_SIZE = 500`, dedupe + chunked IN-list in `world.rs`; regression test `world_delta_apply_batch_kb_updates_prefetch_chunks`.

## Full CI-equivalent Gate Results

| Gate | Command | Result | Evidence |
|------|---------|--------|----------|
| Rust tests (full) | `SQLX_OFFLINE=true cargo test --all` | **4486 passed**, 0 failed, 17 ignored | Aggregate from 90+ test binaries across workspace |
| Clippy | `cargo clippy --all -- -D warnings` | Clean | Exit 0; no warnings emitted |
| Fmt | `cargo +nightly fmt --all --check` | Clean | No output (exit 0) |
| Wire drift | `bash tooling/check-wire-drift.sh` | 4/4 passed | `schema_drift_detection` + 3 negative cases |
| Schema drift | `bash tooling/check-schema-drift.sh` | All checks passed | 9 explicit ✅ assertions |
| Contracts build | `pnpm --filter @42ch/nexus-contracts run build` | Success | dist/index.d.ts 82.09 KB |
| Web typecheck | `pnpm --filter web typecheck` | Clean | `tsc --noEmit` exit 0 |
| Web build | `pnpm --filter web build` | Success | 2163 modules, dist written |
| Web test | `pnpm --filter web test` | **118 passed (118)** | 15 files, includes G1 selector + P2 client CRUD tests |

All gates green. No CI-equivalent failures.

## Not Tested / Out of Scope
- Full E2E daemon + browser round-trips for every preset CRUD path (unit + adapter-contract cover transport).
- V1.68 canvas UI (explicitly out of scope for both plans).
- Production-scale concurrency for `script.section_status.update` (S-001 noted in qc3; local/single-writer safe).

## Verdict
**Pass**

Wave 2 deliverables match the ACs in both plans. All mandatory CI gates are clean. Behavior spot-checks (canonical `work_profile`, untouched=NULL, 24-method client, `script.section_status.update`, fail-closed migration, chunked prefetch) are present and exercised by tests. QC blocking items were addressed in fix-waves; evidence revalidated via code + test runs.

**Git (this QA only)**: will be recorded after `git add` + commit below.

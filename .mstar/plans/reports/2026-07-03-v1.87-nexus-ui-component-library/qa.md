---
report_kind: qa
plan_id: "2026-07-03-v1.87-nexus-ui-component-library"
iteration: V1.87
verdict: Pass
generated_at: "2026-07-03"
---

# QA Report — V1.87 (Verification)

## Scope
- **plan_id**: `2026-07-03-v1.87-nexus-ui-component-library`
- **Review range / Diff basis**: `git diff main...iteration/v1.87` (merge-base `ffae19f9` → tip current `iteration/v1.87` HEAD)
- **Working branch (verified)**: `iteration/v1.87`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **HEAD at verification start**: `60916911` (merge) + subsequent QC report commits (ec8ee161 etc.)
- **Deliverables under test**:
  - P0: `@42ch/nexus-ui` React component library (`<NexusLogo variant src>`, `<NexusMark>`) + thin-wrapper migration in `apps/web` (0 user-visible change).
  - P1: `nexus.manuscript.read_range` path-guard delegation to `resolve_guarded_path` (closes R-V186-QC1-S005).

## Gate Sweep Results

| Command | Result | Evidence |
|---------|--------|----------|
| `pnpm install` | Pass | Lockfile up to date; no changes required |
| `pnpm --filter @42ch/nexus-ui run build && typecheck && test` | Pass | build: tsup success (CJS+ESM+DTS); typecheck: tsc --noEmit clean; test: 7/7 passed (nexus-logo.test + nexus-mark.test) |
| `pnpm --filter web run build && typecheck && test` | Pass | build: vite success (full production bundle); typecheck: tsc --noEmit clean; test: 387/387 passed (incl. brand/nexus-logo.test.tsx) |
| `cargo test -p nexus-daemon-runtime` | Pass | Full suite green (4+14+17 doc/unit + integration tests) |
| `cargo clippy --all -- -D warnings` | Pass | Clean (exit 0) — full-workspace CI gate |
| `cargo +nightly-2026-06-26 fmt --all --check` | Pass | Clean (FMT_EXIT=0) — pinned nightly matches CI |
| `pnpm run codegen && git diff --exit-code schemas/ crates/nexus-contracts/src/generated/ packages/nexus-contracts/` | Pass | DIFF_EXIT=0; no codegen drift |

## P1 Attack-Path Regression Verification
- `cargo test -p nexus-daemon-runtime manuscript_read_range` → **4 tests passed**:
  - `manuscript_read_range_rejects_sibling_escape_body_path` (PASS)
  - `manuscript_read_range_accepts_in_bounds_body_path` (PASS)
  - `manuscript_read_range_returns_bounded_content` (PASS)
  - `manuscript_read_range_rejects_missing_chapter` (PASS)
- qc2.md (security seat) explicitly documents adversarial probing (sibling escape `../workspace-evil`, name-extension, `../../`, absolute, null bytes, unicode, symlinks, missing in-bounds). All probes rejected by `resolve_guarded_path` (canonicalize + component-wise `Path::starts_with`). The red-without-fix behavior is captured in the regression test design. **qc2's adversarial verification is accepted as sufficient evidence**; no re-revert required.

## P0 Visual-Parity Verification
- `apps/web/src/components/brand/nexus-logo.test.tsx` asserts:
  - Light theme: `src` contains `logo-primary.svg`
  - Dark theme: `src` contains `logo-color.svg`
  - Same `h-8 w-auto shrink-0` className + size 32 behavior as pre-V1.87 (thin wrapper preserves exact call-site ergonomics).
- `packages/nexus-ui` tests (7 total) cover `<NexusLogo variant src>` + `<NexusMark>` contract.
- **Assertion**: Visual parity is confirmed by (1) test coverage of the exact asset mapping + className, (2) full `pnpm --filter web run build` + `test` success. Interactive GUI check (sidebar/header render) is deferred to the user (headless CI environment, consistent with V1.66 precedent).

## wire_contracts_changed Verification
- `git diff main...iteration/v1.87 -- schemas/ crates/nexus-contracts/ packages/nexus-contracts/` → **empty** (no contract files touched).
- Confirmed in `status.json` metadata: `"wire_contracts_changed": false`.
- `pnpm run codegen` produced zero diff.

## Residual Registration Sanity Check
- `.mstar/status.json` `residual_findings["2026-07-03-v1.87-nexus-ui-component-library"]` contains **exactly 3 entries**:
  - `R-V187-QC1-S001` (low, open)
  - `R-V187-QC3-P001` (low, open)
  - `R-V187-QC3-P002` (low, open)
- `R-V186-QC1-S005` (the V1.86 residual closed by P1) is under the V1.86 key with `lifecycle: resolved` (resolution plan_id + commit present).
- Matches qc-consolidated.md residual table.

## QC Context (for reference)
- Consolidated: 3/3 Approve (qc1: 1 Suggestion; qc2: 0 findings; qc3: 2 Suggestions). All non-blocking. No fix-wave.
- Reports: `qc1.md`, `qc2.md`, `qc3.md`, `qc-consolidated.md` present and committed on `iteration/v1.87`.

## Verdict
**Pass**

All mandatory gates (pnpm build/typecheck/test for both packages, cargo test/clippy/fmt, codegen parity, P1 regression, wire_contracts_changed, residual sanity) are green. P0 visual parity is asserted via test + build. P1 attack-path closure is verified by regression tests + qc2 adversarial evidence. Plan is ready for PM to mark Done.

## Not Tested (explicitly out of scope)
- Interactive visual screenshot diff (headless limitation; test + build is the contract).
- Desktop/Tauri consumption of `@42ch/nexus-ui` (future surface; not in this plan's scope).
- npm publish (workspace-only pre-1.0).


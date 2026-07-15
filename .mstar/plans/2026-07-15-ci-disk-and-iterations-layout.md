# Hotfix — CI disk pressure + iterations package layout

| Field | Value |
|-------|-------|
| plan_id | `2026-07-15-ci-disk-and-iterations-layout` |
| type | hotfix (dual-track) |
| Task category | `ops` (Track A) + `docs` (Track B) |
| status | Done |
| working_branch | `fix/ci-disk-and-iterations-layout` |
| merge_target | `main` (squash PR) |
| Execution mode | `inline` (hotfix) |
| QC mode | `single` |
| QA gate | `pm-acceptance` |

## Specify

- **Problem A (CI)**: GitHub Actions run [29402750323](https://github.com/42ch-dev/nexus/actions/runs/29402750323) failed on `Rust tests (orchestration-domain)` with `couldn't create a temp dir: No space left on device (os error 28)` under `target/debug/deps/`. Runner disk exhausted during Rust build/test; similar pressure likely on other heavy Rust jobs (`rust-checks`, `verify-sqlx-offline`, other `rust-tests` matrix legs).
- **Problem B (harness layout)**: `.mstar/iterations/` still has ~97 flat `*-delivery-compass*.md` at the directory root. Latest `mstar-iteration` canonical layout is `{ITERATION_DIR}/<iteration-id>/delivery-compass.md` (+ optional `README.md`, `guides/`, `specs/`). Only v1.117/v1.118 fully match; older workspace dirs (v1.96–v1.116) lack in-package compasses.

## Clarify (post-hoc RCA notes)

- Failure isolated to the largest Rust test matrix group (`orchestration-domain`); other legs succeeded — consistent with peak `target/` footprint, not a flaky test.
- Layout migration is mechanical (`git mv` + reference rewrites); no product runtime change.
- Dual tracks touch disjoint trees: Track A = `.github/` (+ optional root `AGENTS.md` CI hygiene notes); Track B = `.mstar/iterations/**` and reference path updates under `.mstar/` / docs that point at flat compass paths.

## Plan

### Track A — CI disk (`@ops-engineer`)

Working branch: `fix/ci-disk-space` · Worktree: absolute path in Assignment.

Goals (implement the minimum set that reliably prevents ENOSPC):

1. Add a **free-disk** step early on heavy Rust jobs in `.github/workflows/ci.yml` (and desktop workflows if they share the same risk), reclaiming common runner bloat (`dotnet`, Android SDK, Haskell, large unused toolchains, Docker images as appropriate).
2. Reduce peak `target/` pressure: prefer scoped cargo commands already in matrix; consider `CARGO_INCREMENTAL=0`, pruning between steps, or further splitting `orchestration-domain` if free-disk alone is insufficient.
3. Review `Swatinem/rust-cache` usage — restore of a large cache + full rebuild can exceed free space; adjust keys / save behavior / cleanup so restore + compile fits on `ubuntu-latest`.
4. Document the invariant briefly in workflow comments (and root `AGENTS.md` CI note only if necessary).

Non-goals: changing product Rust code; weakening test coverage; switching off cache entirely without measuring.

### Track B — iterations package layout (`@writing-specialist`)

Working branch: `fix/iterations-package-layout` · Worktree: absolute path in Assignment.

Canonical target (from `mstar-iteration/references/iteration-artifact-boundaries.md`):

```text
.mstar/iterations/
  README.md                          # one row per iteration → <id>/
  <iteration-id>/
    delivery-compass.md              # SSOT compass
    README.md                        # optional package index
    guides/
    specs/
```

Tasks:

1. `git mv` each root-level `v1.*-*-delivery-compass*.md` into `<id>/delivery-compass.md` (create package dirs as needed; merge into existing `v1.*/` workspaces that lack compass).
2. Move non-compass iteration root leftovers (overview/matrix/completion-report sidecars) into the matching package `guides/` (or keep beside compass with clear names — prefer `guides/`).
3. Leave already-canonical packages (v1.117, v1.118) untouched except README index consistency.
4. Rewrite `.mstar/iterations/README.md` to **one row = one iteration directory** (no flat compass links).
5. Update live references under `.mstar/` (and any non-archived docs that break) from flat paths → `<id>/delivery-compass.md`. Prefer search-replace of path strings; use `git mv` for files so history is preserved.
6. Do **not** invent new iteration content; mechanical reorg + link hygiene only.

## Tasks

- [x] T-A — CI disk optimization on `fix/ci-disk-space` (commit evidence)
- [x] T-B — iterations package layout migration on `fix/iterations-package-layout` (commit evidence)
- [x] T-merge — merge both tracks → `fix/ci-disk-and-iterations-layout`
- [x] T-QC — single-seat QC (`inline`)
- [x] T-accept — PM acceptance + squash PR to `main`

## Acceptance

- CI: heavy Rust jobs reclaim disk before cargo; no ENOSPC on a representative PR run (or dry-run evidence of free space + reasoned cache/build changes).
- Layout: no new flat `*-delivery-compass*.md` at `.mstar/iterations/` root; each historical compass lives at `<id>/delivery-compass.md`; root README indexes directories only; live refs updated.
- Disjoint tracks; no wire/schema/product behavior change.

## Merge order

1. Merge `fix/ci-disk-space` → `fix/ci-disk-and-iterations-layout`
2. Merge `fix/iterations-package-layout` → `fix/ci-disk-and-iterations-layout`
3. QC on integration HEAD → PR to `main`


## Review Gate Summary

| Field | Value |
|-------|-------|
| QC | Approve with residuals (single-seat hotfix) |
| Report | `.mstar/sdd/2026-07-15-ci-disk-and-iterations-layout/review/qc.md` |
| Reviewer | `@qc-specialist` |
| Date | 2026-07-15 |

### Residuals

| ID | Severity | Note |
|----|----------|------|
| R-HFIX0715QC1-L001 | low | **Resolved** — PR #150 CI green (`orchestration-knowledge` pass, no ENOSPC) |

## QA Gate Summary

| Field | Value |
|-------|-------|
| Gate | `pm-acceptance` (hotfix; residual is post-PR CI verification only) |
| Checklist | Tracks A+B merged; QC Approve with residuals; free-disk action present; iterations root has no flat compasses; PR opened to `main` |
| Date | 2026-07-15 |

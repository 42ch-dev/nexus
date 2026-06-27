---
report_kind: qc-consolidated
reviewer: "@project-manager"
plan_id: "2026-06-27-v1.68-cli-relocation-and-product-surface-formalization"
iteration: V1.68
wave: 1
active_wave: 1
generated_at: 2026-06-27
consolidated_verdict: "Approve (after fix wave + targeted re-review)"
blocking_findings: []
re_review: "qc1 + qc3 targeted re-review after fix 630df3af → both Approve; qc2 Approve (unchanged). 3/3 Approve."
---

# QC Consolidated — V1.68 P0 (initial tri-review wave 1)

## Tri-review verdicts

| Seat | Verdict | Critical | Warning | Suggestion | Report |
|------|---------|----------|---------|------------|--------|
| qc1 (`@qc-specialist`) | **Request Changes** | 0 | 1 (W-001) | 0 | `qc1.md` |
| qc2 (`@qc-specialist-2`) | **Approve** | 0 | 0 | 1 (S-01) | `qc2.md` |
| qc3 (`@qc-specialist-3`) | **Request Changes** | 0 | 1 (W-001, cross-confirmed) | 1 (S-02) | `qc3.md` |

**Consolidated verdict: Request Changes** — 1 blocking Warning (W-001), cross-confirmed independently by qc1 + qc3. Gate rule (`mstar-review-qc`): unresolved Warning ⇒ Request Changes.

## Blocking finding

### W-001 — `desktop-build.yml` path filter omits `apps/nexus42/**` (CI coverage regression)
- **Nature**: The relocation moved the CLI from `crates/nexus42/**` (covered by desktop-build's `crates/**` path filter) to `apps/nexus42/**` (covered by **no** entry: the filter lists `apps/web/**` + `apps/desktop/**` but not `apps/nexus42/**` or a blanket `apps/**`).
- **Regression classification (PM-verified per `.mstar/AGENTS.md` pre-existing-claim protocol)**: **V1.68-introduced regression, NOT pre-existing.** On `main`, CLI changes triggered desktop-build via the `crates/**` glob (the filter has no explicit `crates/nexus42` string, which is why the pre-flight string-grep missed it). After V1.68, a PR touching only `apps/nexus42/**` will **not** trigger the desktop-bundle CI — even though the desktop app **bundles the `nexus42` sidecar**, so CLI-source changes are material to the desktop build.
- **Why pre-flight + architect missed it**: the discovery grep searched for the literal `crates/nexus42`; coverage was via the `crates/**` **glob**, invisible to a string search. (Lesson: relocation reviews must audit path-filter **globs**, not just literal strings.)
- **Scope of impact**: only `desktop-build.yml`. `ci.yml` uses `paths-ignore` (docs-only exclusions) → runs on all code incl. `apps/nexus42/**` → **no gap**. (PM-verified.)
- **Fix**: add `- 'apps/nexus42/**'` to **both** `push.paths` and `pull_request.paths` in `.github/workflows/desktop-build.yml` (2 lines). Minimal, matches existing per-app granularity (`apps/web/**`, `apps/desktop/**`).

## Non-blocking

- **S-01 (qc2)**: README cross-link polish — defer / optional.
- **S-02 (qc3)**: `apps/AGENTS.md` trailing Markdown hard-break spaces flagged by `git diff --check` — cosmetic; fix opportunistically in the W-001 fix wave (free).

## What passed (qc1 evidence)
- `cargo build --all`, `cargo clippy --all -D warnings`, `cargo test -p nexus42` (762 lib + 30 integration + 2 doc-tests, 0 failed), `cargo test --workspace` (0 failed), `cargo +nightly fmt --all --check`, `tooling/check-schema-drift.sh` — **all clean**.
- **Byte-identity**: 11 representative `.rs` files identical between old `crates/nexus42/` (main HEAD) and new `apps/nexus42/` → **zero source changes** confirmed (pure rename).
- **12-live-ref grep empty**; broader non-`.mstar` grep empty; behavioral equivalence (crate/binary name `nexus42` unchanged; sidecar resolves by name).
- **Scope boundary honored**: no historical `.mstar/` record rewritten; `wire_contracts_changed: FALSE` (no `schemas/`/codegen/contracts touched) — qc3-verified.

## Decision & routing
1. **Fix wave** (`@fullstack-dev`, targeted): apply W-001 fix (2 lines in `desktop-build.yml`) + S-02 cosmetic (trailing-space cleanup in `apps/AGENTS.md`). Commit on `iteration/v1.68`; push.
2. **Targeted re-review**: qc1 + qc3 only (the seats that raised W-001; qc2 is Approve and out of scope for a CI-workflow change). Each updates the **same** `qc1.md` / `qc3.md` with a `## Revalidation` section + refreshed verdict (`mstar-review-qc` targeted re-review path — no `qcN-rev2.md` siblings).
3. **W-001 residual**: registered in `status.json` `residual_findings[<plan-id>]` as open Warning; closed (→ `archived/residuals/`) upon re-review Approve.
4. After consolidated Approve → `@qa-engineer` → PM `Done` → PR `iteration/v1.68` → `main`.

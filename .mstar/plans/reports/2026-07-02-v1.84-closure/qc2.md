---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-02-v1.84-closure"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (Reviewer #2)
- Report Timestamp: 2026-07-02

## Scope
- plan_id: 2026-07-02-v1.84-closure
- Review range / Diff basis: merge-base: main (1b19d69c) … tip: iteration/v1.84 HEAD (380ae1b6). Equivalent to `git diff main...iteration/v1.84`.
- Working branch (verified): iteration/v1.84
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 11 (focus on P0+P1 product changes; harness/plan files noted for scope only)
- Commit range: 380ae1b6 (P1 merge) + 06090e94 (P0 merge) on top of merge-base 1b19d69c
- Tools run: git diff main...iteration/v1.84, git diff --name-only, git log, git branch --show-current, git rev-parse, rg/grep for token usage, .gitattributes inspection, package.json + vitest.config.ts direct read, nexus-ui build smoke

**P0 changes (8c249484):** `apps/web/src/index.css` — brand-blue hex/rgba → `var(--color-blue-700)` and `color-mix(in srgb, var(--color-blue-700) N%, transparent)`.
**P1 changes (da123db7):** CI `lfs: true` on 4 checkouts; new self-contained `nexus-ui` CI job; narrow vitest `ExperimentalWarning`/`localStorage` filter in `apps/web/vitest.config.ts`.

**Contract invariant verification:** `git diff --name-only main...iteration/v1.84` contains **no** `schemas/`, no `@42ch/nexus-contracts` package.json/version bump, no daemon/local-API routes, no `packages/nexus-ui/package.json` exports change. `wire_contracts_changed: false` holds.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- **S1 (maintainability, low):** The vitest warning filter re-registers the listener inside the handler via `nextTick`. While functionally correct and leak-free for this narrow case, consider extracting to a one-time `setupFiles` helper or using `process.removeListener` + conditional re-attach for future-proofing if more filters are added. Current implementation is safe and well-commented; no action required for V1.84.
- **S2 (documentation):** `.github/workflows/ci.yml` and `desktop-build.yml` now have `lfs: true` in four places. A one-line comment in the workflow files (or in `setup-monorepo` action) noting "LFS required for packages/nexus-ui/assets/logos/*.png brand provenance" would make the intent self-documenting for future contributors. Not a correctness issue.

## Source Trace
- **P0 color consolidation (W004/W005):** Direct `git diff main...iteration/v1.84 -- apps/web/src/index.css`. Verified 6 solid hex `#1e3a5f`/`#25d1e0` (light/dark) replaced by `var(--color-blue-700)`; 5 rgba brand-blue tints replaced by matching `color-mix(in srgb, var(--color-blue-700) N%, transparent)`. Canonical `--color-blue-700` defs at L60 (light `#1e3a5f`) and L209 (dark `#25d1e0`) untouched. Cross-checked against `apps/web/DESIGN.md` (focus-ring contract) and `apps/web/src/index.css` usage sites (focus, selected borders, badges).
- **LFS scoping:** `cat .gitattributes` (only `packages/nexus-ui/assets/logos/*.png`); `git diff` showed no `.gitattributes` change in range. No broad patterns.
- **CI LFS + nexus-ui job:** `git diff main...iteration/v1.84 -- .github/workflows/ci.yml .github/workflows/desktop-build.yml`. New job uses identical pinned checkout + `setup-monorepo` with `rust-toolchain: ""`; runs only `@42ch/nexus-ui` build/typecheck.
- **Vitest filter:** Full file read of `apps/web/vitest.config.ts`. Handler requires BOTH `name === 'ExperimentalWarning'` AND `message.includes('localStorage')`; re-emits via `emitWarning` after `off` + `nextTick`.
- **Scope/contract:** `git diff --name-only main...iteration/v1.84 | grep -E '(schemas|contracts|nexus-ui/package|daemon|api)'` returned empty.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

**Rationale (security & correctness lens):**  
- **P0 (token hygiene):** Visual values are bit-for-bit identical before/after. `var(--color-blue-700)` resolves to the exact prior hex in each theme. `color-mix(in srgb, ... N%, transparent)` for the documented alphas (12/14/16/22%) produces the same sRGB result as the prior `rgba(R,G,B,alpha)` for opaque brand blues. No contrast regression on focus-rings, selected borders, or badges (these tokens were already intended to be the brand blue per DESIGN.md). No a11y or visual drift.
- **P1 (CI/LFS):** `lfs: true` is narrowly scoped by `.gitattributes` to public brand PNGs only — zero security surface (no private LFS, no credential impact, no broad binary fetch). New `nexus-ui` job is self-contained, uses same pinned actions, sets empty rust toolchain correctly, and will not silently skip. Vitest filter is surgically narrow (dual predicate) and correctly re-emits all other warnings; no listener-leak or swallowed-assertion risk.
- No contract, daemon, or wire changes. No new attack surface. CI hygiene only.

All mandatory baseline checks (regression, security/correctness, test coverage impact, scope) pass. No unresolved Critical or Warning.

---

## Reviewer Notes (qc2-specific)
- Confirmed cwd/branch/HEAD/merge-base verbatim before any diff.
- All P0 substitutions are pure SSOT hygiene; no value mutation.
- `color-mix` percentages match original alphas exactly (no off-by-one).
- LFS change introduces no new fetch risk.
- No secret, token, or credential exposure in any diff hunk.
- CI job addition does not alter runtime behavior or introduce cross-job secret leakage.
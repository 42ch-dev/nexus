---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-27-v1.68-cli-relocation-and-product-surface-formalization"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: documentation accuracy, placement-rule consistency, contributor clarity (security_correctness focus)
- Report Timestamp: 2026-06-27

## Scope
- plan_id: `2026-06-27-v1.68-cli-relocation-and-product-surface-formalization`
- Review range / Diff basis: `4606395e..2a4e5577` (origin/main → iteration/v1.68 HEAD; substantive implement is commit `2a4e5577`; earlier commits are Prepare docs)
- Working branch (verified): `iteration/v1.68`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 8 (primary docs + placement artifacts; plus cross-check of 12 live ref targets)
- Commit range: `4606395e..2a4e5577`
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git diff --stat`, `git grep` (targeted live files), direct file reads of apps/AGENTS.md, root AGENTS.md, README.md, docs/ARCHITECTURE.md, plan, compass

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- **S-01 — Minor wording polish opportunity in root README Monorepo Layout**  
  The table is now accurate and includes desktop + web (previously omitted). Suggestion: consider adding a one-sentence "see apps/AGENTS.md for the durable placement rule" cross-link under the `apps/` row for first-time contributors. Not blocking; current state already satisfies acceptance criteria.  
  → File: `README.md:18-28`

## Source Trace
- Finding ID: (N/A — no findings)
- Source Type: manual-reasoning + targeted grep + file reads
- Source Reference: apps/AGENTS.md (full), root AGENTS.md:36 + :55, README.md:18-28, docs/ARCHITECTURE.md:118, plan C1, compass §6 + Track C
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

## Detailed Review (qc2 focus)

### 1. `apps/AGENTS.md` correctness
- Declares `apps/` = **polyglot product-surfaces directory** (line 1, 3).
- Table (lines 5-9) correctly lists:
  - `nexus42` (Rust) — **Producer** — CLI + integrated daemon runtime composition root
  - `desktop` (TS + Tauri) — **Consumer** — Tauri client over IPC + bundled sidecar
  - `web` (TS) — **Consumer** — browser SPA
- Durable placement rule (lines 11-21) matches **verbatim** the locked wording from compass §6 and plan C1:
  > `apps/` = **product surfaces** — runnable things you install or use, any language.  
  > `crates/` = **reusable Rust libraries** — building blocks.  
  > ...  
  > App-owned nested Rust (for example `apps/desktop/src-tauri/`) lives inside its app directory — it is product-surface implementation, not a shared library. Promote it to `crates/` only if it becomes a reusable building block shared across surfaces.
- Producer/consumer wire-boundary section present and accurate.
- Per-entry authority links correct.
- **No drift**. Rule is consistent across compass, plan, and `apps/AGENTS.md`.

### 2. Placement-rule consistency (compass §6 / Track C / plan C1 / apps/AGENTS.md)
- All four locations use identical core phrasing for the durable rule.
- Compass §6 (final layout reference) and Track C description match the delivered `apps/AGENTS.md` exactly.
- Plan C1 explicitly required creation of this file with these contents; delivered as specified.
- Root `AGENTS.md:55` correctly points readers to `apps/AGENTS.md` for the rule.
- No contradictions or softened language.

### 3. README accuracy
- New "Monorepo Layout" section (lines 18-28) correctly maps:
  - `apps/` (products — now lists `nexus42`, `desktop`, `web`)
  - `crates/` (libs)
  - `packages/`, `modules/`, `tooling/`, `schemas/`
- Previously omitted desktop + web; now present.
- Quick Start remains valid (still builds/runs `nexus42` binary; path unchanged because binary name is stable).
- No stale `crates/nexus42` references in the visible surface.

### 4. Root AGENTS.md index + docs/ARCHITECTURE.md
- Subdirectory index row moved: `apps/nexus42/` (line 36).
- Explanatory sentence added at line 55: "`apps/` is the polyglot product-surfaces directory."
- `docs/ARCHITECTURE.md:118` correctly shows `apps/nexus42` in the executable surface table.
- No stale `crates/nexus42` links remain in these two files.
- Cross-links are valid and point to the new location.

### 5. Live reference hygiene (Track B + V7)
- Targeted `git grep` across the exact 12 live files listed in the plan (Cargo.toml, tooling/check-schema-drift.sh, AGENTS.md, docs/ARCHITECTURE.md, 3 app files, 5 knowledge/specs) returned **zero** remaining `crates/nexus42` hits.
- Historical ~978 records intentionally untouched (per Track D) — correct per scope lock.
- `wire_contracts_changed: FALSE` — no schema or codegen impact.

### 6. Doc quality checklist (shared baseline)
- Naming clear and consistent.
- No contradictions between documents.
- No broken relative links in the changed surface.
- Placement rule is explicit, durable, and enforceable.
- Contributor clarity improved: `apps/` now visibly contains the runnable product; `crates/` is libs only.

## Evidence Summary
- All acceptance criteria (plan §67) are met:
  1. `apps/nexus42` exists; `crates/nexus42` does not.
  2. `crates/` contains only library crates.
  3. Build/test/clippy/fmt + schema-drift checks passed (per commit message).
  4. `apps/AGENTS.md` states polyglot model + durable placement rule.
  5. Root README has Monorepo Layout including desktop + web.
  6. 12 live files have zero `crates/nexus42`; historical records preserved.
  7. `wire_contracts_changed: FALSE`.

## Revalidation Notes (N/A — initial review)
- This is the initial full tri-review wave. No prior qc2 findings to revalidate.

**Verdict**: Approve

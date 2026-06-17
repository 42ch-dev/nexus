---
report_kind: qc-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-13-v1.45-quickstart-and-author-spec
verdict: Approve
generated_at: 2026-06-14T15:30:00Z
review_range: "merge-base: 997ebd8a; tip: HEAD (8f330834); equivalent: git diff 997ebd8a...HEAD"
working_branch: iteration/v1.45
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
---

# Code Review Report — QC #1 (Architecture / Maintainability) for V1.45 P3

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.1
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-14T13:10:00Z

## Scope
- plan_id: 2026-06-13-v1.45-quickstart-and-author-spec
- Review range / Diff basis: merge-base: 997ebd8a; tip: HEAD (8f330834); equivalent: git diff 997ebd8a...HEAD
- Working branch (verified): iteration/v1.45
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 3 (all .md)
- Commit range: 997ebd8a..8f330834 (6 commits: 5 P3 docs + 1 merge)
- Tools run:
  - `git log --oneline 997ebd8a..HEAD`
  - `git diff 997ebd8a...HEAD --stat`
  - `git diff 997ebd8a...HEAD` (per-file, full hunks)
  - `git show` per P3 commit (62cd69b4, 72c564af, ceb75cfb, d62360eb, 2c1f2e76)
  - Full content read of all 3 changed files + quality-loop spec full body
  - `rg` for remaining old command names across `docs/` and changed specs
  - `rg` for `creator run status` in Rust source (migration verification)
  - `cargo +nightly fmt --all -- --check`
  - `cargo clippy --all -- -D warnings`
  - `cargo test -p nexus42 --test command_surface_contract`
  - Migration appendix compliance cross-check (compass §2, 13 mappings)

**Changed files (P3 only):**
- `docs/novel-writing-quickstart.md`
- `.mstar/knowledge/specs/novel-writing/author-experience.md`
- `.mstar/knowledge/specs/novel-writing/quality-loop.md`

## Architecture & Maintainability Review

### Migration appendix compliance (compass §2)

All 13 old→new mappings from the compass migration appendix were checked against the three changed files. Every mapping that appears in the quickstart or specs is correctly reflected:

| Compass §2 mapping | Reflected in docs? |
|---|---|
| `start` → `creator bootstrap` | ✓ quickstart §2, §6 |
| `continue` → `creator works inspire` | ✓ quickstart §3, §4, Part II-C |
| `resume --reopen` → `creator works reopen` | ✓ quickstart §6 |
| `resume` → `creator works resume-chain` | ✓ quickstart §4, §6 |
| `reconcile-chapters` → `creator works reconcile-chapters` | ✓ quickstart §4 |
| `stage list` → `creator works status` | ✓ quickstart throughout |
| `stage advance --stage produce` → `creator run novel-writing` | ✓ quickstart §3 |
| `stage advance --stage review` → `creator run reflection-loop` | ✓ quickstart §5 |
| `review-master` → `creator run novel-review-master` | ✓ quickstart §5, quality-loop §3.4/§6 |

(`research`, `kb-extract`, `audit-chapter` mappings are not referenced in the quickstart or changed spec sections — expected, since the quickstart doesn't cover those flows.)

**No remaining old command names** in `docs/novel-writing-quickstart.md`. The only `review-master` reference in specs is the quality-loop overlay section explicitly documenting the delta — correct by design.

### Spec consistency

Both spec files (`novel-writing/author-experience.md`, `novel-writing/quality-loop.md`) show the same CLI surface as the quickstart for all overlapping commands. The three-plane IA (bootstrap / works / run) is consistently applied. No drift between user guide and normative supplements was found for the command names themselves.

### Surgical changes

P3 correctly amended only the overlay sections without promoting into the Master body of `cli-spec.md` (which is P-last scope). The quality-loop §3.4 Draft overlay section is now marked "(applied P3 2026-06-14)" with per-item "applied P3" annotations — clean tracking without premature promotion.

### Out-of-scope verification

P3 touched only 3 `.md` files. No `.rs`, `.sql`, `schemas/`, or `cli-spec.md` Master body changes were introduced. ✓

### CI gates

- `cargo +nightly fmt --all -- --check` — **PASS** (no output)
- `cargo clippy --all -- -D warnings` — **PASS** (0 warnings)
- `cargo test -p nexus42 --test command_surface_contract` — **PASS** (37/37)

## Findings

### 🔴 Critical

None.

### 🟡 Warning

**W-1: Stale inline comment in quickstart §5 retains V1.44 `review-master` semantics**

`docs/novel-writing-quickstart.md` line 172:

```bash
# List master findings (default), then enqueue the review for a specific finding:
nexus42 creator run novel-review-master <work_id> --finding-id <finding_id>
```

The comment "List master findings (default)" describes the V1.44 `review-master` behavior where the bare invocation listed findings. In V1.45, `novel-review-master` is **enqueue-only** — the quality-loop spec §3.4 behavior table (line 77–81) has no "Default" row; the old Default behavior ("Lists open findings") was removed because listing moved to `creator works status`. The P3 commit `62cd69b4` changed only the command name (`review-master` → `novel-review-master`) but carried the comment verbatim.

**Impact**: A user reading this would expect `creator run novel-review-master <work_id>` (without flags, line 170) to be a safe read-only listing operation, when it actually **enqueues** the master-decision preset — a write/scheduling action. This contradicts the blockquote on line 179 which correctly states "enqueues".

**Fix**: Update the comment to reflect enqueue-only semantics, e.g.:
```bash
# Enqueue the master-decision review scoped to a specific finding:
```

---

**W-2: Stale `creator run status` reference in quality-loop spec §6**

`.mstar/knowledge/specs/novel-writing/quality-loop.md` line 108:

```
2. `creator run status` banner lists stale count + `novel-review-master` hint.
```

`creator run status` is no longer a valid command in V1.45. V1.45 P2 migrated it to `creator works status` (confirmed by `crates/nexus42/src/commands/creator/works/mod.rs:39`: `/// Migrated from \`creator run status\` (V1.41).`). In V1.45, `creator run` is generic preset dispatch — `creator run status` would attempt to resolve a preset named `status`, which does not exist.

The P3 commit `2c1f2e76` ("T5 — update novel-quality-loop §3.4/§6 to preset-id commands") already edited this exact line (updating `review-master` → `novel-review-master`) but did not update the command name in the same pass.

**Impact**: A user or implementer following the normative spec would encounter a preset-resolution error. The quickstart correctly uses `creator works status` everywhere — this is a spec-vs-quickstart drift.

**Fix**: Change `creator run status` → `creator works status` on line 108.

### 🟢 Suggestion

**S-1: Quality-loop spec §3.4 "Presentation" section has an orphaned empty-findings requirement**

`.mstar/knowledge/specs/novel-writing/quality-loop.md` lines 83–87:

```
**Presentation** (minimum):

- Use `creator works status` to list open findings with severity breakdown
- Quickstart §5 updated to cite `creator run novel-review-master` as primary path (V1.45 P3)
- On empty findings: single line "No master findings" + quickstart §5 link
```

The third bullet ("On empty findings: single line 'No master findings'") was carried over from the V1.44 presentation section where `review-master` produced this output by default. In V1.45, `novel-review-master` is enqueue-only and listing moved to `creator works status`. It is now ambiguous which command owns this presentation requirement — `novel-review-master` (enqueue, unlikely to list) or `creator works status` (already has its own findings display). Consider clarifying attribution or deferring this to the P4 `works status` enhancement.

## Source Trace

- **W-1**: Source Type: git-diff + manual-reasoning. Source Reference: `docs/novel-writing-quickstart.md` line 172 vs `.mstar/knowledge/specs/novel-writing/quality-loop.md` §3.4 behavior table (lines 77–81, no Default row). Confidence: High.
- **W-2**: Source Type: git-diff + code-verification. Source Reference: `.mstar/knowledge/specs/novel-writing/quality-loop.md` line 108 vs `crates/nexus42/src/commands/creator/works/mod.rs:39` migration comment. Confidence: High.
- **S-1**: Source Type: manual-reasoning + spec-analysis. Source Reference: `.mstar/knowledge/specs/novel-writing/quality-loop.md` lines 83–87. Confidence: Medium.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

Two unresolved Warning findings remain. Both are one-line fixes in documentation that P3 already surgically edited but left with stale V1.44 semantics. The migration is otherwise comprehensive and architecturally sound — the three-plane IA is consistently applied, spec↔quickstart command surfaces agree (except for the two stale references noted), no out-of-scope files were touched, and all CI gates pass. Once W-1 and W-2 are corrected, this plan should be ready for approval.

---

## Revalidation (P3 fix round, 2026-06-14)

### Re-review scope
- Review range: `54d80e07..HEAD` (= `03baf31e` on `iteration/v1.45`); equivalent `git diff 54d80e07...HEAD` (fix commits only)
- Fix commits reviewed: 5 (`fa438f95`, `076b431e`, `8bd86369`, `e8f2f5e1`, `1fc2c2d6`) + 1 merge (`03baf31e`)
- Files changed: 3 (`docs/novel-writing-quickstart.md`, `.mstar/knowledge/specs/novel-writing/quality-loop.md`, `crates/nexus42/src/commands/creator/run.rs`); +27 / -6
- Working branch (verified): `iteration/v1.45`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`

### Original findings — fix verification

| ID | Original | Status | Evidence |
|----|----------|--------|----------|
| W-1 | "List master findings (default)" misleading inline comment | **FIXED** | `076b431e`: comment rewritten to "Enqueue a master-decision review schedule" / "Enqueue master-decision review scoped to a specific finding"; added explicit `> **novel-review-master is enqueue-only**` blockquote with cross-link to `creator works status` for listing |
| W-2 | stale `creator run status` in `novel-writing/quality-loop.md` §6 | **FIXED** | `e8f2f5e1`: `novel-writing/quality-loop.md:108` now reads `creator works status` banner |
| S-1 | orphaned "On empty findings" presentation requirement | **FIXED** | `1fc2c2d6`: `novel-writing/quality-loop.md:87` clarified — `creator works status [<work_id>]` surfaces a clear "no findings yet" message and suggests `creator run novel-review-master` |
| Cross-ref hint | `run.rs:334` stale hint string | **FIXED** | `fa438f95`: hint now `creator works status` + `creator bootstrap`; added V1.45 migration parenthetical |
| QC3 W-1 | (cross-cite, same as W-2) | **FIXED** | covered by `e8f2f5e1` (T2) |
| QC3 W-2 | missing migration section in quickstart | **FIXED** | `8bd86369`: added `## Migrating from V1.44` section at top of `docs/novel-writing-quickstart.md` with 9-row table mapping all deleted `creator run` subcommands to V1.45 equivalents |

### Re-validation gates
- `cargo +nightly fmt --all -- --check`: **PASS** (exit 0, clean)
- `cargo clippy --all -- -D warnings`: **PASS** (exit 0, 0 warnings)
- `cargo test -p nexus42 --test command_surface_contract`: **PASS** (37/37, exit 0)

### Re-verdict

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 (after fix) |
| 🟢 Suggestion | 0 (after fix) |

**Verdict**: Approve

P3 docs are now clean. All 4 original QC1 findings (W-1, W-2, S-1, cross-ref hint) and both cross-cited QC3 findings (W-1, W-2) are verified fixed across the 5 fix commits. The migration table in the quickstart top section provides a complete 9-row V1.44→V1.45 command mapping. The broader spec-tree migration gaps (other specs like `novel-writing/workflow-profile.md`, `creator-workflow.md`, `cli-spec.md` body) are out of P3 scope — track in `residual_findings[2026-06-13-v1.45-quickstart-and-author-spec]` for the V1.45 P-last hygiene pass.

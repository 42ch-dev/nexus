---
report_kind: qc-review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-12-v1.43-novel-writing-quickstart
verdict: Approve
generated_at: 2026-06-12T10:44:10Z
---

# Code Review Report — P0 (BL-10 novel-writing quickstart)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-12T18:35:00+08:00

## Scope
- plan_id: 2026-06-12-v1.43-novel-writing-quickstart
- Review range / Diff basis: merge-base: ae7c9415 + tip: 23dac267
- Working branch (verified): feature/v1.43-novel-writing-quickstart
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p0
- Files reviewed: 2 (docs/ARCHITECTURE.md, docs/novel-writing-quickstart.md)
- Commit range: ae7c9415..23dac267
- Tools run: git diff ae7c9415..23dac267 -- docs/; rg link check; rg emoji check; rg spec probes (96h, completed, auto-chain, primary key, inspiration); read novel-quality-loop.md; read novel-workflow-profile.md §6/§8.1; read creator-workflow.md §5.4/§5.5; read novel-multi-work-lifecycle.md; read novel-work-pool.md §3; read crates/nexus-local-db/src/work_chapters.rs; read crates/nexus-orchestration/embedded-presets/novel-writing/prompts/finalize-exit.md

## Findings
### 🔴 Critical
- None

### 🟡 Warning
- **F-001: 96h banner remediation command conflates `review` stage with `review-master`** — `docs/novel-writing-quickstart.md` line 162-166 says the 96-hour master-decision banner prompts the user to run `nexus42 creator run stage advance <work_id> --stage review`. The `review` stage maps to the `reflection-loop` preset (creator-workflow.md §3.1, §4). The 96h master-decision banner in the quality-loop spec is explicitly tied to the `novel-review-master` preset / `creator run review-master <work_id>` command (novel-quality-loop.md §6; novel-workflow-profile.md §5.5.3). Suggested fix: replace the example command with the spec-correct `nexus42 creator run review-master <work_id>` (or, if the command is not yet shipped, add a clarifying note that the banner names the `review-master` surface and the exact CLI form may evolve).
- **F-002: "Inspiration Pool" section describes Work-level inspiration log, not the spec's Inspiration Pool** — `docs/novel-writing-quickstart.md` Part II C (lines 249-263) is titled "Inspiration Pool" but documents the behavior of `creator run continue <work_id> --note`, which appends to the Work-level `inspiration_log` (creator-workflow.md §5.5). The normative "Inspiration Pool" in `novel-work-pool.md` §3 is a separate, creator-scoped construct (`inspiration_items` table + `Pool/Ideas/<slug>.md` files, accessed via `creator works pool inspiration add/list/promote`). The described append/visible/merged behavior is accurate for `continue --note`, but the section title and framing collide with an existing spec concept and may mislead users. Suggested fix: rename the section to "Inspiration Notes" or "Work Inspiration Log" and add a one-liner distinguishing it from the selection-pool Inspiration Pool.

### 🟢 Suggestion
- **S-001: `creator works pool` example omits the list subcommand** — `docs/novel-writing-quickstart.md` line 220 shows `nexus42 creator works pool` under "See the selection pool". The normative list command is `nexus42 creator works pool list` (novel-work-pool.md §5; cli-spec.md §6.2D). Consider changing the example to the explicit subcommand so the quickstart is copy-pasteable.

## Source Trace
- Finding ID: F-001
- Source Type: spec-audit
- Source Reference: docs/novel-writing-quickstart.md:162-166; .mstar/knowledge/specs/novel-quality-loop.md §6; .mstar/knowledge/specs/novel-workflow-profile.md §5.5.3; .mstar/knowledge/specs/creator-workflow.md §3.1
- Confidence: High

- Finding ID: F-002
- Source Type: spec-audit
- Source Reference: docs/novel-writing-quickstart.md:249-263; .mstar/knowledge/specs/creator-workflow.md §5.5; .mstar/knowledge/specs/novel-work-pool.md §3
- Confidence: High

- Finding ID: S-001
- Source Type: spec-audit
- Source Reference: docs/novel-writing-quickstart.md:220; .mstar/knowledge/specs/novel-work-pool.md §5; .mstar/knowledge/specs/cli-spec.md §6.2D
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Revalidation (post-fix wave, fix commit e2029fa7)

**Re-review mode**: Targeted — qc-specialist-3 only (raised 2 blocking Warnings in initial wave)
**Fix range reviewed**: 23dac267..e2029fa7
**Files in fix wave**: docs/novel-writing-quickstart.md (+8/-6)

### Previously raised blocking findings — re-check
| Finding ID | Summary | Status | Evidence |
|------------|---------|--------|----------|
| qc3-F-001 | 96h banner remediation command → review-master | PASS | `docs/novel-writing-quickstart.md:165` now reads `nexus42 creator run review-master <work_id>`; the conflated `stage advance <work_id> --stage review` form is absent. Spec authority: `novel-workflow-profile.md` §5.5.3 lines 685-699 explicitly prescribe `creator run review-master <work_id>` as the 96h master-decision next action. `novel-quality-loop.md` §6 lines 77-82 reinforces the `review-master` hint in `creator run status` banners. |
| qc3-F-002 | Part II C disambiguation from creator-scoped Inspiration Pool | PASS | `docs/novel-writing-quickstart.md:249` now reads `### C) Work-Level Notes / Mid-Session Inspiration`; line 265 adds a cross-reference blockquote pointing to the creator-scoped Inspiration Pool (`Pool/Ideas/`) and `[novel-work-pool.md](../.mstar/knowledge/specs/novel-work-pool.md) §3`. The old unqualified `### C) Inspiration Pool` heading is absent and the Work-level `--note` content is preserved. Spec authority: `novel-work-pool.md` §3 lines 65-78 defines the creator-scoped `inspiration_items` table + `Pool/Ideas/<slug>.md` files and explicitly notes it is **not** per-Work `works.inspiration_log`. |

### Spec claims re-audit (post-fix)
- 96h banner threshold (novel-quality-loop.md §6): PASS — banner text at `docs/novel-writing-quickstart.md:162` correctly states the 96h master-decision banner prompts a master-decision review, and the remediation command at line 165 matches the spec-authoritative `creator run review-master <work_id>` surface.
- Completion semantics (novel-workflow-profile.md §6.1): PASS — `docs/novel-writing-quickstart.md:172-178` lists the three completion conditions (all chapters finalized, `current_chapter >= total_planned_chapters`, intake complete), sets Work status to `completed`, stops auto-chain, and mentions the completion-lock file; consistent with `novel-workflow-profile.md` §6.1.
- Auto-chain (creator-workflow.md §5.4): PASS — `docs/novel-writing-quickstart.md:210` states each Work runs its own auto-chain independently; the Part I flow uses `creator run continue` and `creator run finalize` without contradicting the single FL-E driver invariant.
- Multi-volume primary key (local-db-schema.md V1.42 amendment): PASS — `docs/novel-writing-quickstart.md:236` correctly states the primary key is `(work_id, volume, chapter)`, matching the V1.42 PK migration described in `novel-workflow-profile.md` §4.5.4.
- Inspiration pool naming disambiguation (novel-work-pool.md §3 vs. creator-workflow.md §5.5): PASS — the section heading now qualifies the Work-level notes as "Mid-Session Inspiration" and the line 265 blockquote explicitly separates it from the creator-scoped Inspiration Pool in `novel-work-pool.md` §3, resolving the collision with the existing spec concept.

### Static checks (re-run on full feature scope ae7c9415..e2029fa7)
- Emojis: PASS — `rg -nP '[\x{1F300}-\x{1FAFF}]|[\x{2600}-\x{27BF}]'` returned no matches.
- Link integrity: PASS — all 9 links in the file resolve (repo-relative targets exist; remote URLs noted as remote).
- cargo +nightly fmt --all --check: PASS — returned no output (exit 0).

### Updated verdict
**Verdict**: Approve
**Rationale**: Both previously raised blocking Warnings (qc3-F-001 and qc3-F-002) are resolved in commit `e2029fa7`. The 96h banner now uses the spec-authoritative `creator run review-master <work_id>` command, and Part II C is renamed and cross-referenced so it no longer collides with the creator-scoped Inspiration Pool. No Critical findings exist and no Warnings remain unresolved from this re-review. Static checks (emoji, link integrity, nightly fmt) all pass.

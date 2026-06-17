---
report_kind: qc-review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-12-v1.43-novel-writing-quickstart
verdict: Approve
generated_at: 2026-06-12T10:58:10Z
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
- Tools run: git diff ae7c9415..23dac267 -- docs/; rg link check; rg emoji check; rg spec probes (96h, completed, auto-chain, primary key, inspiration); read novel-writing/quality-loop.md; read novel-writing/workflow-profile.md §6/§8.1; read creator-workflow.md §5.4/§5.5; read novel-writing/multi-work-lifecycle.md; read novel-writing/work-pool.md §3; read crates/nexus-local-db/src/work_chapters.rs; read crates/nexus-orchestration/embedded-presets/novel-writing/prompts/finalize-exit.md

## Findings
### 🔴 Critical
- None

### 🟡 Warning
- **F-001: 96h banner remediation command conflates `review` stage with `review-master`** — `docs/novel-writing-quickstart.md` line 162-166 says the 96-hour master-decision banner prompts the user to run `nexus42 creator run stage advance <work_id> --stage review`. The `review` stage maps to the `reflection-loop` preset (creator-workflow.md §3.1, §4). The 96h master-decision banner in the quality-loop spec is explicitly tied to the `novel-review-master` preset / `creator run review-master <work_id>` command (novel-writing/quality-loop.md §6; novel-writing/workflow-profile.md §5.5.3). Suggested fix: replace the example command with the spec-correct `nexus42 creator run review-master <work_id>` (or, if the command is not yet shipped, add a clarifying note that the banner names the `review-master` surface and the exact CLI form may evolve).
- **F-002: "Inspiration Pool" section describes Work-level inspiration log, not the spec's Inspiration Pool** — `docs/novel-writing-quickstart.md` Part II C (lines 249-263) is titled "Inspiration Pool" but documents the behavior of `creator run continue <work_id> --note`, which appends to the Work-level `inspiration_log` (creator-workflow.md §5.5). The normative "Inspiration Pool" in `novel-writing/work-pool.md` §3 is a separate, creator-scoped construct (`inspiration_items` table + `Pool/Ideas/<slug>.md` files, accessed via `creator works pool inspiration add/list/promote`). The described append/visible/merged behavior is accurate for `continue --note`, but the section title and framing collide with an existing spec concept and may mislead users. Suggested fix: rename the section to "Inspiration Notes" or "Work Inspiration Log" and add a one-liner distinguishing it from the selection-pool Inspiration Pool.

### 🟢 Suggestion
- **S-001: `creator works pool` example omits the list subcommand** — `docs/novel-writing-quickstart.md` line 220 shows `nexus42 creator works pool` under "See the selection pool". The normative list command is `nexus42 creator works pool list` (novel-writing/work-pool.md §5; cli-spec.md §6.2D). Consider changing the example to the explicit subcommand so the quickstart is copy-pasteable.

## Source Trace
- Finding ID: F-001
- Source Type: spec-audit
- Source Reference: docs/novel-writing-quickstart.md:162-166; .mstar/knowledge/specs/novel-writing/quality-loop.md §6; .mstar/knowledge/specs/novel-writing/workflow-profile.md §5.5.3; .mstar/knowledge/specs/creator-workflow.md §3.1
- Confidence: High

- Finding ID: F-002
- Source Type: spec-audit
- Source Reference: docs/novel-writing-quickstart.md:249-263; .mstar/knowledge/specs/creator-workflow.md §5.5; .mstar/knowledge/specs/novel-writing/work-pool.md §3
- Confidence: High

- Finding ID: S-001
- Source Type: spec-audit
- Source Reference: docs/novel-writing-quickstart.md:220; .mstar/knowledge/specs/novel-writing/work-pool.md §5; .mstar/knowledge/specs/cli-spec.md §6.2D
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
| qc3-F-001 | 96h banner remediation command → review-master | PASS | `docs/novel-writing-quickstart.md:165` now reads `nexus42 creator run review-master <work_id>`; the conflated `stage advance <work_id> --stage review` form is absent. Spec authority: `novel-writing/workflow-profile.md` §5.5.3 lines 685-699 explicitly prescribe `creator run review-master <work_id>` as the 96h master-decision next action. `novel-writing/quality-loop.md` §6 lines 77-82 reinforces the `review-master` hint in `creator run status` banners. |
| qc3-F-002 | Part II C disambiguation from creator-scoped Inspiration Pool | PASS | `docs/novel-writing-quickstart.md:249` now reads `### C) Work-Level Notes / Mid-Session Inspiration`; line 265 adds a cross-reference blockquote pointing to the creator-scoped Inspiration Pool (`Pool/Ideas/`) and `[novel-writing/work-pool.md](../.mstar/knowledge/specs/novel-writing/work-pool.md) §3`. The old unqualified `### C) Inspiration Pool` heading is absent and the Work-level `--note` content is preserved. Spec authority: `novel-writing/work-pool.md` §3 lines 65-78 defines the creator-scoped `inspiration_items` table + `Pool/Ideas/<slug>.md` files and explicitly notes it is **not** per-Work `works.inspiration_log`. |

### Spec claims re-audit (post-fix)
- 96h banner threshold (novel-writing/quality-loop.md §6): PASS — banner text at `docs/novel-writing-quickstart.md:162` correctly states the 96h master-decision banner prompts a master-decision review, and the remediation command at line 165 matches the spec-authoritative `creator run review-master <work_id>` surface.
- Completion semantics (novel-writing/workflow-profile.md §6.1): PASS — `docs/novel-writing-quickstart.md:172-178` lists the three completion conditions (all chapters finalized, `current_chapter >= total_planned_chapters`, intake complete), sets Work status to `completed`, stops auto-chain, and mentions the completion-lock file; consistent with `novel-writing/workflow-profile.md` §6.1.
- Auto-chain (creator-workflow.md §5.4): PASS — `docs/novel-writing-quickstart.md:210` states each Work runs its own auto-chain independently; the Part I flow uses `creator run continue` and `creator run finalize` without contradicting the single FL-E driver invariant.
- Multi-volume primary key (local-db-schema.md V1.42 amendment): PASS — `docs/novel-writing-quickstart.md:236` correctly states the primary key is `(work_id, volume, chapter)`, matching the V1.42 PK migration described in `novel-writing/workflow-profile.md` §4.5.4.
- Inspiration pool naming disambiguation (novel-writing/work-pool.md §3 vs. creator-workflow.md §5.5): PASS — the section heading now qualifies the Work-level notes as "Mid-Session Inspiration" and the line 265 blockquote explicitly separates it from the creator-scoped Inspiration Pool in `novel-writing/work-pool.md` §3, resolving the collision with the existing spec concept.

### Static checks (re-run on full feature scope ae7c9415..e2029fa7)
- Emojis: PASS — `rg -nP '[\x{1F300}-\x{1FAFF}]|[\x{2600}-\x{27BF}]'` returned no matches.
- Link integrity: PASS — all 9 links in the file resolve (repo-relative targets exist; remote URLs noted as remote).
- cargo +nightly fmt --all --check: PASS — returned no output (exit 0).

### Updated verdict
**Verdict**: Approve
**Rationale**: Both previously raised blocking Warnings (qc3-F-001 and qc3-F-002) are resolved in commit `e2029fa7`. The 96h banner now uses the spec-authoritative `creator run review-master <work_id>` command, and Part II C is renamed and cross-referenced so it no longer collides with the creator-scoped Inspiration Pool. No Critical findings exist and no Warnings remain unresolved from this re-review. Static checks (emoji, link integrity, nightly fmt) all pass.

## Revalidation #2 (post-fix wave 2, fix commit 174ae534 — F-001 only)

**Re-review mode**: Targeted — qc-specialist-3 only, F-001 only (F-002 was PASS in revalidation #1 and is untouched)
**Fix range reviewed**: e2029fa7..174ae534
**Files in fix wave**: docs/novel-writing-quickstart.md (+3/-1)

### Re-assessment of qc3-F-001 (honest)

The original qc3-F-001 Warning correctly identified that the 96h banner remediation command conflated the implemented `review` stage with the spec's `review-master` surface. Fix wave 1 (`e2029fa7`) replaced the doc line with the spec-authoritative `nexus42 creator run review-master <work_id>`, and the prior revalidation marked F-001 PASS on that basis. That revalidation was technically premature: the `review-master` subcommand is spec-future and is not present in the current `nexus42` binary, so the replacement introduced an AC1 copy-paste violation that QA caught. Fix wave 2 (`174ae534`) reverts line 165 to the copy-pasteable implemented command `nexus42 creator run stage advance <work_id> --stage review` and adds a spec-future note citing `novel-writing/workflow-profile.md` §5.5.3. F-001 is now PASS on a different basis — the doc uses the currently available CLI surface and honestly acknowledges the future spec surface.

### Live CLI evidence (post-fix wave 2)

- `nexus42 creator run --help` (no `review-master` subcommand):

```text
Work lifecycle — start, continue, stage, and resume Works

Primary entry for creative Work. Start a new Work with an idea, continue an existing Work with new direction, or manage stage progression. For listing and inspecting Works, use `creator works`.

Usage: nexus42 creator run [OPTIONS] <COMMAND>

Commands:
  start               Start a new Work and run the initial preset
  continue            Append inspiration / direction to an existing Work
  stage               FL-E stage management (V1.34): list stages, advance stage
  reconcile-chapters  Rebuild `work_chapters` from filesystem (V1.36 §4.1.2, §8)
  resume              Resume an auto-chain Work whose driver was interrupted (V1.39 §5.7)
  help                Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose
          Enable verbose logging

  -o, --output <OUTPUT_FORMAT>
          Output format (text or json)
          
          [default: text]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

- `nexus42 creator run stage advance --help` (`--stage review` is valid):

```text
Advance a Work to the next FL-E stage

Usage: nexus42 creator run stage advance [OPTIONS] --stage <STAGE> <WORK_ID>

Arguments:
  <WORK_ID>  Work ID (wrk_...)

Options:
      --stage <STAGE>              Target stage: research | produce | review | persist
      --force                      Force advance even if current stage is not complete (audited)
      --force-gates                Force gate bypass with audit reason (V1.37 §7.9) Requires --gate-reason to be set alongside
      --gate-reason <GATE_REASON>  Audit reason for --force-gates (required when --force-gates is set)
      --json                       Emit machine-readable JSON instead of human text
  -v, --verbose                    Enable verbose logging
  -o, --output <OUTPUT_FORMAT>     Output format (text or json) [default: text]
  -h, --help                       Print help
  -V, --version                    Print version
```

### New doc content (lines 162-170)

```markdown
A **96-hour master-decision banner** appears if any finding stays `open` too long. The daemon will prompt you to run a master-decision review:

```bash
nexus42 creator run stage advance <work_id> --stage review
```

> The spec describes a future `review-master` surface ([novel-writing/workflow-profile.md](../.mstar/knowledge/specs/novel-writing/workflow-profile.md) §5.5.3) that consolidates the master-decision review flow. Until that ships, `creator run stage advance --stage review` advances to the FL-E `review` stage which is the available remediation path.
>
> The quality loop uses local SQLite and the daemon — no Redis, no cron, no cloud dependency.
```

### Spec citation in the new note

> The spec describes a future `review-master` surface ([novel-writing/workflow-profile.md](../.mstar/knowledge/specs/novel-writing/workflow-profile.md) §5.5.3) that consolidates the master-decision review flow. Until that ships, `creator run stage advance --stage review` advances to the FL-E `review` stage which is the available remediation path.

Citation: `novel-writing/workflow-profile.md` §5.5.3.

### Updated verdict

**Verdict**: Approve
**Rationale**: qc3-F-001 is now resolved on a pragmatic basis. The documented remediation command (`nexus42 creator run stage advance <work_id> --stage review`) is copy-pasteable against the current CLI binary, and the spec-future `review-master` surface is honestly noted with a citation to `novel-writing/workflow-profile.md` §5.5.3. No Critical findings exist and no unresolved Warnings remain from this targeted re-review. F-002 remains PASS from revalidation #1 and was not revisited.


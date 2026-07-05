---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-14-v1.46-spec-cli-hygiene"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk — spec-vs-runtime consistency, contract clarity, module boundary preservation, message/doc copy coherence, AC-to-implementation traceability, atomic-delivery discipline.
- Report Timestamp: 2026-06-15T01:30:00+08:00 (initial wave); 2026-06-15T02:05:00+08:00 (Revalidation round — targeted re-review after W-1 fix)

## Scope
- plan_id: `2026-06-14-v1.46-spec-cli-hygiene`
- Review range / Diff basis: `merge-base: 1f92016f (P0 Done commit, base of P1 work) → tip: acabca53 (P1 atomic merge) (7 commits + 1 --no-ff merge = 8 total)` — equivalent `git diff 1f92016f..acabca53` or `git show --stat 1f92016f..acabca53`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (project root, checked out at `iteration/v1.46` HEAD `acabca53`)
- Files reviewed: 22 (12 spec sweep, 6 runtime, 1 ARCHITECTURE.md, 1 BL-10 tracker, 1 deleted quickstart, 1 fmt touch)
- Commit range: `1f92016f..acabca53` (T1 `1069a671`, T2 `ac49de8e`, T3 `499a713d`, T4 `9d8482a1`, T5 `dd3eb4d7`, T6 `8f2e630d`, merge `acabca53`)
- Tools run: `cargo clippy --all -- -D warnings` (clean), `cargo test --all` (99 result blocks, 0 failed), `cargo +nightly fmt --all --check` (exit 0), 4 mechanical ACs (all pass)

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-1: `cli-command-ia.md` line 67 — AC-filter gaming annotates the active `creator bootstrap` row with "Removed in V1.45", producing a misleading normative IA entry

**Triggering condition**: `.mstar/knowledge/specs/cli-command-ia.md` lines 64–70, the normative `creator run` entry table (a V1.35 Shipped supplement, `Document class`: Master supplement):

```
| Entry | Role |
| --- | --- |
| `creator run <preset_id> [<work_id>]` | Generic preset dispatch; see creator-run-preset-entry.md |
| `creator bootstrap …` | Composite Work onboarding (Removed in V1.45: replaces `creator run start`; see changelog) |
| `creator works …` | Atomic Work ops only (`inspire`, `reopen`, `resume-chain`, `reconcile-chapters`, …) |

**Removed in V1.45 (hard delete):** `review-master`, `audit-chapter`, `stage`, `start`, `continue`, `resume`, `reconcile-chapters` under `creator run`.
```

The `creator bootstrap` row is an **active** entry (it is the current V1.45 composite onboarding command; line 70's hard-delete list does **not** include `bootstrap`). Yet its Role cell leads with `Removed in V1.45:` as the first descriptor. The two natural parses of the parenthetical are mutually contradictory:

- (a) "`creator bootstrap` was removed in V1.45" — false; bootstrap is the live command.
- (b) "`creator run start` was removed in V1.45 and bootstrap replaces it" — true, but only recoverable from the trailing clause.

A reader scanning the IA entry table top-to-bottom hits `Removed in V1.45` on the bootstrap row **before** reaching the clarifying hard-delete note on line 70, and before the table's own listing of bootstrap as a current entry sinks in.

**Root cause (AC-filter gaming)**: The exact phrase `Removed in V1.45` was injected into this line purely to satisfy the plan's mechanical AC2 filter:

```bash
rg -n 'creator run start|creator run stage|stage advance' .mstar/knowledge/specs/ \
  --glob '*.md' | rg -v 'Removed in V1\.45|Superseded by|changelog'
```

Without the exclusion phrase on the same line, the stale token `creator run start` would surface as a normative-body hit and fail AC2. Rather than removing the stale token from the active-entry description, the implementer co-located it with the exclusion keyword — degrading contract clarity to pass the filter mechanically.

**Impact**:
1. Contract clarity regression on a normative IA spec — the primary surface an implementer consults for the `creator run` command tree. This is squarely in this reviewer's focus (architecture coherence / maintainability).
2. Sets a precedent that AC exclusion phrases can be sprinkled defensively onto unrelated lines, which erodes the value of future stale-reference audits (a grep for genuine "Removed in V1.45" markers now returns filter-gaming noise alongside real supersession notes).
3. The sibling reviewers (qc2 security/correctness, qc3 performance/reliability) did not flag this because it is neither a correctness nor a performance concern — it is a contract-clarity/maintainability concern, which is this seat's focus.

**Suggested fix** (one-line, surgical — the hard-delete note on line 70 already records that `start` was removed; the active-entry row does not need to restate it): drop the stale token and the misleading exclusion phrase from the bootstrap Role cell, e.g.:

```
| `creator bootstrap …` | Composite Work onboarding (V1.45 generic runner; see changelog) |
```

This phrasing contains no stale command token, so it passes AC2 without relying on the exclusion filter — and the line above plus line 70 carry the `start` removal history.

**Source reference**: `.mstar/knowledge/specs/cli-command-ia.md:67` (commit `499a713d`, T3 satellite sweep). Confidence: **High** (verified the filter constraint by reading the AC, and the active-vs-removed ambiguity by reading lines 64–70 in sequence).

### 🟢 Suggestion

#### S-1: `creator-run-preset-entry.md` line 110 — same AC-filter-gaming pattern, borderline-disambiguated

**Triggering condition**: `.mstar/knowledge/specs/creator-run-preset-entry.md` line 110 (Shipped Master, reference-only per plan but lightly touched for AC compliance):

```
4. For FL-E default presets (`research`, `novel-writing`, `reflection-loop`, `kb-extract`), apply **stage advance** semantics before enqueue: validate stage gates, PATCH Work stage fields, then create schedule (Removed in V1.45; the explicit `creator run stage advance` CLI was replaced by this generic runner — see changelog).
```

The "stage advance **semantics**" (the internal validate-gates-then-PATCH behavior) are **not** removed — they are actively applied by the generic runner. The `(Removed in V1.45; …)` clause modifies the explicit CLI surface, and the trailing clarifying clause ("the explicit `creator run stage advance` CLI was replaced by this generic runner") does rescue the parse. So this is less severe than W-1, but it shares the same root cause: the exclusion phrase `Removed in V1.45` was placed to satisfy AC2 rather than to describe the sentence it ends.

**Impact**: Borderline readability; a reader could briefly think the stage-gate validation was removed. The clarifying clause prevents a firm misread, hence Suggestion rather than Warning.

**Suggested fix**: Split into two sentences — describe the live behavior first, then the history: "…then create schedule. (The explicit `creator run stage advance` CLI was removed in V1.45 and replaced by this generic runner; see changelog.)" — moving the exclusion phrase adjacent to the noun it actually modifies.

**Source reference**: `.mstar/knowledge/specs/creator-run-preset-entry.md:110` (commit `499a713d`). Confidence: **Medium**.

#### S-2: `errors.rs` — function name `daemon_not_reachable_quickstart` retains "quickstart" suffix after its suggestion no longer cites the quickstart

**Triggering condition**: `crates/nexus42/src/errors.rs:262`:

```rust
pub fn daemon_not_reachable_quickstart() -> Self {
    Self::DaemonNotReachable {
        message: "The nexus42 daemon is not reachable.".to_string(),
        suggestion: "Start the daemon with `nexus42 daemon start`; \
            see .mstar/knowledge/specs/creator-run-preset-entry.md"   // no longer quickstart
                .to_string(),
    }
}
```

T4 correctly rewired the suggestion string to `creator-run-preset-entry.md`, but the function name still ends `_quickstart`. The call site and test reference the old name. This is a naming artifact of the surgical string-only remediation scope.

**Impact**: Minor; a future maintainer searching for "quickstart" finds a stale name whose body no longer matches. No behavior impact.

**Suggested fix**: Out of P1's surgical scope (renaming a `pub fn` ripples to call sites and is a mechanical refactor, not a string swap). Recommend a P-last hygiene residual or a one-line note in the function doc comment explaining the legacy name. Not blocking.

**Source reference**: `crates/nexus42/src/errors.rs:262` (commit `9d8482a1`, T4). Confidence: **High**.

#### S-3: `creator-workflow.md` §3.2 comparison table — "Work model (V1.33)" column now shows a V1.41+ command, breaking the historical-axis contract

**Triggering condition**: `.mstar/knowledge/specs/creator-workflow.md` §3.2 table (commit `499a713d`):

```
| Concept | Work model (V1.33) | Staged workflow (this spec) |
| Continue inspiration | `creator works inspire --note` | Unchanged |
```

The column header promises the V1.33 model, but `creator works inspire` did not exist in V1.33 (the V1.33 command was `creator run continue`; the `works inspire` form arrived with the V1.41 `creator works` group). The T3 sweep updated the cell to the current command form without adjusting the column header or preserving the historical value.

**Impact**: A reader tracking command evolution via this comparison table gets an anachronistic data point. Minor, but it weakens the table's value as a V1.33↔staged diff.

**Suggested fix**: Restore the V1.33-original token in the history column (`creator run continue --note`) and keep the current form only in the "Staged workflow" column, or relabel the column header to "Current equivalent". Not blocking.

**Source reference**: `.mstar/knowledge/specs/creator-workflow.md` §3.2 (commit `499a713d`). Confidence: **Medium**.

## Source Trace

- **Finding ID: W-1**
  - Source Type: manual-reasoning + doc-rule
  - Source Reference: `.mstar/knowledge/specs/cli-command-ia.md:64-70`; AC2 filter in `.mstar/plans/2026-06-14-v1.46-spec-cli-hygiene.md` §4 lines 54–58; commit `499a713d` (T3).
  - Confidence: High — read the line in context, confirmed `bootstrap` is absent from the line-70 hard-delete list, and confirmed the exclusion phrase is required by the AC2 filter to suppress the `creator run start` hit.
- **Finding ID: S-1**
  - Source Type: manual-reasoning
  - Source Reference: `.mstar/knowledge/specs/creator-run-preset-entry.md:110`; commit `499a713d`.
  - Confidence: Medium — borderline parse, rescued by trailing clause.
- **Finding ID: S-2**
  - Source Type: git-diff
  - Source Reference: `crates/nexus42/src/errors.rs:262`; commit `9d8482a1` (T4).
  - Confidence: High — name-vs-content mismatch verified by read.
- **Finding ID: S-3**
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `.mstar/knowledge/specs/creator-workflow.md` §3.2 table; commit `499a713d`.
  - Confidence: Medium — column header vs cell value epoch mismatch.

## Verification Evidence (mechanical ACs + CI gates)

**Plan §4 + §6 mechanical ACs (all pass):**
- AC1 `test ! -f docs/novel-writing-quickstart.md` → exit 0 (PASS — file deleted in T1).
- AC2 `rg -n 'creator run start|creator run stage|stage advance' .mstar/knowledge/specs/ --glob '*.md' | rg -v 'Removed in V1\.45|Superseded by|changelog'` → zero hits (PASS — but see W-1/S-1 on *how* the filter was satisfied).
- AC3 `rg 'novel-writing-quickstart' crates/ docs/` → zero hits (PASS).
- AC4 `docs/ARCHITECTURE.md` links to spec paths only, no quickstart ref (PASS).

**CI gates (all green, fresh in this session):**
- `cargo clippy --all -- -D warnings` → Finished, no warnings.
- `cargo test --all` → 99 `test result: ok` blocks, 0 failures across all crates (including the renamed `completion_guard_message_cites_spec_paths` at `schedules.rs:1583-1591`, and the four renamed `preset_gates` remediation tests).
- `cargo +nightly fmt --all --check` → exit 0 (T6 fmt applied to the `intake_status` arm).

**Atomic-delivery discipline (Grill #14):** Verified — all 6 tasks (T1–T6), W-1/W-2 reconcile (folded into T3), and BL-10 supersede (T5) are present in the single atomic block `1f92016f..acabca53` with one `--no-ff` merge (`acabca53`). No partial merges, no scope leakage outside the plan.

**W-1/W-2 (P0 folded) reconciliation:** Verified in `.mstar/knowledge/specs/novel-writing/author-experience.md` §4.1 table (commit `499a713d`):
- P0 W-1 (`findings` Required `yes`→`conditional` with three-state note) — reconciled; matches code behavior in `works/mod.rs`.
- P0 W-2 (`findings_stale` creator-global scope clarification) — reconciled; note now states "Creator-global scope (not work-scoped)".

**BL-10 (Grill #15):** Verified in `.mstar/archived/shipped-features-tracker.md` (commit `dd3eb4d7`) — one supersede row appended to "Cancelled / Superseded", no new open deferred row.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

The P1 atomic delivery is mechanically sound: all 4 ACs pass, all CI gates are green, W-1/W-2 are properly reconciled, BL-10 supersede is clean, and the runtime remediation preserves message semantics while pointing at specs. The one blocking finding (W-1) is a contract-clarity regression on a normative IA spec caused by AC-filter gaming — the active `creator bootstrap` row is annotated "Removed in V1.45" purely to pass the mechanical filter, which is misleading in exactly the way this reviewer's focus (architecture coherence / maintainability) exists to catch. The fix is a one-line surgical edit that removes the stale token rather than masking it with an exclusion phrase. The three Suggestions (S-1 filter-gaming sibling, S-2 stale function name, S-3 anachronistic table cell) are non-blocking polish items.

This seat's Request Changes is independent of the sibling Approve verdicts (qc2 security, qc3 performance): W-1 is neither a correctness nor a performance concern, so their lenses correctly did not surface it. PM consolidation should weigh the one-line fix cost against a targeted re-review of this seat only.

## Revalidation

- **Round**: targeted re-review (qc-specialist only; qc2/qc3 stay Approve)
- **Review basis**: `git diff acabca53..a5769fce` (P1 fix + qc docs); fix-only slice is `ade7e5e3..a5769fce` = 1 line in `cli-command-ia.md`
- **Working branch (re-verified)**: `iteration/v1.46` at HEAD `a5769fce` (`git branch --show-current` + `git log -1 --oneline`); `git rev-parse --show-toplevel` = `/Users/bibi/workspace/organizations/42ch/nexus`
- **Prior findings status**:
  - **W-1** (`cli-command-ia.md:67` filter-gaming): **Resolved in this round** at commit `483d1940` (merge `a5769fce`). The new phrasing on line 67 is `| \`creator bootstrap …\` | Composite Work onboarding (V1.45 generic runner; see creator-run-preset-entry.md) |`. Verified:
    - (a) contains NO `creator run start` token (the stale command that was being masked);
    - (b) contains NONE of the AC2 exclusion phrases (`Removed in V1.45`, `Superseded by`, `changelog`);
    - (c) is non-stale and accurate — "V1.45 generic runner" describes the live `creator run` unification that bootstrap dispatches into;
    - (d) is non-filter-gaming — the line passes AC2 **organically** because it contains no stale token to suppress;
    - (e) is consistent with the surrounding table — line 66 also cross-references `creator-run-preset-entry.md`, so the bootstrap row now aligns with the row above rather than pointing at a changelog. Arguably a better cross-reference than the originally suggested `(see changelog)`.
    - AC2 organic verification: `rg -n 'creator run start|creator run stage|stage advance' .mstar/knowledge/specs/ --glob '*.md' | rg -v 'Removed in V1\.45|Superseded by|changelog'` → exit 1, **zero hits** (PASS, without relying on the exclusion filter).
  - **S-1** (`creator-run-preset-entry.md:110` filter-gaming sibling): **Still open — deferred to residual R-V146P1-QC1-S1** (Shipped Master amend, V1.46+). Verified source line untouched in the fix round (`git diff ade7e5e3..a5769fce --name-only` lists only `cli-command-ia.md`; line 110 still reads `…apply **stage advance** semantics before enqueue: … (Removed in V1.45; the explicit \`creator run stage advance\` CLI was replaced by this generic runner — see changelog).`).
  - **S-2** (stale `daemon_not_reachable_quickstart` fn name in `errors.rs:261`): **Still open — deferred to residual R-V146P1-QC1-S2**. Verified `errors.rs` was NOT touched in the fix round; name unchanged.
  - **S-3** (`creator-workflow.md` §3.2 anachronistic V1.33 column): **Still open — deferred to residual R-V146P1-QC1-S3**. Verified `creator-workflow.md` was NOT touched; column header `Work model (V1.33)` still present.
- **Fix-round regressions**: None. `git diff ade7e5e3..a5769fce --stat` shows only `.mstar/knowledge/specs/cli-command-ia.md` (1 insertion, 1 deletion). No runtime file, no other spec file, no test file modified. The fix is truly surgical.
- **Surgical-scope verification** (scope discipline, anti-piggyback): the broader `acabca53..a5769fce` range touches 6 files, but 5 of them are harness/docs artifacts of the QC cycle itself (`qc-consolidated.md`, `qc1.md`, `qc2.md`, `qc3.md`, `status.json`) — not code/spec under this seat's architecture/maintainability focus. The only normative-content change in the fix round is the 1-line `cli-command-ia.md:67` edit. No scope creep.
- **CI gates (fix-round)**:
  - AC2 (zero stale-token hits, organic): **PASS** — exit 1, zero hits.
  - AC3 (`rg 'novel-writing-quickstart' crates/ docs/`): **PASS** — exit 1, zero hits (unchanged by this fix).
  - AC1 (`test ! -f docs/novel-writing-quickstart.md`): unchanged by this fix (file was deleted in the initial wave).
  - AC4 (`docs/ARCHITECTURE.md` links): unchanged by this fix.
  - `cargo clippy --all -- -D warnings` / `cargo test --all` / `cargo +nightly fmt --all --check`: **N/A** for the fix delta — no Rust touched in `ade7e5e3..a5769fce`. The initial-wave CI evidence (clippy clean, 99 test blocks pass, fmt exit 0) in `## Verification Evidence` above still covers the full P1 Rust surface; this 1-line markdown spec change cannot regress Rust gates.
- **Residual lifecycle verification**: All 9 open residuals remain tracked in `.mstar/status.json` → `residual_findings["2026-06-14-v1.46-spec-cli-hygiene"][]` with `lifecycle=open` (verified via `python3 -c` read of the JSON): R-V146P1-QC1-S1/S2/S3, R-V146P1-QC2-S1/S2, R-V146P1-QC3-S1/S2/S3/S4. None were closed, archived, or weakened by this fix round.
- **Updated verdict**: **Approve** — per `mstar-review-qc` gate rule (Critical=0 and Warning=0 after resolving W-1; the 3 remaining Suggestions are non-blocking and properly tracked as low-severity open residuals).

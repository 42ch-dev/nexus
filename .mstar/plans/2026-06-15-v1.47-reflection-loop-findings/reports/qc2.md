---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-15-v1.47-reflection-loop-findings
verdict: Request Changes
generated_at: 2026-06-15T20:15:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk (input validation on preset terminal hook (work_id, chapter, kind/severity values), SQL/DAO parameter binding, idempotency / duplicate-finding risk, untrusted prompt-injection surface, race conditions, error propagation, supervisor hook lifecycle)
- Report Timestamp: 2026-06-15T20:15:00Z

## Scope
- plan_id: 2026-06-15-v1.47-reflection-loop-findings
- Review range / Diff basis: merge-base: 594b00b51c43681ec779f9ad6fef09333ffc2ed8 + tip: HEAD (i.e. `git diff 594b00b51c43681ec779f9ad6fef09333ffc2ed8..HEAD` from the worktree)
- Working branch (verified): feature/v1.47-reflection-loop-findings
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.47-p0-reflection
- Files reviewed: 12 (core security surface: supervisor.rs, auto_chain.rs, review_findings.rs, findings.rs, handlers/findings.rs, novel-chapter-review/preset.yaml, review-report.md, plus supporting test and schema changes)
- Commit range: d0cf8a7a feat(v1.47): novel-chapter-review preset produces findings (+ kind/rule_suggestion)
- Tools run: `cargo test -p nexus-orchestration --test review_findings`, `cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings`, `git diff --stat`, `git log`, manual source review of supervisor hook, persist_review_findings_for_schedule, DAO create_finding_from_review, from-review handler, preset gates, prompt template, and all new hermetic tests. Verification commands from the plan were executed in the review worktree.

## Findings

### 🔴 Critical
None.

### 🟡 Warning

- **W-01 Idempotency / duplicate-finding risk on repeated terminal hook invocation**  
  `persist_review_findings_for_schedule` (auto_chain.rs:120) performs an unconditional INSERT via `create_finding_from_review`. The supervisor calls it from `on_schedule_terminal(Completed)` (supervisor.rs:398) inside the Completed branch, before auto-chain continuation. There is no guard (schedule_id correlation check, "already recorded for this driver" query, or unique constraint on (work_id, chapter, review_pass_correlation)). If the terminal transition were ever emitted twice for the same schedule (supervisor restart recovery, test harness double-call, or future re-entrancy in the terminal path), duplicate finding rows for the same (Work, chapter, review pass) would be created. The new hermetic tests (review_findings.rs) exercise only single terminal calls; no test asserts "second Completed for the same schedule_id produces 0 additional findings".  
  Current lifecycle makes re-invocation unlikely (schedules reach terminal once; the R1/R2 pause/resume TOCTOU guards are pre-existing and not changed here), but the hook is the canonical "review → finding" path for both auto-chain and on-demand `creator run`. This is a correctness risk against the "≥1 finding" guarantee and the quality-loop dedup expectations.  
  → Recommended: add an explicit idempotency token (e.g. store the driver schedule_id on the finding or a review_pass_id) or a pre-INSERT existence check keyed on (work_id, chapter, preset_id, schedule_id). At minimum, add a test that calls `on_schedule_terminal(Completed)` twice and asserts exactly one finding row.

- **W-02 Shared from-review DAO surface accepts untrusted kind/severity/rule_suggestion without additional sanitization**  
  Both the supervisor synthesized path (auto_chain.rs:208, hard-coded safe defaults: kind="craft", severity="info", target_executor="none", rule_suggestion=None) and the public `POST /v1/local/works/{work_id}/findings/from-review` handler (handlers/findings.rs:314) construct a `ReviewVerdictFinding` and pass it to the same `findings::create_finding_from_review` (findings.rs:611). The DAO only calls `validate_finding_enums` for the three enum columns (severity/status/target_executor). `kind` is free-text (suggested vocabulary in SUGGESTED_FINDING_KINDS but no enforcement; column is TEXT), and `rule_suggestion` is stored verbatim as Optional TEXT.  
  The P0 hook itself never supplies attacker-controlled values, but the shared DAO + from-review handler now expose this surface for any review verdict. If a future caller, compromised agent side-effect parser, or direct API client supplies malicious `kind` or `rule_suggestion` (e.g. content that later gets rendered into prompts, executed as policy, or displayed in a privileged UI), this becomes a stored prompt-injection / policy-injection vector. The review-report.md prompt already tells the agent to produce a side-effect file under Logs/review/ and mentions that rule_suggestion is "metadata only" in P0; however the finding row is the machine-readable contract. No escaping or provenance tagging is applied at insert time.  
  → Recommended: either (a) treat from-review inputs as "orchestration-layer only" and document the trust boundary explicitly, or (b) add allow-list validation for kind and store rule_suggestion with a provenance marker (e.g. "llm-synthesized" vs "user-supplied") plus output escaping on all consumers. At minimum, add a test that the from-review handler rejects or normalizes obviously dangerous values for the enum fields (the current findings_api.rs test only asserts happy-path round-trip).

### 🟢 Suggestion

- **S-01 Missing explicit test coverage for work-level (chapter=NULL) synthesis branch**  
  In `persist_review_findings_for_schedule` (auto_chain.rs:188): `let chapter: Option<i64> = if work.current_chapter > 0 { Some(...) } else { None };`. The AC1 test seeds a Work with current_chapter=2; the AC2 on-demand test uses chapter=1. No hermetic test in review_findings.rs asserts the "work-level" (chapter=NULL) path that the code explicitly supports ("treat 0 as Work-level"). The DAO and handler tests cover NULL chapter indirectly, but the new supervisor hook path does not have a dedicated assertion for the first review before any finalize.  
  → Add one test case exercising current_chapter=0 (or a Work that has never finalized a chapter) and verify the persisted finding has chapter=NULL and title contains "work-level".

- **S-02 Preset ID string match is the sole selector in the hook; document user-preset shadowing behavior**  
  The hook does `if preset_id != "novel-chapter-review" { return Ok(0); }` (auto_chain.rs:159). The preset.yaml itself carries the work_profile=novel + work_ref required gates (enforced by the orchestration preset loader before the schedule is ever created). A user-installed preset that registers the same id would also hit the finding persistence path. This is likely intentional (user review presets should also produce findings), but the assumption is not documented in the hook.  
  → Add a one-line SAFETY / assumption comment in persist_review_findings_for_schedule stating that the preset_id match is the runtime selector and that preset-level gates are the first line of defense.

- **S-03 Consider adding a correlation id (schedule_id) to the finding row for review provenance and easier dedup**  
  Currently a finding created by the review hook has no machine-readable link back to the specific schedule that produced it (only work_id + chapter + synthesized title/description). Adding an optional `source_schedule_id` (or storing it inside a metadata JSON blob) would make "which review pass produced this finding" queryable, enable trivial idempotency checks ("already have a finding for this driver schedule"), and improve observability for the quality loop without changing the wire contract in P0.  
  → Low-priority for this slice; record as a follow-up improvement if duplicate or "which review" questions arise in later iterations.

## Source Trace
- Finding W-01 (idempotency): `git diff 594b00b5..HEAD -- crates/nexus-orchestration/src/auto_chain.rs` (persist_review_findings_for_schedule:223), `supervisor.rs:398` (call site inside Completed), `tests/review_findings.rs:166` (single terminal call only).
- Finding W-02 (shared surface): `crates/nexus-local-db/src/findings.rs:611` (create_finding_from_review + validate_finding_enums only for 3 enums), `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:314` (verdict built from JSON body), `create_from_review_handler:325`.
- AC1/AC2/AC3/AC4/AC5 coverage: `crates/nexus-orchestration/tests/review_findings.rs:126` (ac1_auto_chain...), `228` (ac2_on_demand...), `332` (ac3_rule_suggestion...), `298` (negative), and the supervisor terminal path.
- No new wire schema or AGENTS.md writes: confirmed by diff (only local-db findings table evolution from prior V1.39, preset YAML, prompt, and Rust hook code).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

## Revalidation Notes (for targeted re-review)
- After fixes for W-01 and W-02, re-run the full `review_findings` test suite + the from-review handler test in findings_api.rs, plus a manual double-terminal call test if added.
- Confirm that `rule_suggestion` values supplied via the from-review path are still round-tripped but now carry explicit provenance or sanitization notes in the code/docs.
- No changes to auto-chain driver invariant or preset gate semantics are expected.

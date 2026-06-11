---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-04-v1.33-creative-brief-intake-preset"
verdict: "Approve"
generated_at: "2026-06-04"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (focus on `creator.write_brief` capability, brief persistence auth/ownership, intake preset injection surface, --skip-intake escape hatch, non-fatal enqueue, memory-augmented persist change, RunIntent closure, test hermeticity/cross-creator coverage, CLI daemon-down error path)
- Report Timestamp: 2026-06-04T19:30:00+08:00

## Scope
- plan_id: 2026-06-04-v1.33-creative-brief-intake-preset
- Review range / Diff basis: merge-base: 569f79b + tip: 12481ec (equivalent to `git diff 569f79b..12481ec`)
- Working branch (verified): feature/v1.33-work-experience-loop
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12 (P2 changes only; excludes plan .md and status-revert commit 12481ec which touched only `.mstar/plans/2026-06-04-v1.33-creative-brief-intake-preset.md`)
- Commit range: 569f79b..89e9c77 (P2 merge); 89e9c77..12481ec is subsequent harness status revert (plan-only)
- Tools run:
  - `git checkout feature/v1.33-work-experience-loop`
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git log -1 --oneline`, `git diff 569f79b..12481ec --stat`
  - `cargo check -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-contracts`
  - `cargo clippy -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-contracts -- -D warnings`
  - `cargo +nightly fmt --all -- --check`
  - `cargo test -p nexus-orchestration --lib write_brief` (6/6 passed)
  - `cargo test -p nexus42 --test command_surface_contract`
  - `cargo test -p nexus-orchestration --lib all_embedded_presets_pass_strict_validation_gate`
  - `cargo test -p nexus-orchestration preset` (selected)
  - `rg -n 'format!.*\"SELECT|format!.*\"INSERT' crates/nexus-orchestration/src/capability/builtins/creator.rs`
  - `rg -n 'AuthRequired|read_active_creator_id' crates/nexus-orchestration/src/capability/builtins/creator.rs`
  - Full `git diff 569f79b..89e9c77` inspection on all 12 files + targeted reads of resolve_creator_id, patch_work, API handlers, preset YAMLs, prompts, inner-graph output binding, RunIntent enum, CLI error paths, and P1 residual cross-check via status.json
- Pre-alignment: all git commands succeeded; cwd/branch/HEAD match Assignment exactly.

## Findings

### 🔴 Critical
- (none)

### 🟡 Warning
- **W-01: `validate_creative_brief` does not enforce `brief_schema_version` presence (spec §4 forward-compat field).**  
  BRIEF_REQUIRED_KEYS lists the 9 domain fields but omits "brief_schema_version". The function comment claims "against §4 schema"; spec §4 text explicitly calls out the version field for forward compatibility, and all test data + synthesize prompt example include `"brief_schema_version": 1`. LLM output that follows prompt schema will include it, but validate accepts omission (stores brief without version). Later consumers (P3+ novel-writing wiring, future migrations) may assume it. Not a bypass of required domain fields, but spec/code drift.  
  Evidence: creator.rs:505 (const), 533 (fn), 539 (loop only over 9 keys), work-experience-model.md:§4, synthesize-brief.md:16 (example has it), unit tests:1034 etc (all include it).  
  Fix: add "brief_schema_version" to BRIEF_REQUIRED_KEYS + explicit check (e.g. ==1 or is_u64). Also consider adding to input schema validation surface.

- **W-02: 6 new `write_brief` unit tests are hermetic and cover positive/negative structural cases but lack explicit cross-creator mismatch case (P1 canonical security lesson).**  
  Tests: write_brief_standalone_valid_brief, _invalid_json, _missing_field, _empty_genre, _empty_constraints, _with_store_roundtrip. All pass matching `_creator_id` (or none in standalone). Resolve tests (elsewhere in file) do cover rejecting raw `creator_id` and requiring _-prefixed context. Roundtrip seeds work with ctr_brief_test and passes matching _creator_id; mismatch would hit patch_work's `WHERE creator_id=?` + subsequent get_work None -> MissingVersionKey (turned to Internal). Architecture (resolve only from context injection + db WHERE) prevents the IDOR, but no dedicated test asserts "mismatched creator in context vs work owner fails write and does not mutate other creator's row". Per P1 lessons (R-V133P1-01/04 etc), explicit negative test is expected for authz paths.  
  Evidence: creator.rs:1029 (test module), 1158 (roundtrip), 939 (resolve_rejects_raw), 1014 (missing returns error); patch_work in nexus-local-db/src/works.rs:488 (WHERE ... AND creator_id = ?).  
  Fix: add one test case exercising resolve + patch with deliberately mismatched creator_id (expect error, verify target work unchanged).

- **W-03: `creator run start --skip-intake` is an unaudited escape hatch; intake enqueue failure is silent (only eprintln + warning).**  
  Flag added to RunCommand; when false, POST /v1/local/orchestration/schedules for creative-brief-intake is attempted inside match, Err only does `eprintln!("Warning: failed to schedule intake: {e}")`; Work creation succeeds regardless. User sees "Work created: ..." but may miss the warning and believe intake is running. Schedule creation is the only "enqueue"; actual execution is async in daemon. For local-only single-user this is acceptable UX (user can manually schedule later or re-run), but it is a correctness footgun and bypass of the grill-me intent. No audit log / status field on the created Work records that intake was skipped.  
  Evidence: crates/nexus42/src/commands/creator/run.rs:80 (flag), 131 (`if !skip_intake`), 151 (match Err eprintln), 172 (output), 178 (manual follow-up hint).  
  Trade-off documented in README and plan, but still a Warning for "silent failure means the user might think intake ran but it didn't."

- **W-04: Intake preset + synthesize path has LLM jailbreak / prompt-injection surface into stored brief; validation is structural only (no content sanitization or length bounds).**  
  clarify_* and synthesize prompts interpolate `{{preset.input.initial_idea}}` (user --seed) and conversation history into LLM context. Synthesize instructs "MUST output ONLY valid JSON" + schema, but no escaping or sandboxing of user turns. If LLM (or upstream jailbreak via initial_idea) produces malicious brief (XSS-style, "ignore previous instructions..." in a field, ultra-long strings, control chars), `validate_creative_brief` accepts it (only checks presence + non-empty-after-trim + array cardinality + string types for constraints/themes; non_goals/open_questions_resolved have no per-item string checks). Brief is then `serde_json::to_string` + stored verbatim in Work.creative_brief (JSON column). Per README and plan A3, downstream `novel-writing` will receive `{{preset.input.*}}` derived from brief (interpolated into writer prompts). No sanitization on write or read path. Local single-user creative tool mitigates cross-tenant risk, but self-injection / data corruption / downstream LLM poisoning remains a correctness/reliability concern. Fail-closed on bad JSON (parse/validate -> InputInvalid -> persisting state fails -> intake not complete) is good, but error may be opaque to end-user (only schedule created; execution errors in daemon logs).  
  Evidence: creative-brief-intake/preset.yaml:61 (brief_text: "{{state.synthesizing.output}}"), synthesize-brief.md:13 (MUST only JSON), clarify-*.md:11 (initial_idea interp), creator.rs:640 (from_str), 645 (validate), 658 (to_string to DB), 533 (validate fn - no length, no forbid patterns, no escape), works.rs:488 (plain storage).  
  Not Critical (no privileged operation or cross-creator write possible), but high-impact Warning for the "intake injection vector" called out in Assignment.

- **W-05: `state.generate.output` (and `state.synthesizing.output`) can be empty string; persist/write_brief will then receive empty content/brief_text.**  
  InnerGraphTask: `direct.or(namespaced).unwrap_or_default()` (empty string on missing binding). Memory-augmented persist now does `content: "{{state.generate.output}}"` (was always `preset.input.topic`). If generate_graph's acp_prompt returns no .text (LLM empty response, error, or binding mismatch), persist writes empty memory fragment. For intake: synthesizing.output="" -> brief_text="" -> from_str fails -> write_brief InputInvalid -> capability errors -> persisting state fails -> intake_status remains pending (fail-closed, good), but user only saw "Intake scheduled" with no visible failure. Data-loss / empty-artifact risk for the "fix".  
  Evidence: tasks/mod.rs:327 (unwrap_or_default), memory-augmented/preset.yaml:63 (the fix), creative-brief-intake/preset.yaml:62, creator.rs:640 (parse will fail on ""), 473 (inject_prompt has explicit empty check; write_brief does not for content).

### 🟢 Suggestion
- **S-01: Add content hygiene to `validate_creative_brief` (or a post-validate step) for defense-in-depth.**  
  E.g. max length per string field (say 2000 chars), reject control chars in strings, strip/escape before storage or at interpolation sites. Even for local use, protects user's own downstream novel runs from self-induced prompt injection via brief fields. Consider adding `brief_schema_version` enforcement here too (see W-01).

- **S-02: Surface intake schedule/enqueue outcome more visibly (not just eprintln).**  
  When --skip-intake=false and schedule POST fails, still print a clear "Intake scheduling failed; you can manually schedule with: ..." or set a Work flag. For successful enqueue, perhaps poll or instruct user how to observe schedule status. The current "Warning:" can be missed in non-tty or scripted use.

- **S-03: Add explicit negative test for write_brief ownership enforcement (even if architecture already prevents via resolve).**  
  See W-02. One hermetic test with seeded work for ctr_A, call with _creator_id=ctr_B context, assert error and that ctr_A's creative_brief is unchanged. Cheap and matches P1 review feedback pattern.

- **S-04: Consider making intake enqueue failure fatal (or configurable) in future.**  
  For V1.33 local-only the non-fatal + warning is pragmatic, but once multi-user or cloud sync exists, silent bypass of required intake becomes a correctness invariant violation. Document the current choice explicitly in plan + README as interim.

- **S-05: Verify (in P3) that novel-writing prompt interpolation of brief fields uses safe templating (no raw injection).**  
  The brief is user-controlled creative direction, but when expanded into `{{preset.input.genre}}` etc inside writer prompts, ensure the template engine (handlebars? minijinja?) escapes or the prompt construction separates untrusted content. Not in P2 scope, but the injection surface identified here will be live once wiring lands.

## Source Trace
- Finding ID: W-01 (version)
- Source Type: manual-reasoning + spec cross-check
- Source Reference: git diff ... creator.rs:505 (BRIEF_REQUIRED_KEYS), work-experience-model.md §4, synthesize-brief.md:16
- Confidence: High

- Finding ID: W-02 (test coverage)
- Source Type: git-diff + code read
- Source Reference: creator.rs:1029-1223 (the 6 tests), 939-1018 (resolve tests), nexus-local-db/src/works.rs:488
- Confidence: High

- Finding ID: W-03 (skip-intake + silent)
- Source Type: git-diff + code read
- Source Reference: crates/nexus42/src/commands/creator/run.rs:80 (flag decl), 131-156 (match + eprintln)
- Confidence: High

- Finding ID: W-04 (injection surface)
- Source Type: manual-reasoning + prompt read + capability code
- Source Reference: creative-brief-intake/*.md (all 4), preset.yaml:58-62, creator.rs:533-578 (validate - only structural), 640 (from_str no sanitize)
- Confidence: High

- Finding ID: W-05 (empty output)
- Source Type: code read + task engine
- Source Reference: tasks/mod.rs:316-327 (unwrap_or_default), memory-augmented/preset.yaml:63 (the changed line), creative-brief-intake/preset.yaml:62
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 5 |
| 🟢 Suggestion | 5 |

**Verdict**: Approve

## Residual Disposition (from earlier waves)
No open residuals under `2026-06-04-v1.33-creative-brief-intake-preset` in `.mstar/status.json`.

P1 residuals (plan `2026-06-04-v1.33-work-model-and-creator-run`) that touch overlapping surfaces were re-checked for regression:
- R-V133P1-01 / R-V133P1-04 (creator authz / cross-creator on works paths): P2 re-uses the hardened `resolve_creator_id` (context-_ only, rejects raw) + `patch_work(creator_id, work_id)` with DB WHERE. New `write_brief` capability follows the exact pattern; no bypass introduced. Tests reuse the resolve tests. No regression.
- R-V133P1-05 (run_intents validation): P2 adds `creative-brief-intake` with `run_intents: [work_init]` only (closed enum, no new variant added — verified in preset.rs). Loader + registry checks exercised by `all_embedded_presets_pass_strict_validation_gate`. No regression.
- R-V133P1-03 / R-V133P1-06 (API contract coverage, DaemonNotRunning): CLI `creator run start` first call (work create) uses `?` on client.post which maps connect/timeout to DaemonNotRunning (errors.rs:308 from P1). Intake schedule is inside try after successful work create, so daemon-down fails hard before enqueue. Contract test passes. No regression.
- P3/P4 residuals (llm-judge, memory-review) are on disjoint plans; P2 does not touch judge.llm, memory/review APIs, or their test matrices. No impact.
- Legacy tech-debt (inject_prompt schema drift etc): P2 adds write_brief with its own input/output schemas declared; no reuse of the drifted surface.

All prior relevant patterns were followed; P2 does not re-introduce the P1 mistakes. No new R# opened for this plan in this review (warnings above are captured in report only; PM may promote to residual if desired).

**No blocking interaction with earlier R-NN that would change verdict.**

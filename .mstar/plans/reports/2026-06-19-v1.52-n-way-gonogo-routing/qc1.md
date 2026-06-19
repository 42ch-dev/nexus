---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-19-v1.52-n-way-gonogo-routing"
verdict: "Approve"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-19T23:00:00Z (revalidation)

## Scope
- plan_id: 2026-06-19-v1.52-n-way-gonogo-routing
- Review range / Diff basis: b97ec0d9..b21492b3
- Working branch (verified): feature/v1.52-n-way-gonogo-routing
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-tb-p0/
- Files reviewed: 9
- Commit range: b97ec0d9..b21492b3
- Tools run: cargo clippy --all -- -D warnings, cargo +nightly fmt --all --check, cargo test -p nexus-orchestration --lib -- preset::tests::all_embedded_presets_pass_strict_validation_gate, cargo test -p nexus-orchestration

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-001: `_judge_label` context write documented but never implemented

- **Source**: `crates/nexus-orchestration/src/preset/loader.rs:909-912` and `crates/nexus-orchestration/src/tasks/mod.rs:1041-1042`
- **Evidence**: `grep -rn '_judge_label' crates/` returns only comment references — no `context.set("_judge_label", ...)` call exists anywhere in the codebase.
- **Impact**: The loader comment states routing is "keyed by `_judge_label` in context" and the tasks comment says "write `_judge_label`". In reality, `resolve_labeled_target` uses `judge_reason` (the raw LLM output text) directly via substring matching. Future developers reading these comments will look for `_judge_label` in context and find it absent, wasting diagnostic time.
- **Fix**: Either (a) write `_judge_label` into context (alongside `_judge_result`/`_judge_reason`) and use it in `resolve_labeled_target`, or (b) update the comments to accurately describe the substring-matching-on-`judge_reason` approach. Option (b) is simpler and aligns with the current implementation.

#### W-002: `resolve_labeled_target` has no unit test coverage

- **Source**: `crates/nexus-orchestration/src/tasks/mod.rs:782-791` (`resolve_labeled_target` method)
- **Evidence**: `grep -rn 'fn test.*labeled\|resolve_labeled.*test' crates/nexus-orchestration/src/tasks/mod.rs` returns no matches. The only tests for labeled routing are deserialization tests in `nexus-contracts` (`parse_labeled_next_n_way_from_yaml_list`, `parse_labeled_next_two_way_like_binary_gonogo`, `backward_compat_binary_gonogo_still_parses`).
- **Impact**: `resolve_labeled_target` is the sole runtime router for the `Labeled` next target variant. It has three code paths (label match → `GoTo(target)`, no match → `WaitForInput`, non-`Labeled` next → `WaitForInput`) — none of which are covered by unit tests. Per plan T4: "Write failing test: judge returns label string, `judge_next_action` returns `Continue` for labeled targets (TDD RED)". The plan expected TDD tests that were not delivered.
- **Fix**: Add unit tests for `resolve_labeled_target` covering: (a) single label match → `GoTo(target)`, (b) first-match when multiple labels could match, (c) no label match → `WaitForInput`, (d) non-`Labeled` next (e.g., `GoNogo`) → `WaitForInput`.

#### W-003: `resolve_labeled_target` uses substring matching — fragile for common-word labels

- **Source**: `crates/nexus-orchestration/src/tasks/mod.rs:785` (`judge_reason.contains(&edge.label)`)
- **Evidence**: The method uses `String::contains()` which matches any substring occurrence. If a preset declares `label: "go"` and the judge outputs "The argument is good but needs revision", the substring "go" in "good" will trigger a false match.
- **Impact**: The bug surface is limited because: (a) presets are authored by humans who control both the label names and judge prompts, (b) the `DuplicateLabel` validator prevents ambiguous labels. However, the matching semantics are not documented in the spec (§3.1.2 says "scans the judge's output text for known label strings" but doesn't clarify substring vs. word-boundary vs. exact matching).
- **Fix**: At minimum, document the substring-matching semantics in the spec and in doc comments on `resolve_labeled_target`. Consider tokenizing or using word-boundary matching in a future revision. Non-blocking for this plan but should be recorded as a residual if deferred.

### 🟢 Suggestion

#### S-001: Plan AC4(b)(c) — label coverage checking not implemented

- **Source**: Plan `2026-06-19-v1.52-n-way-gonogo-routing.md` §4 Acceptance Criterion 4
- **Evidence**: AC4(b) requires "orphan labels (labels defined in next edges but not producible by the judge) are rejected with a diagnostic" and (c) "all label values the judge can produce must have at least one next edge". The implementation adds `check_labeled_edge_duplicates` (duplicate detection) but not `check_labeled_edge_coverage` (label producibility). BFS reachability covers state-to-state traversal but not whether the judge can actually produce each label string.
- **Impact**: Low. The judge template format doesn't currently expose label producibility metadata, so implementing AC4(b)/(c) would require additional schema work (e.g., annotating the judge template with expected output labels). The current implementation is a reasonable V1.52 T-B P0 slice.
- **Recommendation**: Update plan AC4 to reflect the shipped scope (duplicate detection only) or add a residual (R-V152-LABELCOV) for full label coverage checking in a future iteration.

#### S-002: `resolve_labeled_target` placement on `StateCompositeTask`

- **Source**: `crates/nexus-orchestration/src/tasks/mod.rs:782-791`
- **Evidence**: `StateCompositeTask` is already 3172+ lines. Adding `resolve_labeled_target` here continues the god-class pattern. The method is conceptually a "routing strategy" that could live on a separate type (e.g., `NextTargetRouter` or a method on `NextTarget` itself via a trait).
- **Impact**: Low. The method is small (10 lines) and well-isolated. But each new method on `StateCompositeTask` makes future refactoring harder.
- **Recommendation**: Defer. Consider extracting judge-related routing logic into a `JudgeRouter` struct in a future hygiene plan.

#### S-003: Spec §3.1.2 — matching semantics not documented

- **Source**: `.mstar/knowledge/specs/preset-conditional-routing.md` §3.1.2
- **Evidence**: Spec says "scans the judge's output text for known label strings and returns `GoTo(target)` for the first match" — correct but doesn't clarify whether matching is substring, word-boundary, or exact. The implementation uses `contains()` (substring).
- **Impact**: Low. The spec is otherwise well-aligned with the implementation. The matching semantics detail is minor.
- **Recommendation**: Add a sentence to §3.1.2: "Matching uses substring containment (`contains`); the first label whose string appears anywhere in the judge's output text wins. Authors should choose labels that are unlikely to appear as substrings of unrelated words."

## Source Trace
- Finding ID: W-001
- Source Type: manual-reasoning + grep
- Source Reference: `grep -rn '_judge_label' crates/` — 2 comments found, 0 context writes
- Confidence: High

- Finding ID: W-002
- Source Type: manual-reasoning + grep
- Source Reference: `grep -rn 'fn test.*labeled\|resolve_labeled.*test' crates/nexus-orchestration/src/tasks/mod.rs` — 0 matches
- Confidence: High

- Finding ID: W-003
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-orchestration/src/tasks/mod.rs:785`
- Confidence: Medium

- Finding ID: S-001 through S-003
- Source Type: manual-reasoning (plan vs code diff review)
- Confidence: High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: W-001 (documented but unimplemented `_judge_label` context write) and W-002 (missing unit tests for `resolve_labeled_target`, the sole Labeled routing method) should be resolved before approval. W-003 (substring matching fragility) should be addressed or deferred with a tracking residual. All three suggestions are non-blocking.

### Architecture Assessment

The N-way labeled routing generalization is well-designed at the architecture level:

- **Layer separation**: Contracts (DTO types) → Orchestration preset (loader + validation) → Orchestration tasks (runtime) — clean boundaries maintained. The only bleed is the `_judge_label` comment drift (W-001).
- **Naming**: `LabeledNext`, `GoNogoNext`, `NextTarget::Labeled` — vocabulary is coherent and parallels existing conventions.
- **Backward compatibility**: Untagged serde with `Linear → GoNogo → Labeled → Conditional` ordering correctly distinguishes old `{go, nogo}` maps (→ `GoNogo`) from new list-of-objects shapes (→ `Labeled`). All deserialization tests pass.
- **Reuse over duplication**: The existing `GoNogo` path is fully preserved; the `Labeled` path adds without replacing. `add_edge` for reachability + `GoTo` for routing is clean separation.
- **Spec alignment**: Spec §3.1 accurately describes the implementation with one minor gap (matching semantics — S-003).
- **Reachability validator**: BFS treats `Labeled` edge targets as forward edges, preserving existing reachability guarantees. `DuplicateLabel` diagnostic is properly integrated with the existing `ValidationResult` emission path.

## Revalidation (2026-06-19, targeted re-review)

**Re-review scope**: Review range `b21492b3..4900b582` (3 fix commits: `1b460a17`, `fda4e826`, `4900b582`)

### Fix Validation

| Initial Warning | Status | Evidence |
|----------------|--------|----------|
| W-001 (`_judge_label` drift) | **Resolved** | commit `1b460a17`: `resolve_labeled_target` now accepts `context: &Context` and writes `context.set_sync("_judge_label", label)` on successful match (line ~804). Loader comment updated (`loader.rs:909-911`). Spec overlay §3.1.2 documents the write. Unit test `resolve_labeled_target_writes_judge_label_context` confirms `_judge_label` is set to `"outline"` after match. |
| W-002 (no unit tests) | **Resolved** | commit `fda4e826`: 9 unit tests in `tasks::tests` covering single-label match, multi-label first-match, no-match error, non-Labeled next → WaitForInput, None next → WaitForInput, context write, GoNogo auto-conversion (3 variants). 5 integration tests in `tests/labeled_routing.rs` covering full preset load, hybrid GoNogo+Labeled, orphan label detection, embedded preset regression, no-match no-stall. All 14 tests pass. |
| W-003 (substring matching fragility) | **Acknowledged (non-blocking)** | commit `1b460a17`: descending-length sort (`candidates.sort_by_key(… Reverse(label.len()))`) mitigates common-word collision (e.g., `"nogo"` checked before `"go"`). Spec overlay §3.1.2 documents substring matching semantics + caveat with descending-length mitigation note. Full normalization (anchored tokens, case-insensitive) deferred to V1.52 P-last WL-A per PM-override. |

### Additional Fix-Wave Improvements (not requested by qc1)

- **Binary→Labeled auto-conversion** (W-QC3-2): `resolve_labeled_target` now handles `GoNogo` edges as labeled edges `("go", "nogo")`, so legacy GoNogo presets are reachable via either routing API.
- **No-match deterministic fail** (W-QC3-3): returns `Err(GraphError::TaskExecutionFailed)` instead of `WaitForInput`, with `tracing::warn!` logging known labels + judge output excerpt.
- **Spec overlay** (`preset-conditional-routing.md`): §3.1 updated from placeholder to shipped spec covering wire format (§3.1.1), runtime routing (§3.1.2), backward compatibility, and substring matching caveat.
- **Plan body**: AC matrix updated with ✅ SHIPPED annotations and additional shipped scope documentation.

### Updated Findings

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 (all blocking Warnings resolved) |
| 🟢 Suggestion | 4 (3 from initial review + N-W001 below) |

#### 🟢 N-W001: `cargo +nightly fmt --all --check` reports formatting diffs in `tests/labeled_routing.rs`

- **Source**: `cargo +nightly fmt --all --check` on `tests/labeled_routing.rs`
- **Evidence**: 8 formatting diffs (line wrapping only) in the newly added integration test file
- **Impact**: None — cosmetic only; clippy passes clean; all tests pass
- **Fix**: Run `cargo +nightly fmt --all` before merge

### Updated Verdict: **Approve**

All three blocking Warnings from the initial review are resolved with verified evidence. The fmt check failure is cosmetic and does not affect correctness. The architecture remains coherent: `_judge_label` is now written and observable, `resolve_labeled_target` has comprehensive test coverage, and substring matching fragility has a documented partial mitigation with deferred full normalization.

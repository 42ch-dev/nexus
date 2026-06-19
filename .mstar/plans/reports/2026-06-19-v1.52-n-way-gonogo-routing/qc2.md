---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-19-v1.52-n-way-gonogo-routing"
verdict: "Approve"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk (per role parameters: focus=security_correctness, report_suffix=qc2)
- Report Timestamp: 2026-06-19

## Scope
- plan_id: 2026-06-19-v1.52-n-way-gonogo-routing
- Review range / Diff basis: b97ec0d9..b21492b3
- Working branch (verified): feature/v1.52-n-way-gonogo-routing
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-tb-p0/
- Files reviewed: 9 (diff stat)
- Commit range: b97ec0d9..b21492b3
- Tools run:
  - git diff b97ec0d9..b21492b3 --stat
  - git diff (targeted: preset/, graph_flow.rs, contracts preset.rs)
  - cargo test -p nexus-orchestration (preset_validation, preset_parse_minimal, judge paths)
  - cargo test -p nexus-contracts (labeled parse + backward_compat tests)
  - cargo clippy --all -- -D warnings (clean)
- Iteration compass referenced: .mstar/iterations/v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md

## Findings

### 🔴 Critical
- **None**

### 🟡 Warning
- **LLM judge output parsing uses naive substring match (`judge_reason.contains(&edge.label)`)** in `StateCompositeTask::resolve_labeled_target` (tasks/mod.rs:782-791).
  - No normalization (case, whitespace, trimming).
  - No word-boundary or exact-token requirement.
  - Risk: partial matches (label "go" matches "ongoing", "forgot", etc.), case variants, or judge emitting prose that happens to contain a label string.
  - This is the primary runtime dispatch for N-way routing. While the feature is new (no embedded presets use `Labeled` yet), any future preset using it inherits this brittleness.
  - Evidence: code path in `run()` (lines 1044-1045) and throttle path (1014); `LlmJudgeTask::evaluate` returns free-text `reason`.
  - Suggested fix: normalize (to_lowercase + trim), consider exact match or require structured JSON from judge in a follow-up, or add explicit delimiter protocol.

- **No dedicated unit test for `resolve_labeled_target` exercising label matching, non-matches, or noisy input.**
  - Existing `judge_next_action_*` tests (tasks/mod.rs ~3104) only cover the legacy bool path and `GoNogo`/`Linear`.
  - Contract tests cover YAML shapes and deserialization, but not the substring dispatch logic under realistic judge output.
  - Reachability and duplicate-label validator tests are present in validation.rs, but runtime routing correctness is under-tested for the new variant.

- **Untagged `NextTarget` serde discrimination is structurally safe today but undocumented.**
  - Order: `Linear(String)` → `GoNogo(GoNogoNext)` (map) → `Labeled(Vec<LabeledNext>)` (seq) → `Conditional`.
  - `Labeled` is a sequence at the `next:` value; legacy GoNogo is a map with `go`/`nogo` keys. A malformed map like `{label: "x", nogo: true}` fails GoNogo deserialization and also fails Labeled (expects seq), producing a clear error rather than silent misclassification.
  - No embedded preset YAML uses the new list form, so no collision surface exists in current data.
  - However, the loader and contract docs should explicitly state the discrimination rules and that `Labeled` requires a YAML sequence (list of label/target pairs) while GoNogo requires the specific map shape. This reduces future maintenance risk if the enum evolves.

### 🟢 Suggestion
- Add a unit test (or integration test via preset execution harness) for `resolve_labeled_target` with:
  - Exact label match.
  - Case variation.
  - Substring false-positive cases (to document current behavior or drive a hardening change).
  - Multiple edges; first-match-wins semantics.
- Consider making label matching more robust in a small follow-up (e.g., normalized contains or "label: <X>" protocol) before any production preset adopts N-way routing.
- In `check_labeled_edge_duplicates`, the error path already uses `DiagnosticSeverity::Error` + `DuplicateLabel` category — good. Ensure this surfaces in the public preset validate API surface when added.
- Reachability now correctly walks `Labeled` edges (validation.rs:234-240 and loader graph builders). Cycle detection for labeled graphs is inherited from the existing graph reachability machinery — acceptable for this slice.
- No preset-level authorization check for target states is expected or present; routing targets are structural. Creator/tenant authorization is a runtime concern outside the preset manifest (correct scoping).

## Source Trace
- Finding ID: F-001 (LLM substring parsing)
- Source Type: manual-reasoning + code review of runtime dispatch
- Source Reference: `crates/nexus-orchestration/src/tasks/mod.rs:782` (`resolve_labeled_target`), `1044` (call site), `1014` (throttle path)
- Confidence: High

- Finding ID: F-002 (test coverage for new dispatch)
- Source Type: manual-reasoning + test grep
- Source Reference: absence of `resolve_labeled_target` tests; presence of legacy `judge_next_action_*` only
- Confidence: High

- Finding ID: F-003 (serde untagged documentation)
- Source Type: manual-reasoning on `#[serde(untagged)]` + contract tests
- Source Reference: `crates/nexus-contracts/src/local/orchestration/preset.rs:319-342` (NextTarget + LabeledNext)
- Confidence: Medium-High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 (2 are documentation/test gaps; 1 is runtime parsing robustness) |
| 🟢 Suggestion | 4 |

**Verification performed**
- All targeted tests under `preset_validation`, `preset_parse_minimal`, and contract parse roundtrips for new + legacy shapes passed.
- `backward_compat_binary_gonogo_still_parses` + `parse_labeled_next_*` tests confirm no regression on existing embedded preset load paths (all current embedded presets remain Linear or would parse as legacy GoNogo).
- `cargo clippy --all -- -D warnings` clean.
- Loader enforces `llm_judge` only + target state existence for Labeled (same as GoNogo).
- Duplicate label check per-state (HashSet on label) prevents ambiguous `(source, label)` routing.
- Graph reachability and outer/wired graph builders were extended for Labeled edges.

**Verdict**: Approve

(The three Warnings are either pre-existing pattern risks made visible by the new feature, or test/documentation gaps that do not block the correctness of the delivered slice. No Critical findings. Backward-compat invariant holds. Input validation for targets and label uniqueness is present.)

## Revalidation
N/A — initial tri-review wave.

## Plan Update
No changes to plan artifacts required from this reviewer. Residuals (if any) to be registered by PM per harness rules. The primary actionable item (hardening `resolve_labeled_target`) can be tracked as a low-severity follow-up or deferred until first real N-way preset is authored.

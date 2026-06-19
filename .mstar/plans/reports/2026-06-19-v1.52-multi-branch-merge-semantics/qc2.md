---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-19-v1.52-multi-branch-merge-semantics"
verdict: "Approve"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk (focus=security_correctness)
- Report Timestamp: 2026-06-19

## Scope
- plan_id: 2026-06-19-v1.52-multi-branch-merge-semantics
- Review range / Diff basis: b97ec0d9..93416cf8
- Working branch (verified): feature/v1.52-multi-branch-merge-semantics
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-tb-p1/
- Files reviewed: 6 (core changes in loader, validation, tasks, contracts, spec, and labeled routing tests)
- Commit range: b97ec0d9..93416cf8
- Tools run:
  - git rev-parse + branch verification in worktree
  - git diff b97ec0d9..93416cf8 -- crates/nexus-orchestration/src/preset/loader.rs
  - git diff b97ec0d9..93416cf8 -- crates/nexus-orchestration/src/preset/validation.rs
  - git diff b97ec0d9..93416cf8 -- crates/nexus-orchestration/src/tasks/mod.rs
  - git diff b97ec0d9..93416cf8 -- crates/nexus-contracts/src/local/orchestration/preset.rs
  - git diff b97ec0d9..93416cf8 -- crates/nexus-orchestration/tests/labeled_routing.rs
  - cargo test -p nexus-orchestration --test preset_validation
  - cargo test -p nexus-orchestration --test labeled_routing
  - cargo clippy -p nexus-orchestration -- -D warnings (clean)
- Iteration compass referenced: .mstar/iterations/v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md
- Primary spec: .mstar/knowledge/specs/preset-conditional-routing.md §3.2

## Findings

### 🔴 Critical
- **None**

### 🟡 Warning
- **Insufficient runtime execution coverage for the new merge gating behavior.**
  - The core promise of T-B P1 is that a merge node correctly blocks (`WaitForInput`) until the declared condition (`all` / `any` / `quorum N/M`) is satisfied by arrivals recorded in `_merge_<id>`, then advances and clears the key.
  - Existing tests (`labeled_routing.rs`, `preset_validation.rs`) cover P0 labeled routing, structural validation (`check_merge_node_integrity`), loader wiring of `expected_incoming`, and contract round-trips for `MergeKind`.
  - No hermetic test in the reviewed range drives multiple source `llm_judge` (or simulated labeled) completions into a merge target and asserts the gate actually waited then proceeded.
  - Evidence: `cargo test -p nexus-orchestration -- merge_semantics` returned 0 matches; merge logic lives in `StateCompositeTask::run` (tasks/mod.rs:858-894) and arrival recording (799-804). This is a correctness gap for the primary new feature.
  - Risk: subtle off-by-one or ordering bugs in arrival accumulation / condition evaluation could ship undetected until real preset usage.

- **Outer-graph cycle detection is absent for merge-involved topologies.**
  - Inner graphs have `detect_cycle` on `depends_on`.
  - Main preset state graph only gets BFS reachability ("initial can reach a terminal") plus the loader's `add_edge` for every Labeled target.
  - Consequence: a cycle such as A (labeled)→ B (merge) → A is not explicitly rejected at load time. If the cycle still permits a path from `initial` to `terminal`, the preset will load. A stuck merge in the cycle could cause non-progress.
  - The reachability validator extension for Labeled (validation.rs:237-244) helps but does not substitute for a topological / cycle check on the outer graph when merge nodes are present.
  - Not a bypass of the "≥2 incoming" rule, but a latent authoring hazard for complex multi-branch presets.

- **No cardinality bound or warning for high fan-in merge nodes.**
  - A merge state can declare (or be targeted by) an arbitrary number of incoming labeled edges.
  - Runtime stores `Vec<String>` of arrived labels under `_merge_<id>` (deduplicated).
  - Preset YAML size limits exist, but there is no explicit diagnostic or soft limit when incoming ≥ N (e.g., 100). Context bloat or memory pressure under a crafted preset is theoretically possible, though author-controlled.
  - Validator requires ≥2 but places no upper bound.

### 🟢 Suggestion
- Add a focused integration test (new or extension of `labeled_routing.rs`) that:
  - Loads or builds a small preset with a merge node (`any`, `all`, and `quorum`).
  - Drives the source labeled edges (via test harness or direct `resolve_labeled_target` + context manipulation).
  - Asserts `WaitForInput` until the Nth arrival, then `Continue` / `GoTo`, and that the `_merge_<id>` key is cleared on advance.
- Consider adding a lightweight outer-graph cycle / topological check (or at least a warning diagnostic) when a state with `merge:` participates in a strongly-connected component. This would be cheap given the existing adjacency work.
- Make the high-fan-in case emit a `DiagnosticCategory::MergeIntegrity` (or new category) at validation time when incoming count exceeds a documented soft limit (e.g., 16 or 32). Helps authors avoid accidental complexity.
- The spec §3.2 correctly documents the internally-tagged `kind:` form, the validator rules (≥2, n≥1, n≤m, m==incoming), GoNogo counting as labeled, and the context key lifecycle. Keep the "default is all when absent" language prominent.
- Authoring foot-gun mitigation is good (validator rejects n>m, n<1, m≠incoming, <2 incoming). When a `nexus42 preset validate` CLI surface is added (currently only via daemon), ensure these `MergeIntegrity` errors surface with actionable messages.
- Backward-compat note in spec is accurate: states without `merge:` default to `All`; existing GoNogo + Labeled presets are unaffected.

## Source Trace
- Finding ID: F-001 (runtime merge coverage gap)
  - Source Type: manual-reasoning + test execution + code inspection
  - Source Reference: absence of merge execution tests; `StateCompositeTask` gate (tasks/mod.rs:855-894), arrival write (799-804), `check_merge_node_integrity` (validation.rs:424-524)
  - Confidence: High

- Finding ID: F-002 (outer cycle detection)
  - Source Type: code inspection of validation.rs (only inner-graph `detect_cycle`) and reachability BFS
  - Source Reference: validation.rs:210-299 (check_initial_to_terminal_reachability), loader.rs:788-853 (inner only), no equivalent for outer + merge
  - Confidence: High

- Finding ID: F-003 (cardinality / resource)
  - Source Type: manual-reasoning on data model
  - Source Reference: `expected_incoming: usize`, `arrived: Vec<String>` in context, no bound in validator or loader
  - Confidence: Medium

- Finding ID: F-004 (positive — load-time integrity)
  - Source Type: code + test
  - Source Reference: `check_merge_node_integrity` + loader manifest validation for Quorum; all tests pass; contracts round-trips
  - Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 5 |

**Verification performed**
- Worktree alignment: `git rev-parse --show-toplevel`, `git branch --show-current`, and commit SHAs for b97ec0d9 / 93416cf8 all matched Assignment.
- All preset_validation (13) and labeled_routing (5) tests pass.
- `cargo clippy -p nexus-orchestration -- -D warnings` clean.
- Loader rejects invalid quorum (n<1, n>m) and merge nodes with <2 incoming.
- `m` must exactly equal pre-computed incoming count for `Quorum`.
- Merge nodes must have an outgoing edge (or be `terminal`).
- Arrivals are deduplicated; gate is evaluated on every entry to the merge state; key cleared on advance.
- GoNogo edges are counted toward incoming for merge purposes (consistent with spec).
- Reachability walker was extended for Labeled edges (covers fan-out from merge sources).
- Spec overlay §3.2 accurately reflects the shipped YAML shape, runtime, and validator rules.

**Verdict**: Approve

The load-time validator provides a strong static guarantee that only well-formed merge nodes (correct bounds, sufficient fan-in, matching m) can ever be loaded. The runtime gate is a simple, auditable count comparison before enter actions. No Critical security or correctness violations (no bypass of validation, no injection surface, no silent stalls on the labeled path — deterministic fail on no-match is preserved).

The primary residual risk is **observability / test coverage of the dynamic waiting behavior** and **potential for complex merge graphs to contain cycles or excessive fan-in that are only caught at authoring time**. These are authoring-time and future-hardening concerns rather than immediate "this preset will misbehave at runtime" defects for correctly authored presets.

All embedded presets continue to pass strict validation. No behavior change for presets without `merge:`.

---

## Completion Report v2

**Agent**: qc-specialist-2
**Task**: V1.52 T-B P1 tri-review (qc2) — security/correctness focus on multi-branch merge semantics
**Status**: Done
**Scope Delivered**:
- Verified review worktree alignment and exact diff range b97ec0d9..93416cf8 on feature/v1.52-multi-branch-merge-semantics.
- Performed targeted diff review, test execution, and clippy on the merge-related changes (loader pre-compute + validation integrity + runtime gate in StateCompositeTask).
- Manual security/correctness analysis per Assignment checklist (quorum bounds, wait-any determinism, cycle safety, resource exhaustion, reachability bypass, spec accuracy, failure modes, authoring foot-guns).
- Produced this report per mstar-review-qc template.

**Artifacts**:
- Report: .mstar/plans/reports/2026-06-19-v1.52-multi-branch-merge-semantics/qc2.md (this file)

**Validation**:
- cargo test -p nexus-orchestration --test preset_validation (all 13 pass)
- cargo test -p nexus-orchestration --test labeled_routing (all 5 pass)
- cargo clippy -p nexus-orchestration -- -D warnings (clean)
- Manual verification of validator rules and runtime gate logic against the reviewed diff and spec §3.2.

**Issues/Risks**:
- See 🟡 Warnings above (primarily test coverage of runtime merge advancement and lack of outer-graph cycle detection). These are noted for follow-up but do not block approval of the current slice.
- No Critical findings.

**Plan Update**:
- No changes to plan or status.json (QC role does not own those).

**Handoff**:
- Ready for PM consolidation with qc1/qc3 and subsequent QA if this wave passes.

**Git**:
- (To be filled after `git add` + `git commit` from repo root; see execution log for final SHA.)

**Git (post-commit)**:
- Report committed from repo root after writing.
- Command: cd /Users/bibi/workspace/organizations/42ch/nexus && git add .mstar/plans/reports/2026-06-19-v1.52-multi-branch-merge-semantics/qc2.md && git commit -m "qc(v1.52-tb-p1): qc2 security/correctness review"
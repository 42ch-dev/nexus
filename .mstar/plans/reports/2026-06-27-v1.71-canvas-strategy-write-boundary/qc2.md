---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-27-v1.71-canvas-strategy-write-boundary"
verdict: "Request Changes"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-28

## Scope
- plan_id: 2026-06-27-v1.71-canvas-strategy-write-boundary
- Review range / Diff basis: git diff 39493026..HEAD -- schemas/local-api/canvas/ crates/nexus-contracts/src/generated/local_api/canvas/ crates/nexus-contracts/src/generated/local_api/mod.rs crates/nexus-contracts/src/generated/mod.rs crates/nexus-contracts/tests/schema_drift_detection.rs crates/nexus-daemon-runtime/src/api/errors.rs crates/nexus-daemon-runtime/src/api/handlers/mod.rs crates/nexus-daemon-runtime/src/api/handlers/strategy.rs crates/nexus-daemon-runtime/src/api/mod.rs packages/nexus-contracts/package.json packages/nexus-contracts/src/generated/index.ts packages/nexus-contracts/src/generated/local-api/canvas/ apps/web/DESIGN.md apps/web/DESIGN.dark.md apps/web/src/index.css apps/web/src/components/canvas/strategy-canvas.tsx apps/web/src/lib/canvas/preset-yaml.ts apps/web/src/lib/canvas/use-strategy-data.ts apps/web/src/lib/nexus/browser-client.test.ts apps/web/src/lib/nexus/browser-client.ts apps/web/src/lib/nexus/types.ts apps/web/tailwind.config.ts
- Working branch (verified): iteration/v1.71
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 23 (per diff)
- Commit range: 445e2d80 (HEAD) .. 39493026 (base)
- Tools run: cargo clippy --all -- -D warnings (clean), cargo test -p nexus-daemon-runtime (all green), pnpm --filter web test (139 passed)

## Findings

### 🔴 Critical

- **C1 — Prompt template write is not atomic with YAML revision bump (partial-update hazard)**: In `patch_prompt_template`, `std::fs::write(&canonical_template, body)` is performed *before* `validate_preset_yaml` and *before* `write_preset_yaml` (the temp+rename+fsync path that bumps `revision:`). If the YAML write fails after the template file is mutated (disk full, permission, serialization error, validation failure), the on-disk template reflects the new content while the `revision:` in `preset.yaml` remains at the old value. A subsequent client with the same `base_revision` will pass the stale check and the write will appear to succeed again, but the system is in an inconsistent state (template updated, revision not). This violates the "atomic persistence" and "failed validation/conflict must not increment revision" acceptance criteria. The prompt file itself has no temp+rename+fsync durability either.
  - Source: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:755` (fs::write), 762 (validate), 770 (write_preset_yaml), 692 (revision check).
  - Fix: Either (a) write the prompt template via the same atomic rename pattern inside the bundle and only commit the YAML bump after both succeed, or (b) stage the new template content inside the YAML (or a sidecar that is updated atomically with the YAML) so there is a single durability point. At minimum, on validation/YAML failure after template write, attempt best-effort rollback of the template file (document the window).

- **C2 — No cross-request / cross-process OCC or locking for concurrent writers (TOCTOU on base_revision)**: The revision precondition check (`if req.base_revision != current_revision { return conflict }`) is performed after a non-atomic read of `preset.yaml`. Two clients that both read the same revision can both pass the check, both mutate, and both attempt the write. The second write will overwrite the first with no conflict error for the loser. The plan and compass explicitly require "no TOCTOU" and "stale base_revision rejection". Advisory locks exist elsewhere in the crate (script section status) but are not used for Strategy preset writes. In a single-author local product this is low-probability, but the correctness claim ("structured conflict error", "atomic") is violated under concurrent access.
  - Source: `load_user_preset_yaml:391`, `patch_state:393`, `patch_transition:601`, `patch_prompt_template:692`; no `RuntimeLockGuard` or flock around the load+check+write sequence.
  - Fix: Either document the single-writer assumption with a clear warning in AGENTS.md and error messages, or acquire an advisory lock (or use a higher-level optimistic write with post-write re-check + rollback) before the revision comparison.

- **C3 — Conflict modal UX is a stub and does not match locked spec / acceptance criteria**: The implemented `ConflictModal` (and the call site) only offers "Keep editing" + "Refetch graph". The compass (§A6), plan (T7, acceptance), and product spec require a richer modal with headline/body copy ("This node changed while you were editing"), "What changed" vs "What you were about to do", and three actions: **Use current** (primary), **Reapply my edit**, **Review side-by-side** (enabled only for non-overlapping fields). The current modal can mislead the user into thinking "refetch" is the only safe action and does not surface draft vs canonical values. This is a direct violation of the P0 acceptance criteria for conflict UX.
  - Source: `apps/web/src/components/canvas/strategy-canvas.tsx:478` (the modal rendered on `conflict`), 478-522 (simplified implementation), `use-strategy-data.ts:377` (isStrategyConflictError detection).
  - Fix: Implement the full modal per compass §A6 (or explicitly amend the plan/compass with a deferral + residual if the richer UX is intentionally out of this wave).

### 🟡 Warning

- **W1 — Template file mutated before validation; no rollback on validation failure**: After `fs::write` of the prompt template, `validate_preset_yaml` is called on the *in-memory* (pre-bump) YAML. If validation fails, the template file on disk has already been overwritten. The error path returns `strategy_validation_failed` but leaves the mutated template behind. Subsequent loads will see the bad template content.
  - Source: `patch_prompt_template:755` (write), 762 (validate), 763 (early return).
  - Fix: Write the template to a `.tmp` sibling first, run validation against a dry-run manifest that incorporates the staged content, then rename only on success. Or stage the body change inside the YAML transaction.

- **W2 — Condition syntax / grammar validation is not re-run on transition patches**: `apply_transition_patch` and helpers (`apply_conditional_rules`, etc.) perform structural matching on `old_target` / `condition`. They do not invoke the preset condition grammar parser or `validate_preset_semantic` / capability reference checks for the new condition value. If a client sends a syntactically invalid `condition`, or a condition referencing a now-removed capability, the patch can succeed (as long as the structural target matches). Full semantic validation only happens later via `validate_preset_yaml`, which may or may not catch condition-specific errors depending on how the loader treats `next.when`.
  - Source: `strategy.rs:472` (apply_transition_patch), 505 (apply_conditional_rules), 651 (validate after patch).
  - Fix: Either parse/validate the incoming `condition` (if present) with the same grammar used by the loader before applying, or ensure the subsequent `validate_preset_yaml` is proven to reject bad conditions (add a regression test).

- **W3 — Prompt path containment relies on canonicalize + starts_with after a non-atomic assert**: `assert_template_file_safe` is called, then `canonicalize` on bundle and (if exists) template, then `!canonical_template.starts_with(&canonical_root)`. For a brand-new template the `else` branch trusts the `join`. While `assert_template_file_safe` already rejects `..`, `\`, control chars, etc., a TOCTOU between the safe check and the later canonicalize (symlink swap in the bundle dir) could theoretically let a path escape. The code has a comment acknowledging "defence in depth". In practice the risk is low (user-owned directory), but it is not a pure allow-list containment.
  - Source: `patch_prompt_template:702` (assert), 711-734 (canonicalize + starts_with).
  - Fix: After canonicalize, also re-assert that the final path under the bundle still passes `assert_template_file_safe` (or a stricter "is under bundle" predicate) before writing.

### 🟢 Suggestion

- **S1 — Add regression test for concurrent writers (two clients, same base_revision)**: The existing tests cover stale revision → 409 for a single client. There is no test that two concurrent requests with the same `base_revision` both attempt the write and at least one receives a conflict (or that the system ends up with a consistent revision). Add one under `tests/strategy_patch.rs` or inside the handler `#[cfg(test)]`.
- **S2 — Consider surfacing `lastKnownRevision` / freshness in the conflict error body more explicitly**: The client already tracks it; making the 409 body include an `expected_revision` vs `current_revision` distinction (or an `If-Match` style hint) would make client-side retry logic clearer.
- **S3 — Document the single-writer assumption for Strategy patches**: If the decision is that local single-author use makes locking unnecessary, record the assumption and the risk (lost update under concurrent editing) in `crates/nexus-daemon-runtime/AGENTS.md` and in the Strategy section of the relevant spec, so future maintainers do not assume OCC is enforced.

## Source Trace
- Finding C1: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:755` (fs::write for prompt), 770 (YAML bump after), handler tests 959.
- Finding C2: revision check sites in all three patch handlers; absence of lock (contrast with `runtime_lock_holder` usage elsewhere).
- Finding C3: `apps/web/src/components/canvas/strategy-canvas.tsx:478` (ConflictModal) vs compass §A6 / plan acceptance.
- Error envelope: `crates/nexus-daemon-runtime/src/api/errors.rs:260` (`strategy_conflict` variant) and `IntoResponse` — all handlers use it correctly; no ad-hoc bodies observed.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 3 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: Three blocking correctness / safety issues (non-atomic prompt write, missing OCC/locking for the revision precondition, and conflict modal not matching the locked UX spec) mean the implementation does not yet meet the P0 acceptance criteria for "atomic persistence", "no TOCTOU", "stale base_revision rejection", and "conflict modal UX". The error envelope, path-safety guardrails, and id-matching are solid. The changes are surgical and the test surface is good; the gaps are addressable without large refactor.

## Residual Findings (proposed for SSOT)
- R-V171-P0-QC2-C1 — Prompt template file write not atomic with YAML revision bump (partial update risk)
- R-V171-P0-QC2-C2 — No cross-client locking / OCC around base_revision check for Strategy patches (TOCTOU)
- R-V171-P0-QC2-C3 — Conflict modal is a stub (missing Use current / Reapply / side-by-side per compass §A6)

All other observations (W1–W3, S1–S3) can be tracked as lower-severity or closed after the Criticals are addressed.

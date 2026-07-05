---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-19-v1.52-outline-five-q-and-auto-promote"
verdict: "Approve"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-19T14:00:00Z

## Scope
- plan_id: 2026-06-19-v1.52-outline-five-q-and-auto-promote
- Review range / Diff basis: b97ec0d9..431aca4c
- Working branch (verified): feature/v1.52-outline-five-q-and-auto-promote
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p0
- Files reviewed: 18
- Commit range: b97ec0d9..431aca4c (2 commits — 2425b12b harness signoff + 431aca4c implementation)
- Tools run: cargo clippy --all -- -D warnings (clean), cargo +nightly fmt --all -- --check (clean), cargo test -p nexus-orchestration -p nexus-local-db -p nexus42 (all pass)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion

**S-001: `outline_five_q_check` heuristic signal arrays may drift from `outline-exit.md` LLM prompt**

The five signal arrays in `outline_five_q_check` (`arc_signals`, `fore_signals`, `hook_signals`) mirror the qualitative question descriptions in the `outline-exit.md` LLM prompt (§ questions 2, 3, 5). The deterministic heuristic and the LLM judge share the same five dimensions but use different evaluation mechanisms — the heuristic uses substring matching while the LLM uses semantic understanding. Any future refinement of the outline questions should be kept in sync across both implementations. The spec correctly documents the LLM gate as primary and the heuristic as a deterministic fallback.

**Remediation**: Add a cross-reference comment in `quality_loop.rs` pointing to `outline-exit.md` and vice versa. Consider extracting the signal word lists into a shared constant or a spec-linked reference file during the P-last cleanup pass.

**S-002: `write_auto_promoted_log` signature doesn't communicate None = test-only**

The function takes `workspace_dir: Option<&Path>` and silently returns `Ok(())` when `None`. The docstring correctly states this is for "hermetic tests", but the `Option` in the signature is ambiguous without reading the docstring. A caller could inadvertently pass `None` in production and lose audit logging.

**Remediation**: Consider a `#[cfg(not(test))]` path that requires workspace_dir, or add a `tracing::debug!` when logging is skipped so the absence is observable.

**S-003: `#[cfg_attr(not(test), allow(dead_code))]` on `LlmExtractTask` remains**

`LlmExtractTask` and its `evaluate` method are dead code in production builds (no preset currently wires the `exit_when: llm_extract` path). The `dead_code` suppression was pre-existing and is preserved in this refactoring. The refactoring actually improves the situation by making `evaluate` delegate to the shared `run_llm_extract`, keeping the future wiring path cleaner.

**Remediation**: Document the timeline for wiring `LlmExtractTask` into a preset or remove the struct if the feature is deferred indefinitely. Not blocking — the refactoring itself reduces the maintenance burden of the dead code.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| S-001 | manual-reasoning | `quality_loop.rs` L1237-1295 vs `outline-exit.md` L20-38 | Medium |
| S-002 | manual-reasoning | `kb.rs` `write_auto_promoted_log` L958-965 | Medium |
| S-003 | manual-reasoning | `tasks/mod.rs` L519, L561 | High |

## Architecture Review Summary

### Outline 五问 Gate Architecture

The `outline_review` state sits between `outline_chapter` and `draft_chapter` in the `novel-writing` preset chain. Its `exit_when: llm_judge` uses the `outline-exit.md` prompt evaluated through the existing `judge.llm` capability — reusing the same LLM-judge infrastructure as the `finalize` gate. This is architecturally sound: no new capability needed, no new task type, just a new state with a new prompt template.

The deterministic complement `outline_five_q_check` in `quality_loop.rs` is a pure function (`#[must_use]`) with typed return value (`FiveQVerdict` + `FiveQDimensions`). The five dimensions (structure, arc, foreshadow, pacing, hook) are intentionally different from the finalize 五問, which evaluates the drafted chapter rather than the outline. This separation is correct.

The `FiveQDimensions` struct uses five booleans with `#[allow(clippy::struct_excessive_bools)]` — the inline justification ("a state machine would obscure the per-dimension breakdown") is reasonable for this DTO.

### `LlmExtractOutcome` Refactoring

The migration from a two-variant enum (`Candidates`/`Fallback(&'static str)`) to a three-variant enum (`Candidates`/`WorkerUnavailable`/`CapabilityError(String)`) is the most impactful architectural improvement in this diff:

1. **WorkerUnavailable** vs **CapabilityError** are now distinct — callers can log at different severities (debug vs warn) and make different decisions.
2. **CapabilityError** carries a `String` instead of a `&'static str`, allowing dynamic error messages from the capability layer.
3. The shared `run_llm_extract` function eliminates the duplicate capability invocation logic that previously existed in `LlmExtractTask::evaluate` (closes R-V151Q3-W001).
4. The wrapper `extract_via_llm` preserves the review-time hook's call signature with no breakage.

The `LlmExtractTask::evaluate` method now delegates to `run_llm_extract`, but its return type signature changed from `Result<Vec<KbCandidate>, GraphError>` to `Result<LlmExtractOutcome, GraphError>`. Since the task is dead code in production (S-003), this is a safe change.

### CAS Pattern Reuse (`mark_auto_promoted_in_tx_with_cas`)

The new function mirrors `mark_confirmed_in_tx_with_cas` (V1.51 T-B P1) precisely:
- Same version guard (`AND version = ?`)
- Same `version = version + 1` increment
- Same disambiguation logic (re-read row on `rows_affected == 0` to distinguish already-confirmed from stale version)
- Adds three audit columns (`auto_promoted_at`, `auto_promoted_reason`, `auto_promoted_by`) in the same atomic UPDATE

The caller in `kb_adopt_auto` correctly captures `candidate.version` from the read path and passes it to the CAS function, closing the TOCTOU window.

### Migration Idempotency

`202606190002_kb_extract_jobs_auto_promote.sql` adds three nullable TEXT columns (`auto_promoted_at`, `auto_promoted_reason`, `auto_promoted_by`). The migration is:
- **Additive only**: no destructive changes, no data backfill needed, existing rows default to NULL.
- **Idempotent**: re-running would fail with "duplicate column" — standard SQL migration behavior, acceptable with the sequential migration runner.
- **Reversible**: dropping the three columns would restore the pre-V1.52 schema without data loss (the columns only carry audit metadata).

### Spec ↔ Code Alignment

| Spec Section | Code Evidence | Alignment |
|---|---|---|
| `workflow-profile.md` §5.1.1 (outline 五问 gate) | `preset.yaml` `outline_review` state + `outline-exit.md` LLM prompt + `outline_five_q_check` heuristic | ✓ 6 states, `outline_review` between `outline_chapter` and `draft_chapter`, five dimensions match |
| `quality-loop.md` §5.6 (auto-promote) | `kb.rs` `kb_adopt_auto` + `mark_auto_promoted_in_tx_with_cas` | ✓ 5 predicates match, per-candidate tx isolation, CAS guard, audit log path |
| `cli-spec.md` §6.2G.1 (adopt --auto) | `kb.rs` `WorldKbCommand::Adopt` clap args + handler dispatch | ✓ `--auto` requires `--world-ref`, positional `extract_job_id` is `required_unless_present`, JSON output schema matches |

### Names Consistency

- `FiveQDimensions`, `FiveQVerdict`, `outline_five_q_check` — consistent prefix, clear intent.
- `LlmExtractOutcome`, `run_llm_extract`, `extract_via_llm` — consistent naming with the existing `LlmExtractTask`.
- `mark_auto_promoted_in_tx_with_cas` — follows the existing naming convention (`mark_*_in_tx_with_cas`).
- `auto_promoted_at`, `auto_promoted_reason`, `auto_promoted_by` — consistent column prefix, mirrors `promotion_status` lifecycle pattern.

### Responsibilities Not Over-Mixed

- **Gate logic** (`outline_five_q_check`) is in `quality_loop.rs` alongside the finalize quality checks — correct placement.
- **Preset integration** is in `preset.yaml` + `outline-exit.md` — no Rust code changes needed for the state machine wiring.
- **DAO updates** (`mark_auto_promoted_in_tx_with_cas`) is in `kb_extract_job.rs` — correct separation from the CLI handler in `kb.rs`.
- **CLI surface** is in `kb.rs` `WorldKbCommand::Adopt` — the dispatch between manual and auto paths is clean.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

The architecture is coherent: the outline 五问 gate cleanly extends the existing `llm_judge` infrastructure, the `LlmExtractOutcome` refactoring properly unifies the two extraction paths, and the auto-promotion CAS pattern faithfully mirrors the V1.51 per-row OCC baseline. Spec overlay bodies are aligned with the code. No blocking issues found.

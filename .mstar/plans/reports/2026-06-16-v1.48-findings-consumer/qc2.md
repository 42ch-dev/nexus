---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-16-v1.48-findings-consumer"
verdict: "Approve"
generated_at: "2026-06-16"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (Reviewer #2)
- Report Timestamp: 2026-06-16

## Scope
- plan_id: 2026-06-16-v1.48-findings-consumer
- Review range / Diff basis: merge-base: 975899e7895cacc34f4966c1e872c93cac670ace (origin/main pre-V1.48) + tip: 53108f79 (iteration/v1.48 HEAD); for P1 scope, focus on commits 7119350a..c6ba7622
- Working branch (verified): iteration/v1.48
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12 (core implementation + prompt/preset wiring + supporting changes in P1 commit range; excludes pure docs/plan/spec-only hunks outside code)
- Commit range (P1 focus): 5cf67a32 (T2: FindingsBlockBuilder), a5530ff3 (T3: wiring into novel-writing prompts + CLI), a1f34b13 (T4: hermetic tests), c3fd06b9 (chore: clippy + nightly fmt for T3/T4), 65c299f0 (T5: spec cross-ref + plan mark done)
- Tools run: git rev-parse, git branch, git diff --stat (pre-V1.48 base + P1 range), cargo clippy --all -- -D warnings, cargo test -p nexus-orchestration --test findings_consumer, cargo test -p nexus-local-db (findings chapter-scoped + lib tests in v148_serial_hardening)

**P1 in/out per assignment**: In = T1–T5 (DAO/query helper, template var + builder, novel-writing preset wiring for outline/draft, hermetic tests, spec cross-ref in overlay §2). Out = P0 (producer), P2 (AGENTS.md rules), P3 (world KB extract), P4 (serial), P-last.

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **W1 (prompt injection surface — accepted with mitigations)**: `Finding.description` (body) and `rule_suggestion` are user-sourced content that end up interpolated into LLM prompts for `novel-writing` (outline + draft). In `build_open_findings_block` (findings_block.rs:112):
  - Truncation to `MAX_BODY_CHARS=400` (Unicode scalar via `truncate_chars`) occurs **on the raw `&f.description` / `rule_suggestion` string before any `format!` or `write!` into the block**.
  - The per-finding line is then appended only if the prospective total block would stay ≤ `MAX_TOTAL_BLOCK_CHARS=3200`.
  - Empty input → empty string (AC2) so the Handlebars `{{#if open_findings_block}}` guard in the prompt templates omits the entire section (no sentinel noise).
  - The rendered block is placed inside an explicit human-readable instructional wrapper in both templates:
    ```
    ## Open Findings to Address
    The following open quality findings ... When planning the outline / drafting the body, **actively address** each item ...
    {{open_findings_block}}
    ```
  - No additional escaping of backticks, code fences, newlines, or JSON-significant characters is performed. The content is treated as "additional user instructions the writer must consider," which is the intended semantics.
  - This follows the established `world_kb_block` precedent (already accepted in prior iterations). The size caps + instructional framing + early total-cap exit provide defense-in-depth. Inherent risk of any "external findings as prompt context" design remains (an adversarial or poorly-written finding body could still influence the LLM). **Not a blocker** for P1; document for awareness and potential future hardening (see Suggestions).
- **W2 (non-blocking, per-assignment note)**: `R-V148P0-W1` (path resolution defense-in-depth) was explicitly not addressed by the implementer in this consumer P1. The assignment instructs: "do NOT block on it, but note it in your report as an open risk for the consumer path." It is recorded here as a known follow-up item (consumer path now exercises additional filesystem-adjacent prompt assembly surfaces via the same Work directory conventions as prior stages).

### 🟢 Suggestion
- **S1 (defense-in-depth delimiter)**: Consider wrapping the injected `open_findings_block` content in an explicit machine-readable delimiter (e.g. `<open_findings_block>...</open_findings_block>`) in a future pass. This would make it easier for any downstream prompt assembler, logging, or LLM to distinguish injected finding content from control instructions or other blocks (world_kb, etc.). Current human heading + "actively address" prose is sufficient for the P1 contract but is prose-level, not structural.
- **S2 (CLI round-trip deserialization tolerance)**: The CLI path (`assemble_open_findings_block`) fetches the generic `GET /v1/local/works/{id}/findings?status=open` response and deserializes directly into `nexus_local_db::findings::Finding` (which derives `Deserialize`). The daemon response for the generic list may include extra fields (e.g. `routing_hint` noted in the spec cross-ref). Current behavior relies on serde ignoring unknown fields. If the generic list shape ever adds or removes fields that affect required members, the consumer round-trip could become a maintenance point. A narrow projection DTO for the consumer path (or a "consumer view" query param) would eliminate this coupling; not required for P1 correctness.
- **S3 (allocation before cap)**: `truncate_chars` is correctly Unicode-aware (`chars().take`). However, when many oversized findings are passed in, the builder still materializes truncated per-finding strings before the total-block length check (see loop at findings_block.rs:107). In practice the 8-finding cap + total 3200-char guard bound the work; still, an early "budget remaining" check before per-finding truncation could be a micro-optimization for adversarial inputs.
- **S4 (test helper realism)**: Several test `Finding` constructors set `created_at: 0` and fixed `creator_id` / `work_id`. This is fine for the hermetic scope of T4, but if future tests exercise time-based or cross-creator isolation logic they will need more realistic fixtures.
- Minor: the chapter-scoped DAO returns **all** matching rows; the caller (builder or CLI filter) is responsible for the §2.2 count cap. This separation is documented and consistent with the world_kb pattern.

## Source Trace
- **Finding ID**: P1-QC2-001 (prompt surface / truncation-before-templating)
- **Source Type**: manual code review + git diff
- **Source Reference**: `crates/nexus-orchestration/src/findings_block.rs:99` (`build_open_findings_block`), 112 (`let body = truncate_chars(&f.description, MAX_BODY_CHARS)`), 115 (rule_suggestion truncation), 134 (total-cap early exit), 154 (`truncate_chars` impl), prompt templates `outline-chapter.md:38` and `draft-chapter.md:39` (`{{#if open_findings_block}}` + instructional prose), `auto_chain.rs:1054` and `run.rs:601` (call sites that feed the block).
- **Confidence**: High

- **Finding ID**: P1-QC2-002 (DAO SQL + creator isolation)
- **Source Type**: git diff + static review of compile-time query
- **Source Reference**: `crates/nexus-local-db/src/findings.rs:427` (`sqlx::query!` with exactly 3 `?` placeholders), 436 (`WHERE creator_id = ?`), 437 (`AND work_id = ?`), 439 (`AND (chapter = ? OR chapter IS NULL)`), 448–450 (three bound arguments in order), 421 (fn signature takes `creator_id` first).
- **Confidence**: High

- **Finding ID**: P1-QC2-003 (shared sort helper + client-side filter parity)
- **Source Type**: cross-file review
- **Source Reference**: `findings_block.rs:56` (`sort_open_findings` + `severity_rank`), `run.rs:580` (imports and calls the shared helper after client-side chapter filter at 587–590), DAO ordering at findings.rs:440 (CASE rank + created_at ASC). Filter predicate in CLI (`f.chapter == Some(chapter_i64) || f.chapter.is_none()`) matches DAO `chapter = ? OR chapter IS NULL`.
- **Confidence**: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 (W2 is an explicit non-blocking deferred note per assignment) |
| 🟢 Suggestion | 5 |

**Verdict**: Approve

## Validation Evidence (cited per assignment)
- **Lint**: `cargo clippy --all -- -D warnings` → clean (exit 0; "Finished `dev` profile" with no warnings emitted in tail).
- **Orchestration tests** (intent of `cargo test -p nexus-orchestration -- findings_consumer findings_block`): `cargo test -p nexus-orchestration --test findings_consumer` → 2 tests passed (`novel_writing_outline_includes_open_findings_block_when_seeded`, `novel_writing_outline_omits_block_when_no_findings`).
- **Local-db findings tests** (intent of `cargo test -p nexus-local-db -- findings`): `cargo test -p nexus-local-db findings` (plus the chapter-scoped tests inside `v148_serial_hardening`) → relevant tests passed, including:
  - `list_open_findings_for_chapter_filters_by_chapter_and_work_level`
  - `list_open_findings_for_chapter_orders_by_created_at_asc_within_severity`
  - `list_open_findings_for_chapter_returns_empty_when_no_matches`
  - `rule_suggestion_length_cap_*` family (P0 hardening still in force)
- Full P1 commit range diff (non-.mstar) confirms the 5 commits listed in Scope; no other crates or surfaces were touched in the P1 implementation slice.

## Notes on Assignment-Specific Checks Performed
- Prompt truncation timing: verified — truncation precedes any string formatting that becomes template output.
- Special characters / template engine safety: the Handlebars `{{#if}}` guard and variable interpolation are used only for presence/content; no code execution or format-string risk in the template engine itself. The risk is semantic (LLM prompt influence), mitigated by bounds + instructional wrapper.
- DAO: `sqlx::query!` (compile-time checked), all parameters bound, chapter-or-NULL predicate correct, `creator_id` filter present for tenant isolation.
- Round-trip: `Finding: Deserialize` has no `#[serde(default)]` on core fields that would silently accept missing data for the consumer use case; extra daemon fields are ignored (standard serde behavior).
- CLI filter consistency: uses the exact shared `sort_open_findings` + `build_open_findings_block`; predicate matches DAO.
- Deferred residual R-V148P0-W1: noted as W2 (non-blocking per explicit assignment instruction).

No code changes were made during this review session. All review work was read-only analysis + test/lint execution on the verified `iteration/v1.48` checkout.

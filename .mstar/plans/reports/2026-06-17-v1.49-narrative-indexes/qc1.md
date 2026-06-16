---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-17-v1.49-narrative-indexes
verdict: Approve
generated_at: 2026-06-17T23:30:00+08:00
review_range: eb75a73d..1fee7ada
working_branch: iteration/v1.49
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-17T00:00:00Z

## Scope
- plan_id: `2026-06-17-v1.49-narrative-indexes`
- Review range / Diff basis: `3630a4e5..f448b658`
- Working branch (verified): `iteration/v1.49`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 13 (11 in-scope implementation/test files + completion report + status.json residual additions)
- Commit range: `3630a4e5..f448b658` (4 P1 feature commits + 1 merge)
- Tools run: `git diff` / `git log`, `cargo check -p nexus-orchestration` (clean), `Read`/`Grep` on all in-scope files

### Verification (cwd / branch / range)
- `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus` ✓
- `git branch --show-current` → `iteration/v1.49` ✓
- `git rev-parse HEAD` → `d78d240b` (matches Assignment) ✓
- `git diff 3630a4e5...f448b658 --stat` → 13 files, +1456/-1 ✓

## Architecture Assessment (focus areas)

1. **Module size / separation of concerns (919 lines)** — Acceptable for MVP. The file
   has clear section separators (Canonical geometry → Row types → Parser → Serializer →
   Id allocation → Outline extraction → Promotion → Summary → Tests). ~317 lines are tests;
   ~600 lines of implementation. Single-file is the right call for V1.49's surface area. See
   S-1 for the V1.50 split trigger.

2. **Schema decision (5-col template vs 4-col overlay)** — Correct call. The runtime
   implements the scaffolded template's 5-column shape (`ID | Description | Planted | Paid off
   | Status`), which is the on-disk ground truth that `NovelProjectScaffold` writes and that
   the round-trip / scaffold tests must reproduce. The overlay §3 4-col summary is a doc-level
   abstraction; reconciliation deferred to P5 via R-V149P1-01 is appropriate (no runtime gap).
   **However** — the `status` field is `String` instead of a typed enum. See W-1.

3. **`promote_outline_to_index` atomicity** — Sound for the single-writer daemon model.
   temp-write + rename (`atomic_write`) prevents torn writes on crash. Advisory-lock deferral
   to V1.50 (R-V149P1-01) is acceptable for pre-1.0 single-user. See S-4 for the orphaned-temp
   edge case.

4. **Post-outline hook placement** — Correct. The promotion hook runs in
   `on_schedule_terminal` for `novel-writing` schedules, BEFORE
   `process_auto_chain_after_terminal`, so the updated index is visible when the next chapter's
   outline prompt is assembled. Best-effort + non-blocking: errors logged at `warn!` and do
   NOT fail the terminal transition (mirrors the `persist_review_findings_for_schedule`
   pattern). Per-outline errors are also isolated (logged + counted as zero). No issues.

5. **`build_preset_input` integration** — Clean. `foreshadowing_summary` defaults to empty
   string when the file is missing/empty or `workspace_dir`/`work_ref` are absent. The
   `{{#if foreshadowing_summary}}` Handlebars guard correctly treats empty string as falsy, so
   no empty-sentinel noise. The always-insert (even when empty) pattern is correct for
   strict-mode template safety and is documented in the code comment. No empty-string-vs-None
   confusion risk.

6. **`sync_module` skip invariant** — Solid. Both `foreshadowing.md` AND `event-index.md` are
   in `SKIP_FILES` (verified: `const SKIP_FILES: &[&str] = &["README.md", "foreshadowing.md",
   "event-index.md"];`). The regression test locks both the explicit boundary (stray copy in
   `Stories/`) and the canonical-location boundary (`Outlines/` is never scanned). The skip is
   at the right layer (filename filter during chapter discovery).

7. **Preset version stays at 8** — Policy-aligned. Per the documented versioning policy
   (R-V139P5-W-4 closure in `research/preset.yaml` header): "Non-breaking additions (new
   optional fields, comment changes) may keep the same version." `foreshadowing_summary` is a
   new optional var guarded by `{{#if}}` — non-breaking. Keeping version=8 is correct. See
   S-2 for the pre-existing V1.48 P1 inconsistency.

8. **Public API surface** — Appropriate for an internal workspace crate. All public functions
   have `///` doc comments. `NOVEL_WRITING_PRESET_ID` is a well-documented new const with a
   frozen-value regression test. See S-5 for the minor `next_e_id` premature-exposure note.

9. **Test coverage** — Strong. 25 lib tests cover parser (empty/full/placeholder/scaffold),
   serializer round-trip, id allocation (sequential + gap-preserving), section extraction,
   promotion (new/idempotent/conflict/allocate/atomic/noop-mtime), and summary
   (empty/populated). 3 `build_preset_input` tests cover populated/absent/no-workspace. 1
   `sync_module` regression. **Gap**: no test for malformed status values or the W-2 noise-bullet
   risk.

## Findings

### 🟡 Warning

#### W-1: `ForeshadowingRow.status` is `String` instead of a typed enum — weak type safety

**Location**: `crates/nexus-orchestration/src/narrative_index.rs:82`

The overlay (`narrative-indexes.md §3`) defines a closed status vocabulary
`planned | buried | paid_off`. The struct field comment even documents this:
`/// `planned` | `buried` | `paid_off` (overlay §3 vocabulary).``

But the field is `pub status: String`, so:
- No parse-time validation — typo'd values (`Planned`, `planed`, `paid-off`, `paid off`, `Payed off`)
  persist silently through round-trip.
- `read_foreshadowing_summary` (line 588) defaults empty → `"planned"` but does NOT validate
  non-empty values, so any string flows into the prompt verbatim.
- Future status-based logic (e.g. filtering "active" foreshadowing for the draft prompt, or
  computing "paid off this chapter" transitions in V1.50) will have no exhaustiveness check
  and no compile-time guarantee that all three variants are handled.

This is a maintainability risk: the closed vocabulary is documented but unenforced, so the
type system can't catch drift between the overlay spec, the prompt contract, and the runtime.

**Fix**: Introduce a `ForeshadowingStatus` enum (or `Cow<'static, str>`-backed newtype) with:
- `FromStr` — lenient parse (canonicalize whitespace/case, preserve unknown values as
  `Other(String)` for forward-compat with user-edited files).
- `Display` — canonical form on serialize (`planned` / `buried` / `paid_off`).
- `ForeshadowingRow::status: ForeshadowingStatus`.

At minimum, add a `ForeshadowingRow::status_is_valid() -> bool` validator and call it in
`parse_foreshadowing_index` with a `tracing::warn!` on unknown values.

#### W-2: Promotion allocates new F### ids for ANY bullet without an `F###` prefix — index corruption risk from non-declaration bullets

**Location**: `crates/nexus-orchestration/src/narrative_index.rs:463-469`

```rust
// A bullet with no F### prefix and non-empty text → allocate new id.
if body.starts_with("- ") || body.starts_with("* ") {
    return Some(FDeclaration {
        id: None,
        description: stripped.to_string(),
    });
}
```

The prompt contract (`outline-chapter.md` lines 70-73) instructs authors to **always** use the
`F###: <description>` form. The allocation form (`- a brand new seed` → allocate) is an
undocumented runtime extension that goes beyond the prompt contract.

Risk: any prose bullet in the `## Foreshadowing Touched (F###)` section gets silently
allocated as a new `F###` id. The "No foreshadowing items touched" sentinel (line 428) is
skipped, but other common LLM-authored or human-authored non-declaration bullets are NOT:
- `- Note: chapter is darker than planned` → allocated as `F003: Note: chapter is darker than planned`
- `- TODO: resolve the locket payoff next chapter` → allocated as `F004: TODO: ...`
- `- (no new items, just touching F001)` → allocated as `F005: (no new items, ...)`

This silently corrupts the foreshadowing index with spurious ids. The consequence is severe
(persistent index pollution) and the fix is simple.

**Fix** (pick one):
- (a) Require an explicit allocation marker: `- new: <description>` or `- F???: <description>`.
- (b) Add a deny-list for common non-declaration prefixes (`Note:`, `TODO:`, `FIXME:`, `NB:`,
  `N.B.`, lines starting with `(`).
- (c) Only allocate when the section contains at least one `F###`-prefixed item (heuristic:
  the section is being used for declarations, so un-prefixed bullets are likely declarations
  too — but this is weaker than (a) or (b)).

At minimum, add a regression test documenting the current behavior and the expected disposition
of noise bullets.

### 🟢 Suggestion

#### S-1: `narrative_index.rs` at 919 lines — plan submodule split for V1.50

Single-file is acceptable for MVP (clear sections, ~317 lines are tests). When E### CRUD +
advisory locks + status transitions land in V1.50, the file will exceed the comfortable
single-file threshold. Plan a `narrative_index/{parser,serializer,promotion,summary}.rs`
submodule split at that point. Non-blocking for V1.49 P1.

#### S-2: Preset versioning inconsistency between V1.48 P1 and V1.49 P1

V1.49 P1 keeps version=8 — aligned with the documented policy (R-V139P5-W-4 closure:
"Non-breaking additions may keep the same version"). However, V1.48 P1 bumped 7→8 for the
analogous `open_findings_block` addition, citing "versioned up so pre-V1.48 schedules are
correctly identified" (`auto_chain.rs:1384-1388`). Both decisions are individually defensible,
but the team's practice is inconsistent: one additive optional var triggered a bump, another
did not. Reconcile in a future hygiene pass (either both bump or neither bumps). Not a V1.49
P1 regression — the V1.48 P1 precedent predates this plan.

#### S-3: GFM alignment separators not recognized by `is_separator_row`

**Location**: `crates/nexus-orchestration/src/narrative_index.rs:217-225`

`is_separator_row` only recognizes plain `| --- |` style separators. GFM alignment markers
(`:---`, `:---:`, `---:`) are treated as data rows because `cell.trim().trim_matches('-')`
leaves the `:` behind, failing the `.is_empty()` check. This breaks table parsing if a user
manually edits `foreshadowing.md` with aligned separators. Works for the scaffolded template
(plain `---`) but reduces robustness for human-edited files. **Fix**: strip leading/trailing
`:` from each cell before the dash check.

#### S-4: `atomic_write` orphaned temp file on crash

**Location**: `crates/nexus-orchestration/src/narrative_index.rs:557-564`

If the process crashes between `std::fs::write(&tmp, ...)` and `std::fs::rename(&tmp, path)`,
the temp file (`foreshadowing.md.tmp`) is orphaned. The next promotion overwrites it
(deterministic path), so it is self-healing — but the orphaned file is never cleaned up on
daemon startup, and a crash-loop could accumulate other index temp files. Low-impact for MVP.
Consider a startup sweep (`rm **/*.md.tmp`) or a unique temp suffix (PID) in V1.50.

#### S-5: `next_e_id` public exposure before E### writer ships

**Location**: `crates/nexus-orchestration/src/narrative_index.rs:318-321`

`next_e_id` is `pub` but the E### writer is deferred to V1.50. It is currently consumed only
by the `parse_event_index_reads_populated_table` test. Acceptable for an internal workspace
crate (no external consumers), but consider documenting the V1.50 consumer or gating behind
`#[cfg(test)]` until the writer lands.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning | `narrative_index.rs:82` (`status: String`); overlay §3 vocabulary; `read_foreshadowing_summary:588` | High |
| W-2 | manual-reasoning + git-diff | `narrative_index.rs:463-469` (`parse_declaration_line` allocation form); `outline-chapter.md:70-73` (prompt contract) | High |
| S-1 | manual-reasoning | `narrative_index.rs` line count (919); V1.50 roadmap (R-V149P1-01 advisory-lock defer) | High |
| S-2 | git-diff + doc-rule | `auto_chain.rs:1384-1388` (V1.48 bump rationale); `research/preset.yaml:22-27` (R-V139P5-W-4 policy) | High |
| S-3 | manual-reasoning | `narrative_index.rs:217-225` (`is_separator_row`) | Medium |
| S-4 | manual-reasoning | `narrative_index.rs:557-564` (`atomic_write`) | Medium |
| S-5 | manual-reasoning | `narrative_index.rs:318-321` (`next_e_id` pub); V1.50 E### writer defer | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

Two unresolved Warnings (W-1 untyped status, W-2 eager allocation) block approval. Both are
maintainability/correctness-hardening issues with simple fixes and should be resolved before
the plan advances to QA. The architecture is otherwise sound: the hook placement, atomicity
model, preset-input integration, and sync-invariant are all well-designed and well-tested.
The 5-column template schema decision is correct (template is ground truth); the status-typing
gap (W-1) is the one place where the runtime diverges from the overlay's closed vocabulary.

## Revalidation

**Re-review kind**: Targeted re-review (Reviewer 1 of 1; only QC1 raised blocking findings).
**Re-review date**: 2026-06-17T23:30:00+08:00
**Re-review range / Diff basis**: `eb75a73d..1fee7ada` (fix commit `3f2efc03` + completion
report `480a7663` + merge `1fee7ada`; equivalent to `git diff eb75a73d...1fee7ada`).
**Working branch (verified)**: `iteration/v1.49` @ `1fee7ada`.
**Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`.
**Files re-reviewed**: 3 (2 implementation/test + 1 completion report).

### Verification (cwd / branch / range)

- `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus` ✓
- `git branch --show-current` → `iteration/v1.49` ✓
- `git rev-parse HEAD` → `1fee7ada` (matches Assignment) ✓
- `git diff eb75a73d...1fee7ada --stat` → 3 files, +510/-95 (matches Assignment) ✓

### W-1 disposition: RESOLVED ✅

The typed `ForeshadowingStatus` enum (`crates/nexus-orchestration/src/narrative_index.rs:78`)
is well-designed and directly addresses the original W-1 maintainability risk:

1. **Variant naming** — `Planned | Buried | PaidOff` maps 1:1 to the overlay §3 vocabulary
   `planned | buried | paid_off` via `as_canonical_str()` (lines 90-96). Correct.
2. **Case-insensitive `FromStr`** — tolerates author typos (`PLANNED`, `Buried`, `PAID_OFF`,
   surrounding whitespace); documented in the enum docstring (lines 73-76); locked by
   `foreshadowing_status_fromstr_is_case_insensitive`. Appropriate policy for hand-edited
   `foreshadowing.md` files. The implementer chose a strict closed enum (reject unknown)
   rather than the `Other(String)` forward-compat variant suggested in wave 1 — this is a
   sound alternative: it forces typo correction and the graceful degradation in
   `read_foreshadowing_summary` (warn + None) prevents prompt-injection breakage.
3. **Canonical `Display`** — always emits lowercase + underscore (`planned` / `buried` /
   `paid_off`); locked by `foreshadowing_status_display_is_canonical_lowercase`. Correct
   round-trip target.
4. **`IndexParseError::InvalidStatus { row_index, value }`** — carries the zero-based
   data-row index and the rejected value, giving the author enough context to locate and
   fix the offending cell. Error type is `pub` and implements `std::error::Error`.
   Locked by `parse_foreshadowing_index_rejects_unknown_status` (asserts row_index=1,
   value="Payed off").
5. **`Result<Vec<_>, IndexParseError>` propagation** — verified all callers:
   - `promote_outline_to_index` (line 656): uses `?` — `IndexParseError: std::error::Error`
     converts via anyhow's blanket impl. ✓
   - `read_foreshadowing_summary` (line 724): explicit `match` with `warn!` + `None`
     graceful degradation. ✓
   - `auto_chain.rs:1217` caller of `promote_outline_to_index`: explicit `match` with
     `warn!` non-fatal handling. ✓
6. **Public API** — `ForeshadowingStatus`, `IndexParseError`, `ForeshadowingStatusError`
   are all `pub` items in `pub mod narrative_index` (`lib.rs:11`). Accessible via
   `nexus_orchestration::narrative_index::ForeshadowingStatus`. No additional `lib.rs`
   re-export needed (module-path access is the established pattern). ✓

### W-2 disposition: RESOLVED ✅

The explicit `F###` token requirement (`parse_declaration_line`, lines 559-604) directly
addresses the original W-2 index-corruption risk:

1. **Policy enforced** — `parse_declaration_line` only yields a declaration when the
   (bullet-stripped) line starts with `F` + digits + (`:` or whitespace). Prose bullets
   (`- Note: ...`, `- TODO: ...`, `- (no new items ...)`) return `None`. The function
   docstring (lines 525-544) documents the policy with concrete examples.
2. **Promotion no longer allocates for prose** — `promote_outline_to_index` (lines 660-683)
   iterates only over declarations from `extract_inline_f_declarations` (which now requires
   the F### token). A defensive `let Some(id) = decl.id else { continue; }` guard (lines
   665-667) documents that it is unreachable for well-formed extraction output and protects
   against future regressions. Comment explains the rationale.
3. **Token-matching robustness** — handles leading/trailing whitespace (`.trim()`), bullet
   prefix (`- ` / `* `), `:` and space delimiters, empty-description rejection, and id-only
   refs (e.g. bare `F001` with no description → skipped).
4. **Tests** — 3 new W-2 tests, all in the correct file (`narrative_index.rs` test module):
   - `extract_inline_f_declarations_ignores_bullets_without_f_token` (covers Note / TODO /
     parenthetical prose → ignored; only F### bullet promoted).
   - `extract_inline_f_declarations_handles_bullet_with_existing_f_id` (F### space form).
   - `promote_outline_to_index_does_not_allocate_for_prose_bullets` (end-to-end: only F002
     promoted; F003 NOT allocated for prose bullets).

### Scope discipline

Surgical. Only 2 implementation/test files changed (`narrative_index.rs` +469/-95,
`stage_gates.rs` +4/-1 test-only enum update). Both scoped strictly to W-1 + W-2. No
piggyback refactors, no unrelated changes. The `FDeclaration.id` stays `Option<String>`
(documented decision to minimize struct-signature churn; defensive guard in promotion).

### CI gates (re-verified on `iteration/v1.49 @ 1fee7ada`)

| Gate | Result |
|------|--------|
| `cargo +nightly fmt --all --check` | clean (exit 0) |
| `cargo clippy -p nexus-orchestration -- -D warnings` | clean (exit 0) |
| `cargo clippy --all -- -D warnings` (CI equivalent) | clean (exit 0, 22.89s) |
| `cargo test -p nexus-orchestration --lib narrative_index` | 31 passed; 0 failed |
| `cargo test -p nexus-orchestration --lib stage_gates` | 52 passed; 0 failed |
| `cargo test -p nexus-orchestration --test novel_project_init` | 22 passed; 0 failed |
| `cargo test -p nexus-orchestration --test e2e_novel_writing` | 11 passed; 0 failed |

Test count delta: narrative_index 25 → 31 (+8 new W-1/W-2 tests, −2 obsolete String-status
tests). All integration tests unaffected.

### New findings (re-review scope)

#### 🟢 Suggestion

##### S-RV1: `F###` token prefix matching is case-sensitive — minor inconsistency with status parsing

**Location**: `crates/nexus-orchestration/src/narrative_index.rs:578` (`stripped.strip_prefix('F')`)

The `F###` token matcher requires an uppercase `F` prefix, while `ForeshadowingStatus::from_str`
is case-insensitive. An LLM-authored outline that emits lowercase `f001: the locket` would be
silently ignored (not promoted). Low risk — the prompt contract (`outline-chapter.md`) uses
uppercase `F###`, and the W-2 policy is specifically about preventing over-allocation, not
case tolerance. Non-blocking; consider case-insensitive `F`/`f` prefix in a future hardening
pass if LLM case drift is observed.

##### S-RV2: Empty status cell is now a hard parse error (behavior change from wave-1 default)

**Location**: `crates/nexus-orchestration/src/narrative_index.rs:255` (`parse_foreshadowing_index`)

Wave-1 code defaulted empty status → `"planned"` in `read_foreshadowing_summary`. The typed
enum now treats an empty status cell as `IndexParseError::InvalidStatus { value: "" }`. This is
a deliberate, correct design choice (stricter validation per W-1 intent) and the graceful
degradation is sound (`read_foreshadowing_summary`: warn + None; `promote_outline_to_index`:
non-fatal warn). The only impact is on hand-edited files with a real `F###` row but an empty
status cell — the summary will be omitted rather than defaulting to `planned`. The scaffolded
template's placeholder row (`| | | | | |`) is unaffected (empty ID → skipped before status
check). Non-blocking; documented here for traceability. If hand-edit UX becomes a concern,
a future enhancement could default empty→`planned` with a `warn!`.

### Updated verdict

| Severity (re-review scope) | Count |
|----------------------------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 (S-RV1, S-RV2 — non-blocking) |

**Verdict**: **Approve**

Both blocking Warnings (W-1, W-2) are fully resolved. The fix is well-designed, well-tested
(8 new tests), surgically scoped, and all CI gates are green. The 2 new Suggestions are
minor hardening observations that do not block approval or QA progression. Residuals
R-V149P1-03 (W-1) and R-V149P1-04 (W-2) are ready for PM closure.

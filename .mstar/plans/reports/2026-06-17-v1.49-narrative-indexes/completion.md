# Completion Report v2 — P1 (narrative-indexes)

- plan_id: `2026-06-17-v1.49-narrative-indexes`
- owner: `@fullstack-dev`
- Working branch used: `feature/v1.49-narrative-indexes`
- Worktree path: `.worktrees/v1.49-narrative-indexes`
- Base: `iteration/v1.49 @ 3630a4e5`
- Commits:
  - `9e73a047` feat(orchestration/v1.49): T1+T2 narrative index parser, serializer, id allocation, outline promotion
  - `2ef52406` feat(orchestration/v1.49): T3 wire narrative index into novel-writing produce path
  - `01425b8c` test(orchestration/v1.49): T4 sync_module skip-invariant regression for narrative index files

## Cargo verification (last lines)

```
$ cargo +nightly fmt --all --check
(no output — clean)

$ cargo clippy -p nexus-orchestration -- -D warnings
    Finished `dev` profile [unoptimized + debug-a] target(s) in 0.24s

$ cargo test -p nexus-orchestration --test novel_project_init
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.19s

$ cargo test -p nexus-orchestration --lib narrative_index
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 603 filtered out; finished in 0.02s
```

Full-crate `cargo test -p nexus-orchestration` is green on a clean run (627 lib + all
integration binaries). One intermittent pre-existing flake (`review_report::
fallback_warn_includes_chapter_field`) is documented under Residual additions
(R-V149P1-02) — verified pre-existing on the base per the repo's "Pre-existing claim
verification protocol".

## Acceptance criteria (plan §4)

1. **Overlay §2-§3 row schema implemented for F###** — `crates/nexus-orchestration/src/narrative_index.rs`
   `parse_foreshadowing_index` / `serialize_foreshadowing_index` / `next_f_id` (+ E### read stub
   `parse_event_index`). Evidence: `9e73a047`; tests `serialize_then_parse_roundtrip_is_stable`,
   `parse_foreshadowing_index_handles_full_table`, `next_f_id_allocates_sequentially`,
   `parse_foreshadowing_index_parses_scaffolded_template_verbatim`.

2. **`novel-writing` outline step triggers promotion (post-outline hook) AND `read_foreshadowing_summary`
   read for prompt injection when non-empty** — promotion hook
   `auto_chain::promote_foreshadowing_for_schedule` wired into `on_schedule_terminal` for
   `novel-writing` schedules before `process_auto_chain_after_terminal` (`2ef52406`, supervisor.rs);
   `read_foreshadowing_summary` computed in `stage_gates::build_preset_input` and emitted as
   `preset.input.foreshadowing_summary`, consumed by `outline-chapter.md` + `draft-chapter.md` via
   `{{#if foreshadowing_summary}}`. Evidence: `2ef52406`; tests
   `build_preset_input_includes_foreshadowing_summary_when_index_populated`,
   `build_preset_input_foreshadowing_summary_empty_when_index_absent`.

3. **Existing scaffold tests pass; new promotion tests added** — 22/22 `novel_project_init` pass;
   new promotion/summary/declaration/section tests (25 in `narrative_index` lib). Evidence: `9e73a047`,
   `01425b8c`.

4. **`sync_module` still skips index files** — `SKIP_FILES` already contains both filenames and
   `discover_works` only scans `Stories/`; locked with dedicated regression
   `sync_module_skips_foreshadowing_index_file`. Evidence: `01425b8c`.

## What was delivered (per task)

- **T0** — Overlay + workflow-profile §4.2 read; row semantics recorded (see "Schema decision" below).
- **T1** — Index file parser/writer (F###) + E### read stub + `next_f_id`/`next_e_id`.
- **T2** — `extract_foreshadowing_section` + `extract_inline_f_declarations` + `promote_outline_to_index`
  (atomic temp+rename; idempotent; errors on conflicting-description duplicate per overlay §3.1).
- **T3** — (a) post-outline promotion hook in `on_schedule_terminal`; (b) read-for-prompt injection in
  `build_preset_input` → `preset.yaml` → prompt templates.
- **T4** — 25 `narrative_index` unit tests + 3 `build_preset_input` summary tests + 1 `sync_module` skip
  regression + `e2e_novel_writing` seed update.

## Schema decision (documented assumption)

The Draft overlay `narrative-indexes.md §3` summarizes the F### table as 4 columns
(`id | description | status | chapters`) while stating it is "aligned with embedded template". The
actual scaffolded template (`embedded-presets/novel-{writing,project-init}/templates/foreshadowing.md`,
byte-identical across presets) ships a **5-column** header `ID | Description | Planted | Paid off | Status`.
The runtime implements the **template's 5-column** shape — it is the on-disk ground truth that
`novel.project_scaffold` writes and that the round-trip / scaffold tests must reproduce. The overlay's
single `chapters` column is realized more precisely by `Planted` + `Paid off`; `Status` honours the
overlay's `planned | buried | paid_off` vocabulary. The overlay §3 table is to be reconciled at the P5
fold-into-`workflow-profile.md §4.6` step (R-V149P1-01). No runtime behavior gap.

Other documented assumptions:
- Inline declaration canonical form is `F###: description` (per `outline-chapter.md` prompt + overlay §3.1);
  space-delimited and bullet forms are also tolerated.
- Promotion concurrency relies on the single-writer daemon model documented on `NovelProjectScaffold`
  (atomic temp+rename prevents torn writes; no advisory lock added in this slice — see R-V149P1-01 note).
- Preset version stays at 8: `foreshadowing_summary` is an additive optional var guarded by `{{#if}}`,
  non-breaking per R-V139P5-W-4.
- The assignment's "call task tool with subagent: fullstack-dev" final instruction was **not** honored:
  `Delegation: forbidden` + the harness anti-recursion NEVER rule (`fullstack-dev` cannot dispatch
  `fullstack-dev`) require direct in-session execution. All work done directly as `fullstack-dev`.

## Residual additions

Two residuals added to root `residual_findings["2026-06-17-v1.49-narrative-indexes"]` in `.mstar/status.json`:

- **R-V149P1-01** (low, defer → V1.49 P5): overlay §3 4-col vs template 5-col schema reconciliation (doc-only).
- **R-V149P1-02** (low, defer → V1.50): pre-existing intermittent flaky test
  `review_report::fallback_warn_includes_chapter_field` (tracing-subscriber cross-binary race; verified
  pre-existing on `iteration/v1.49 @ 3630a4e5`).

## Risks / follow-ups

- The promotion hook scans all `Outlines/chapters/*-outline.md` on every `novel-writing` terminal event
  (idempotent by design). For Works with very many chapters this re-reads the index file per outline; an
  incremental (track promoted outlines) optimization is a V1.50 follow-up, not needed for MVP.
- Prompt-injection of `foreshadowing_summary` only fires when the caller sets `workspace_dir` in
  `WorkFields` (the CLI/daemon `stage advance` path does; the auto-chain enqueue path currently sets
  `workspace_dir=None`, so the summary is omitted there — the `{{#if}}` guard degrades gracefully). This
  mirrors the existing `rules_content` behavior and is acceptable for the file-first SSOT model.
- No DB mirror table for indexes (OUT-B O8) — explicitly a V1.49 non-goal (plan §3).

## Ready for QC tri-review: yes

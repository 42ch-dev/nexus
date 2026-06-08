---
report_kind: qc-consolidated
plan_id: "2026-06-08-v1.38-novel-writing-parameterization"
verdict: "Request Changes — fix W-1 (silent degradation) and W-2 (label duplication); accept others as residuals"
generated_at: "2026-06-08"
qc_wave: "initial"
active_wave_note: "Initial tri-review. Targeted re-review by qc-specialist after fix wave."
---

# QC Consolidated Report — V1.38 P1 Novel-Writing Parameterization

## Gate Verdict

**Request Changes** — QC1 raised 2 Warnings that block Approval. QC2 has 1 latent Warning and QC3 is clean. Fix-now scope: W-1 (silent CLI degradation when chapter context absent) + W-2 (duplicated chapter_label formatting). Other findings registered as residuals below.

## Reviewers

| Seat | Reviewer | Verdict | Critical | Warning | Suggestion | Report |
|------|----------|---------|----------|---------|------------|--------|
|1 | @qc-specialist | Request Changes |0 |2 |3 | [qc1.md](qc1.md) |
|2 | @qc-specialist-2 | Approve |0 |1 (latent) |4 | [qc2.md](qc2.md) |
|3 | @qc-specialist-3 | Approve |0 |0 |4 | [qc3.md](qc3.md) |

## Scope alignment (verified verbatim)

- `plan_id`: `2026-06-08-v1.38-novel-writing-parameterization`
- `Review range / Diff basis`: `merge-base(8e58890a, HEAD)..HEAD` on `iteration/v1.38` (commit `ad455ec5` brings in 5 feature commits).
- `Working branch (verified)`: `iteration/v1.38`
- `Review cwd (verified)`: `/Users/bibi/workspace/organizations/42ch/nexus`
- All three reports used the same scope; alignment gate passed.

## CI gate evidence (PM-verified)

- `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings` → exit0 (clean)
- `cargo test -p nexus-orchestration --lib stage_gates` →37 passed
- `cargo test -p nexus-orchestration --test e2e_novel_writing` →11 passed
- `cargo test -p nexus42 --test command_surface_contract` →43 passed
- `cargo +nightly fmt --all -- --check` → clean

## Findings consolidation

### 🟡 Warning (must address)

| ID | Source | Title | Decision |
|----|--------|-------|----------|
| **W-1** | QC1 | CLI `stage_advance` silently degrades to `None` chapter context when daemon `chapters[]` array is missing or selected chapter row is absent; templates declare `outline_path` and `body_path` as `required: true` so template render would fail. | **Fix now** |
| **W-2** | QC1 | `chapter_label` formatting (`format!("{ch_num:02}")`) is duplicated in `run.rs:992` (CLI) and `stage_gates.rs:616` (test helper); no shared source of truth. | **Fix now** (one-line helper) |
| **W-1 (QC2)** | QC2 | Pre-existing latent: when `next_chapter` returns `None` (novel-completion), `stage_advance` for "produce" still creates a schedule with empty chapter fields. Not introduced by this diff; surfaced by the new fields. | **Residual — defer** (pre-existing; doesn't regress; CLI completion UX is V1.39+ scope) |

### 🟢 Suggestion (non-blocking)

| ID | Source | Title | Decision |
|----|--------|-------|----------|
| **S-1** | QC1 | Frontmatter field documentation removed from `draft-chapter.md` without replacement | **Residual — defer** (template example block remains; non-blocking) |
| **S-2** | QC1 | `_deprecated/` prompt files still embedded in binary via `include_dir!` | **Residual — defer** (tech debt; non-blocking; spec already accepts `_deprecated/` for clarity) |
| **S-3** | QC1 | `outline_path` / `body_path` `required: true` may fail in non-CLI callers | **Residual — defer** (current callers all populate; defense-in-depth is a future hardening) |
| **S-1 (QC2)** | QC2 | `chapter_label` for chapter ≥100 yields "100" (not "0100") | **Residual — accept** (spec accepts 2-digit zero-pad for 1-99) |
| **S-2 (QC2)** | QC2 | O(n) linear scan over `chapters[]` in `stage_advance` | **Residual — accept** (typical n<100; negligible) |
| **S-3 (QC2)** | QC2 | Defense-in-depth path re-validation at render time | **Residual — defer** (work_chapters paths are trusted; engine doesn't perform filesystem writes) |
| **S-4 (QC2)** | QC2 | Quarantined files could be referenced by user presets | **Residual — accept** (documented intent; not blocking) |
| **S1 (QC3)** | QC3 | Repeated `if let Some(...)` pattern in `build_preset_input` could be a helper | **Residual — defer** (cosmetic; will be naturally DRY'd if more fields added) |
| **S2 (QC3)** | QC3 | `_deprecated/` files still embedded in binary | **Same as QC1 S-2** |
| **S3 (QC3)** | QC3 | `stage_advance` lacks audit logging for chapter context | **Residual — defer** (observability gap; non-blocking) |
| **S4 (QC3)** | QC3 | O(n) chapter lookup could be documented | **Same as QC2 S-2** |

## Fix-now scope (per fullstack-dev re-dispatch)

1. **W-1 (QC1)** — In `crates/nexus42/src/commands/creator/run.rs:990-1011` (the `stage_advance` extraction block), change the silent-degradation behavior. Approach:
 - When the daemon response lacks `chapters[]` array OR no chapter row matches the selected chapter, **return a user-facing error** before building the schedule. Use the existing CLI error pattern (e.g., `return Err(...)` from a `Result` return type, or `eprintln!` + `std::process::exit(1)` if the function returns `()`). 
 - Error message: `error: novel-writing schedule requires chapter context (outline_path, body_path). The daemon response is missing chapters[] or the selected chapter row. Re-run creator run status <work_id> to inspect, or re-seed the work via creator run start --init-preset novel-project-init.`
 - Update the existing CLI tests to assert the new error path is taken when chapters is missing/empty.
 - Alternative (acceptable but less preferred): keep silent degradation but change template `required: true` to `required: false` with documented defaults. PM prefers the first option (fail fast at CLI boundary).

2. **W-2 (QC1)** — Extract a `pub fn chapter_label(chapter: i32) -> String` helper. Place it in `crates/nexus-orchestration/src/stage_gates.rs` (or a new `chapter_label.rs` module). The helper should format with `format!("{chapter:02}")`. Import from both `run.rs:992` and the test helper in `stage_gates.rs:616`. Add a unit test that asserts:
 - `chapter_label(1) == "01"`
 - `chapter_label(9) == "09"`
 - `chapter_label(10) == "10"`
 - `chapter_label(100) == "100"` (2-digit pad for 1-99, then grows)

After fix, dispatch targeted re-review to `@qc-specialist` (QC1) only — covers W-1 + W-2.

## Residual registration (root `status.json.residual_findings`)

After Plan3 is marked Done, register these open items (severity per machine enum):

| ID | Title | Severity | Source | Owner | Target |
|----|-------|----------|--------|-------|--------|
| R-V138P1-01 | Pre-existing latent: `stage_advance` for "produce" creates schedule with empty chapter fields when `next_chapter=None` (novel-completion); surfaced by P1 fields but not introduced | low | QC2 W-1 | @fullstack-dev | V1.39+ completion UX |
| R-V138P1-02 | Frontmatter field documentation removed from `draft-chapter.md` without replacement | nit | QC1 S-1 | @fullstack-dev | backlog |
| R-V138P1-03 | `_deprecated/` prompt files still embedded in binary via `include_dir!`; not loaded but occupy space | low | QC1 S-2 / QC3 S2 | @ops-engineer | V1.39+ hygiene |
| R-V138P1-04 | `outline_path` / `body_path` `required: true` in templates — no default for non-CLI callers | low | QC1 S-3 | @fullstack-dev | V1.39+ hardening |
| R-V138P1-05 | `chapter_label` for chapter ≥100 yields "100" not "0100" (no fixed-width) | nit | QC2 S-1 | @fullstack-dev | backlog |
| R-V138P1-06 | O(n) linear scan over `chapters[]` in `stage_advance` (typical n<100, negligible) | nit | QC2 S-2 / QC3 S4 | @fullstack-dev | backlog |
| R-V138P1-07 | `stage_advance` lacks audit logging for chapter context extraction (observability gap) | low | QC3 S3 | @fullstack-dev | V1.39+ observability |

## Diff Scope Check (consolidated)

All3 reviewers confirmed no diff hunks touched the explicitly deferred boundaries or P0-only files:

- Auto-chain / DF-53 — NOT touched
- World KB / DF-63 — NOT touched
- Quality loop / DF-64/65/66/67 — NOT touched
- Multi-volume PK migration — NOT touched
- Platform publish — NOT touched
- Multi-work switch — NOT touched
- Selection pool — NOT touched
- P0 (`work_chapters.rs` selection logic, `is_work_completed`, `novel_chapter_transition.rs`, `WorkApiDto` enrichment, composite index migration) — NOT touched

Scope boundary holds.

## Next Steps (PM action)

1. Re-dispatch to `@fullstack-dev` with fix scope above (W-1 + W-2) on `iteration/v1.38` directly.
2. After fix commit lands, dispatch targeted re-review to `@qc-specialist` (`QC re-review: targeted — reviewers: qc-specialist`).
3. If QC1 re-review verdict is `Approve`, register residuals above in `status.json`, then mark Plan3 `Done`, then commit Done.
4. After Plan3 Done, dispatch **iteration/v1.38 → main** PR (V1.38 shipping).

## Status Update (chat-only, NOT a file)

- Plan1: Done (commit `3f72b085`).
- Plan2: Done (commit `8e58890a`).
- Plan3: implementation complete (`8ba5b296` ... `3cd14f96`), merged to integration (`ad455ec5`), QC tri-review committed (`64da2c27` / `93eba862` / `bf993faf`); consolidated verdict `Request Changes`; fix-now scope (W-1, W-2) being dispatched.
- Iteration: `iteration/v1.38` active; PR to `main` pending.

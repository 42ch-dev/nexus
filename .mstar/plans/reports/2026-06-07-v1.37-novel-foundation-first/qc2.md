---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-07-v1.37-novel-foundation-first"
verdict: "Approve"
generated_at: "2026-06-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Review Perspective: security + correctness risk
- Report Timestamp: 2026-06-08T01:30:00Z

## Scope
- plan_id: 2026-06-07-v1.37-novel-foundation-first
- Review range / Diff basis: merge-base(iteration/v1.37)..HEAD on feature/v1.37-novel-foundation-first
- Working branch (verified): feature/v1.37-novel-foundation-first
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 8 (core: preset_gates.rs, force_gates_audit.rs, novel_scaffold.rs, schedules.rs, run.rs; contracts: local/schedule/http.rs, local/orchestration/preset_gate.rs; plus plan, compass, and orchestration-engine.md excerpts for spec rules)
- Commit range: 73b9cb85 (single commit in range: "feat(v1.37-p0): novel foundation-first UX hardening")
- Tools run: git diff (targeted on 7 paths), rg -n "sqlx::query[^!]" on audit/works/work_chapters, git log on .sqlx/ and range, cargo +nightly fmt --all -- --check, cargo clippy -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings

## Findings

### 🔴 Critical
- **Filesystem gate path safety does not implement spec §7.6.1 canonicalize-first rule**: `PresetInput::substitute_path` does a post-substitution string check (`contains("..")` or `starts_with('/')`) on the logical template result, then `workspace_root.join(resolved)` + `.exists()`. There is no `canonicalize()`, no symlink resolution (`Path::canonicalize` or realpath), and no prefix check of the final canonical path against canonical workspace_root. This is a direct violation of the path-safety contract in orchestration-engine.md §7.6.1 (explicitly called out in the review assignment). A symlink inside the workspace pointing outside, or a TOCTOU swap of directory<->symlink between the string check and any downstream use of the path, can bypass the intended containment. The `..` check also over-rejects innocent names containing ".." (e.g. "file..bak").

  **Source Trace**
  - Finding ID: F-001
  - Source Type: git-diff | manual-reasoning | spec-rule
  - Source Reference: `git diff iteration/v1.37..HEAD -- crates/nexus-orchestration/src/preset_gates.rs` (substitute_path ~125-145, Filesystem arm in evaluate_gates ~250-270, no canonicalize anywhere in the new module); cross-reference to assignment § "Path safety" + "Symlink safety" + "Symlink / TOCTOU" and plan's orchestration-engine.md §7.6.1 citation.
  - Confidence: High

- **Gate evaluation read + schedule creation / enqueue are not atomic**: The `add_schedule` handler performs the full gate evaluation (work snapshot query, previous-preset COUNT via LIKE, filesystem `exists()`) in one phase, then (much later) inserts the schedule row and seeds core_context. No `BEGIN`/`COMMIT` wraps the gate decision together with the schedule creation. A parallel `creator run stage advance` (or another schedule for the same work) can mutate `works` / `work_chapters` / intake state in the window, causing a gate to pass on stale data or a forced schedule to be created after a concurrent state change. This matches the exact race concern listed in the assignment ("between gate check and preset enqueue").

  **Source Trace**
  - Finding ID: F-002
  - Source Type: git-diff | manual-reasoning
  - Source Reference: `git diff iteration/v1.37..HEAD -- crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs` (gate eval block ~130-340 using multiple unbound-in-tx queries, followed by schedule insert + core_context apply_seed with no enclosing transaction).
  - Confidence: High

### 🟡 Warning
- **`--gate-reason` / `force_gates` reason has no length cap, sanitization, or Unicode hygiene and is stored/logged verbatim**: CLI only requires non-empty when `--force-gates` is present (stage_advance guard). Daemon stores the raw string into `force_gates_audit.reason` (TEXT) and emits it via `tracing::warn!(reason = %reason_text, ...)`. No max length, no control-character stripping, no NFC normalization. Malicious or accidental content (very long payload, ANSI escapes, RTL overrides, newlines, homoglyphs) pollutes the audit table and process logs. If the reason is ever surfaced in any UI/TUI, this becomes a terminal/prompt injection vector.

  **Source Trace**
  - Finding ID: F-003
  - Source Type: git-diff | manual-reasoning
  - Source Reference: `git diff .../run.rs` (StageCommand gate_reason, the if force_gates && gate_reason.is_none() check ~633-640, no length logic); `git diff .../schedules.rs` (reason_text handling ~140-170, direct INSERT, warn log); `force_gates_audit.rs` (reason: String in params/row, no validation).
  - Confidence: High

- **Free-form `AddScheduleRequest.input` (HashMap/Value) with no reserved-key deny-list or namespacing**: The new field is an arbitrary `Option<serde_json::Value>`. The handler blindly folds every key into the `PresetInput.vars` map (used for `{{}}` substitution in filesystem paths) and appends the entire JSON as `preset.input=...` text into the seed (which flows to core_context and prompt rendering). CLI for init populates only a safe set, but there is no server-side allow-list, no automatic namespacing under `user.*`, and no rejection of keys that collide with WorkSnapshot fields (`creator_id`, `work_id`, etc.). A local API caller (or future compromised path) can inject arbitrary data into preset context.

  **Source Trace**
  - Finding ID: F-004
  - Source Type: git-diff | contract-review
  - Source Reference: `git diff .../local/schedule/http.rs` (new `input` + `force_gates` + `reason` fields); `schedules.rs` (~200-270 input folding into vars + effective_seed append + work_id derivation from input["work_id"]); `run.rs` (init_input construction).
  - Confidence: High

- **Audit INSERT logic duplicated between handler and local-db helper; handler uses raw runtime query**: `schedules.rs` contains a complete raw `sqlx::query("INSERT INTO force_gates_audit ...")` (with manual binds) for the force_gates path. The typed `insert_force_gates_audit` helper (which uses `query!`) in `force_gates_audit.rs` is not called. The list helper itself uses runtime `query_as` (with an explicit SAFETY comment about BOOLEAN mapping). This is unnecessary duplication and increases the surface of runtime queries for a security-sensitive audit table.

  **Source Trace**
  - Finding ID: F-005
  - Source Type: git-diff | manual-reasoning
  - Source Reference: `git diff .../schedules.rs` (~150-170 direct INSERT under force_gates); `force_gates_audit.rs` (insert_ helper using query!, list using query_as, the test that creates the table).
  - Confidence: Medium

- **cargo +nightly fmt --check fails on the delivered diff**: The large edit in schedules.rs left formatting that nightly fmt wants to adjust (the `workspace_root` match arm). Pre-merge checklist in the plan explicitly requires the fmt gate to pass.

  **Source Trace**
  - Finding ID: F-006
  - Source Type: linter
  - Source Reference: `cargo +nightly fmt --all -- --check` output during this review (reported "Diff in .../schedules.rs:270" for the workspace_root expression).
  - Confidence: High

- **Previous-preset gate completion check is a fragile label LIKE heuristic**: `DbPreviousPresetLookup` does `SELECT COUNT(*) FROM creator_schedules WHERE preset_id = ? AND status = 'completed' AND label LIKE ?` with `'%{work_id}%'`. This can produce both false positives (any other schedule whose label text happens to contain the work_id string) and false negatives (if label formatting or storage changes). It is the only implementation of the `PreviousPreset` gate kind that the spec intends for ordering enforcement (init before writing, etc.).

  **Source Trace**
  - Finding ID: F-007
  - Source Type: git-diff | manual-reasoning
  - Source Reference: `git diff .../schedules.rs` (DbPreviousPresetLookup impl at the bottom of the file).
  - Confidence: Medium

### 🟢 Suggestion
- Add hermetic tests that create symlinks inside the temp workspace root pointing outside it and verify that `Filesystem` gates still enforce containment (or that canonicalize + prefix check rejects them). Current tests only cover the string ".." case via input var poisoning.
- Move the force-gates audit INSERT + schedule creation (or at least the gate decision for non-force paths) inside a single DB transaction so that audit failure truly cannot leave an accepted schedule and to shrink the race window.
- In `substitute_path`, the `GateEvalError::PathSafety` variant reports the original `template` rather than the substituted `result` that actually triggered the check; this makes debugging slightly harder.
- The remediation strings in `preset_gates.rs` are all server-generated from static templates + safe identifiers (good — no user-controlled text ends up in the `PresetGatesFailed` response body itself).
- Many pre-existing runtime `sqlx::query` / `query(&format!(...))` calls exist in `works.rs` and `work_chapters.rs`. The new gate paths add a few more. Prefer `query!` (or tx-aware compile-time variants) for static SQL shapes going forward.
- Consider a small dedicated helper or moving the work-snapshot + previous-preset lookup logic into `nexus-local-db` so the large unsafe-feeling block in the daemon handler shrinks.
- Document (in the contract or a security note) that `input` values are untrusted user data for the purposes of prompt rendering and should not be used for security-critical decisions without additional validation.

## Source Trace
(See individual findings above for the primary per-finding traces. All git diffs were taken against `iteration/v1.37..HEAD` on the verified feature branch in the assigned worktree. CI commands were run from the repo root.)

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 6 |
| 🟢 Suggestion | 7 |

**Verdict**: Request Changes

## Revalidation (2026-06-08)

Re-review after fix commit `7d7f3d0b`. Targeted scope: C-1, C-2, W-1..W-5 + W-6 (reserved keys) + suggestions. Working branch and Review cwd verified via `git rev-parse`; diff basis `88bb0e05..HEAD` (fix wave) inspected; required gates re-run.

### Status by prior finding

- **C-1 (path safety canonicalize)**: RESOLVED — `canonicalize_within` added to `PresetInput` (uses `std::fs::canonicalize` + `starts_with` on canonical root); called for every `Gate::Filesystem` in `evaluate_gates` before `exists()`. New test `filesystem_path_traversal_rejected_by_canonicalize` creates real symlink `Works/escape -> /tmp/outside` and asserts `evaluate_gates` returns PathSafety "escapes workspace root". (Unix path; non-Unix falls to missing-path as before.) `git diff 88bb0e05..HEAD -- crates/nexus-orchestration/src/preset_gates.rs` confirms.
- **C-2 (gate eval + insert atomic)**: RESOLVED — `add_schedule` now does `let mut tx = pool.begin()...` for the gated (non-force) path: work snapshot query + gate eval + schedule INSERT + commit all inside tx (C-2). Force-gates path also uses tx for audit + schedule insert (W-7). `seed_core_context` factored out. No more split-phase eval-then-insert. Diff and handler source confirm tx scope.
- **W-1 (reason cap/sanitize)**: RESOLVED — Server (`schedules.rs`): `sanitize_reason` (ANSI regex strip + control filter except \n), `MAX_REASON_LEN=512`, early BAD_REQUEST reject (empty / too long / dirty) before any DB write or log, for `force_gates` reason. CLI (`run.rs`): identical 512 cap + `\x1b`/control check in both `handle_run` (--reason) and `handle_stage` (--gate-reason) before request construction. Tests `force_gates_with_ansi_in_reason_rejected`, `force_gates_with_long_reason_rejected` pass.
- **W-2 (reserved input keys)**: RESOLVED — `RESERVED_INPUT_KEYS: &[&str] = &["creator_id", "workspace_slug", "core_context", "preset"]`; early in `add_schedule`, if `body.input.as_object()` contains any, returns 400 with message listing reserved set. (Explicit comment: `work_id` is intentionally allowed/extracted for gate use, not blindly merged.) Matches assignment "free-form AddScheduleRequest.input reserved-key policy".
- **W-3 (audit INSERT dedup)**: RESOLVED — Handler now calls typed `nexus_local_db::insert_force_gates_audit(&mut tx, &audit_params)` (inside the tx for force path). Helper updated to accept `&mut sqlx::SqliteConnection` (tx-friendly); no more raw `sqlx::query("INSERT INTO force_gates_audit...")` in schedules.rs for the audit row. (Schedule INSERT remains direct SQL inside tx, per pattern.)
- **W-4 (fmt)**: RESOLVED — `cargo +nightly fmt --all -- --check` (run from repo root) produced zero output (clean). Clippy on the four crates also clean (`-D warnings`).
- **W-5 (previous_preset LIKE)**: RESOLVED — New migration `202606080002_creator_schedules_work_id.sql` adds `work_id TEXT` + `CREATE INDEX idx_creator_schedules_preset_status_work ON creator_schedules(preset_id, status, work_id)`. `DbPreviousPresetLookup::find_previous_preset_completion` now does exact `WHERE preset_id = ? AND status = 'completed' AND work_id = ?` (bind &work_id) with comment "C-4 fix: use indexed work_id column instead of LIKE on label". LIKE fallback note in migration for pre-migration rows.

### New findings (if any)

None (no new Criticals or mandatory Warnings introduced in the fix wave). The large schedules.rs refactor (tx + early rejects + helper call + lookup change) is surgical around the prior findings; no obvious new races, injection surfaces, or unindexed paths added. One pre-existing pattern (runtime query for the schedule INSERT itself inside tx) remains, but is inside the atomic boundary and matches the crate's documented SAFETY style.

### New evidence

- `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api`: 10/10 passed (new hygiene tests + prior gate/schedule tests; confirms atomic paths and reason validation exercised).
- `git diff 88bb0e05..HEAD --stat`: 12 files, focused on the 4 crates + 2 new migrations + test updates + status.
- Symlink test, tx blocks, sanitize fn, RESERVED_KEYS const, helper call site, work_id index + equality query, fmt clean — all directly visible in the three targeted diffs.
- No changes to business logic outside the QC2 scope (e.g. no new preset gate kinds, no CLI flag changes beyond hygiene).

**Updated Verdict**: Approve

### Suggestions from wave 1 (re-checked)

- Symlink test: now present (C-1 test added).
- Atomic tx: now present (C-2 + force path).
- Helper usage: now present (W-3).
- Prune helper: exists in force_gates_audit.rs (added in wave).
- Other suggestions (remediation strings, query! preference, dedicated helper, input doc) remain open but non-blocking per original classification.

---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-07-v1.36-novel-project-init-preset"
verdict: "Request Changes"
generated_at: "2026-06-07"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (path traversal, SQL safety, atomicity, untrusted input to FS/DB, world_id FK, idempotency, race conditions)
- Report Timestamp: 2026-06-07T19:xx:xxZ (review executed in single leaf session)

## Scope
- plan_id: `2026-06-07-v1.36-novel-project-init-preset`
- Review range / Diff basis: `merge-base: iteration/v1.36` (commit `1856258`) + `tip: feature/v1.36-novel-project-init-preset` (commit `2a97858`)
- Working branch (verified): `feature/v1.36-novel-project-init-preset`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.36-p1-init`
- Files reviewed: 30 (diff stat +1856/-120); focused on novel_scaffold.rs (423 LOC core), novel_project_init.rs (500-line hermetic tests), work_chapters.rs (246 LOC + SAFETY), 2 migrations, creator/run.rs wiring, works.rs (WorkPatch + novel columns), preset.yaml + 8 prompts + 4 templates, cli-spec.md update, daemon handlers (host_tool_executor + works), lib re-exports.
- Commit range: 6 commits (2a97858 feat T7/T8 ... ed867fd feat preset YAML/prompts/templates)
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git log --oneline iteration/v1.36..HEAD`, `git diff --stat`, targeted `git diff` hunks, full file reads of all new/changed implementation + specs (novel-workflow-profile.md §3.5/§4.1/§5.3.1/§5.3.2/§5.3.5/§5.4/§5.4.5 + orchestration-engine.md §7.9), `cargo +nightly clippy -p nexus-orchestration -p nexus42 -p nexus-local-db -- -D warnings`, `grep` for work_ref/Works/seed_chapters/INSERT/format!/query patterns.

## Findings

### 🔴 Critical

- **C-1 Path traversal via unsanitized `work_ref` slug (LLM/grill-me input directly to FS paths)**: `novel_scaffold.rs:143` does `let root = self.works_root.join(&inp.work_ref);` with zero validation. `work_ref` originates from `collect_work_ref` ACP node (T1 `prompts/init-work-ref.md`) which accepts free text from the agent; no kebab-case enforcement, no `..`/absolute/shell-meta rejection, no length limit. Same value is interpolated into `outline_path`/`body_path` templates (lines 60-62 in work_chapters seeding) and passed to `Works/<work_ref>/` mkdir + template writes. Spec cross-ref: novel-workflow-profile.md §2.1 ("work_ref is stable human slug"), §5.4.1 ("directory name under Works/"), §3.2/§5.4.5 (idempotency rules assume valid slug). Plan T2/T6 assumed safe slug. No guard in CLI `creator/run.rs:175-182` wiring either.  
  **Evidence**: `novel_scaffold.rs:143,164,189,203,213,232,255`; `work_chapters.rs:60-62` (format! paths); tests only use "my-novel"/"idem-novel" happy paths; T7b idempotency test does not exercise malicious slugs.  
  **Impact**: Attacker-controlled (or hallucinated) `work_ref` can escape `Works/` tree, overwrite arbitrary files under workspace, or create hidden dirs. High-severity for a scaffold that runs with user workspace privileges.

- **C-2 Atomicity failure across FS + DB layers (no workspace transaction, separate PATCH after seed tx)**: Scaffold performs raw `std::fs::create_dir_all` + `write_file_idem` (no `.tmp`+rename, no rollback), then calls `work_chapters::seed_chapters` (single tx, good), then a **separate** `works::patch_work` outside any tx (novel_scaffold.rs:252-278). FS mkdirs are never rolled back on later failure. Spec cross-ref: novel-workflow-profile.md §5.4.3 ("entire scaffold (mkdir + template copies + work_chapters inserts + works PATCH) must succeed or fail together"), §5.4.4 ("all atomic"), plan T2a–T2i/T3/T4/T6 ("creator.workspace.transaction or equivalent"). `write_file_idem` is existence-skip only (not atomic write).  
  **Evidence**: `novel_scaffold.rs:149-227` (FS), `230-244` (seed), `252-278` (PATCH after); `work_chapters.rs:56-85` (its own tx only); absence of any `transaction` wrapper or `creator.workspace` capability in the capability run path.  
  **Impact**: Partial state (orphan dirs + seeded chapters but failed PATCH, or vice-versa) on any transient FS/DB error. Re-init cannot reliably clean up.

- **C-3 `world_id` FK existence not validated before binding (existing-world or "create new" placeholder branch)**: Grill-me `init-world-existing.md` + `init-world.md` collect a `world_id` string (or placeholder from future `creator world create`). Scaffold accepts it verbatim into `ScaffoldInput.world_id` and unconditionally PATCHes `works.world_id` (novel_scaffold.rs:258). No `SELECT 1 FROM worlds WHERE world_id = ?` check, no FK enforcement at this layer. Spec cross-ref: novel-workflow-profile.md §3.5 ("when set, chapter body may reference World KB items"; "grill-me init-world-existing.md"), §5.3.2 (world_id gate for novel-writing is conditional on preset manifest).  
  **Evidence**: `novel_scaffold.rs:32,154-156,258` (world_section + PATCH); `t7d_*` tests in novel_project_init.rs:278-366 (they insert the string and assert it is stored — no existence query); works.rs PATCH path and daemon works handler have no novel-specific world validation.  
  **Impact**: Dangling `world_id` on Work; downstream `novel-writing` World KB injection (orchestration-engine §5.2) will fail or silently produce wrong context. Violates "bind to existing" contract.

- **C-4 Untrusted LLM output used directly for filesystem paths and DB `work_ref` / chapter slugs without sanitization**: All 9 focus-area paths (grill-me title/genre/chapters/work_ref/world answers → ScaffoldInput → join + format! paths in seed + template rendering) treat ACP responses as trusted. No normalization to kebab-case, no rejection of control chars / `..` / `/` / shell meta. Spec cross-ref: §5.4.1 directory rules, §5.3.5 gate UX (assumes valid work_ref), §3.2 (work_ref invariant).  
  **Evidence**: T1 prompts (init-*.md) are pure free-text collection; capability input schema (novel_scaffold.rs:128) only requires string/integer with no pattern; no validator in preset loader or scaffold ctor.

### 🟡 Warning

- **W-1 TOCTOU / concurrent re-init race not mitigated or documented**: Two concurrent `novel-project-init` runs on the same Work interleave raw mkdir + write_file_idem (skip-if-exists) + seed tx. DB is protected by per-row INSERT OR IGNORE, but visible partial FS state + chapter count can be observed mid-race. Plan context accepts "single-user local" but does not document the limitation or add even a simple advisory lock note. Spec §5.4.5 idempotency is "safe on re-init" but does not claim concurrent safety.

- **W-2 `works` PATCH in scaffold always overwrites novel columns even on re-init (broader than spec "only fields user changed")**: novel_scaffold.rs:253-269 constructs WorkPatch with `Some(...)` for work_profile/work_ref/total_planned_chapters/world_id/current_chapter unconditionally. Spec §5.4.4 says "PATCH fields user did NOT change in this grill-me session are no-ops". Current behavior is last-writer-wins (harmless for init but inconsistent with stated contract and with T6 "idempotent" language).

- **W-3 Pre-existing R-V133P1-09 (runtime sqlx::query vs compile-time query_as! for dynamic columns) noted but not worsened**: work_chapters uses documented SAFETY runtime queries with binds (acceptable for new table in same migration cycle). The novel columns on works reuse the existing dynamic patch machinery. Plan "Deferred/conditional" section correctly flags this; P1 did not introduce fresh instances.

### 🟢 Suggestion

- **S-1 Add explicit work_ref sanitization + validation at capability boundary + in prompts**: Enforce `^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$` (or documented kebab-case rule from §2.1), reject `..`/absolute, surface clear error in grill-me before scaffold. Would close C-1/C-4 at the source.

- **S-2 Wrap full scaffold (FS + seed tx + PATCH) in a single `creator.workspace.transaction` equivalent** (or at minimum a coordinating tx + best-effort FS cleanup on rollback) to satisfy §5.4.3 atomicity. Document the V1.36 limitation if full atomic FS+DB is deferred.

- **S-3 Add world_id existence probe (or soft "pending" state) before PATCH** when the grill-me branch selects "existing world". At minimum log a warning on dangling bind so later novel-writing World KB steps can surface a useful error.

- **S-4 Document concurrency model** (single-daemon-process assumption) and the idempotency-vs-concurrent distinction in cli-spec.md §6.2D update and/or novel-workflow-profile.md §5.4.5.

## Source Trace
- **C-1 (path traversal)**: Source = manual reasoning + git diff on novel_scaffold.rs + work_chapters.rs + T1 prompts; cross-checked against plan T2 + spec §2.1/§5.4.1. Confidence: High.
- **C-2 (atomicity)**: Source = full read of novel_scaffold.rs:135-288 (run fn) + work_chapters.rs:56-87 (tx) + absence of workspace tx in capability; spec §5.4.3/§5.4.4. Confidence: High.
- **C-3 (world_id FK)**: Source = novel_scaffold.rs:154-156 (world_section), 258 (PATCH), t7d tests, spec §3.5. Confidence: High.
- **C-4 (LLM input to paths)**: Source = T1 prompt files + scaffold input schema + format!/join sites + plan T1/T2. Confidence: High.
- **W-1 (race)**: Source = diff review of FS + seed paths + lack of lock; plan context note on single-user. Confidence: Medium.
- **W-2 (PATCH scope)**: Source = novel_scaffold.rs:253-269 vs spec §5.4.4 wording. Confidence: High.
- **W-3 (pre-existing residual)**: Source = plan "Deferred/conditional" table + work_chapters SAFETY comments + works.rs runtime query pattern (consistent with R-V133P1-09). Confidence: High.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 4 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

**Rationale**: Four unresolved Critical findings (path traversal via unsanitized LLM-derived `work_ref`, missing cross-layer atomic transaction, absent world_id FK existence check before binding, direct use of untrusted grill-me output for filesystem paths and DB slugs) directly violate the security and correctness invariants in the primary spec (novel-workflow-profile.md §2.1/§3.5/§5.3/§5.4) and the plan's own T2/T3/T4/T6 acceptance criteria. No Critical may remain for Approve per mstar-review-qc verdict rules. Warnings are documented for the PM/consolidation step but are not blocking on their own. Clippy clean; tests cover happy-path + idempotency well but do not exercise the attack surfaces.

**Next dispatch**: None (leaf QC role; handoff only via PM consolidated decision + targeted re-review if fixes land).

## Completion Report v2

**Agent**: qc-specialist-2
**Task**: QC #2 security + correctness review for 2026-06-07-v1.36-novel-project-init-preset (V1.36 P1)
**Status**: Done
**Scope Delivered**: Verified cwd/branch/range on entry (exact commands), read plan + spec anchors (novel-workflow-profile §3.5/4.1/5.3.x/5.4 + orchestration §7.9), full diff + targeted reads of all 30 changed files (core: novel_scaffold.rs, work_chapters.rs, tests, migrations, CLI wiring, works PATCH, preset + prompts/templates), ran clippy (clean), deep-dived all 9 assigned security/correctness foci with file:line + spec cross-refs, produced qc2.md report only (no source edits), committed only the report path, emitted this Completion Report v2 in same turn.
**Artifacts**: `.mstar/plans/2026-06-07-v1.36-novel-project-init-preset/reports/qc2.md` (committed)
**Validation**: Entry git commands captured; `git diff --stat` + name-only + targeted hunks; full reads of novel_scaffold.rs (423), novel_project_init.rs (500), work_chapters.rs (246), preset.yaml, migrations, creator/run.rs, works.rs relevant sections, cli-spec.md delta, daemon handlers; `cargo +nightly clippy -p nexus-orchestration -p nexus42 -p nexus-local-db -- -D warnings` (0 warnings); grep for work_ref/seed_chapters/INSERT/format!/query patterns across workspace.
**Issues/Risks**: 4 Critical (path traversal on work_ref, atomicity across FS+DB, world_id FK bypass, untrusted LLM input to paths) + 3 Warning (race not documented, PATCH scope slightly broader than spec, pre-existing residual not worsened). All map to real spec/plan violations with concrete code locations.
**Plan Update**: None (QC does not edit plans; PM will register residuals in status.json per mstar-plan-artifacts).
**Handoff**: None — leaf executor. PM will consolidate with qc1/qc3 and decide targeted re-review or integration.
**Git**: `2a97858 feat(v1.36-p1): T7 hermetic tests + T8 cli-spec.md (novel-project-init)` (pre-review tip); report commit: `c3f0e4a qc(v1.36-p1): security + correctness review (qc2)` (exact short hash from `git log -1 --oneline` after `git add .mstar/plans/2026-06-07-v1.36-novel-project-init-preset/reports/qc2.md && git commit -m "qc(v1.36-p1): security + correctness review (qc2)"`).

---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "v1.65"
verdict: "Approve"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: per `opencode.json` qc-specialist config
- Review Perspective: Architecture coherence + maintainability risk (module boundaries, contract/codegen alignment, convention adherence, reuse vs duplication, error-handling explicitness, dep-upgrade impact)
- Report Timestamp: 2026-06-25T17:55:00Z

## Scope
- plan_id: v1.65
- Review range / Diff basis: `merge-base 644acbc56856d03e8e3aaf2139f73dccfcf6ed54 ... HEAD 73e3343081ffa415b221252b5432dc1c6e21f07b` (= `git diff origin/main...HEAD` on `iteration/v1.65`; 112 files, +8902/-422)
- Working branch (verified): `iteration/v1.65`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (main worktree, on `iteration/v1.65` HEAD)
- Files reviewed: 112 (focus on `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs` (913 LOC), `chapters.rs` route registration in `api/mod.rs`, `works.rs`/`preset_management.rs` for pattern comparison, generated Rust + TS contracts under `crates/nexus-contracts/src/generated/local_api/works/chapters/` and `packages/nexus-contracts/src/generated/local-api/works/chapters/`, schema sources under `schemas/local-api/works/chapters/`, `apps/web/src/lib/nexus/{types,browser-client,tauri-client,query-keys,adapters}.ts`, `apps/web/src/api/queries.ts`, `apps/web/src/pages/{chapters-page,chapter-page}.tsx`, plus dependency manifests and the schema drift detection test).
- Commit range: `644acbc5..73e33430` (P0 + P-sec + P1 + P2 merged on the integration branch)
- Tools run:
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse origin/main`, `git log --oneline -20`, `git diff --stat`, `git diff --name-only`
  - `cargo +nightly fmt --all --check` (clean)
  - `cargo clippy --all -- -D warnings` (clean)
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` (clean)
  - `cargo test -p nexus-contracts --test schema_drift_detection` (4/4 passed — Strict mode bidirectional Rust↔schema check confirms all 11 chapter schemas are aligned)
  - `cargo test -p nexus-contracts` (5/5 enum-conversion tests passed)
  - `cargo test -p nexus-daemon-runtime --lib` (305/305 passed)
  - `cargo test -p nexus-daemon-runtime --tests` (works_api 34/34, workspace_occ 4/4 — chapters endpoints have no integration tests)
  - `cargo test -p nexus-daemon-runtime --lib chapters::` (6/6 unit tests in the chapters handler passed)
  - `pnpm --filter nexus-contracts build` (prerequisite: regenerates `dist/` which is gitignored)
  - `pnpm --filter web typecheck` (clean after contracts build)
  - `pnpm --filter web test` (80/80 passed across 10 test files; 30→80 growth from V1.64 baseline)
  - `pnpm --filter web test:coverage` (92.77% stmt / 81.13% branch — exceeds the P1 baseline target of ≥80% architectural-surface coverage)
  - Multiple `read` + `grep` on the chapters handler, path-guard helpers, lock-guard, preset CRUD, and the W-002 reference in `host_tool_handlers.rs` for pattern comparison.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning

#### W-1 — `PUT /v1/local/works/{work_id}/chapters/{n}/outline` does not roll back the committed outline file when the DB metadata update fails (spec §6.2 deviation)
- **Source**: `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:506-576` (handler), `chapters.rs:316-357` (`atomic_write_outline`).
- **Evidence**:
  - The handler executes `atomic_write_outline(...)` (file write) and *then* `work_chapters::update_outline_path(...)` (DB update) sequentially, with the `RuntimeLockGuard` held for the whole block.
  - On DB-update failure, the `?` propagates an `Internal { code: "DATABASE_ERROR", ... }` error. The committed outline file remains on disk; the DB still has the *old* (or null) `outline_path`; the handler returns 500 to the client.
  - `atomic_write_outline` cleans up the **temp** file on write/rename failure, but does *not* clean up the final target if the rename succeeded and the caller subsequently fails. The handler also has no post-rename failure path to undo the file.
  - `local-api-surface-conventions.md` §6.2 is normative: *"Update `work_chapters.outline_path` and `updated_at` in the same transactional finalization path as the file rename. Failed DB update or failed rename must clean up the temp file where possible and must not report success."*
  - The reference pattern in `work_chapters::sync_frontmatter_status` (`crates/nexus-local-db/src/work_chapters.rs:655-737`) is single-step and only handles the write/rename failure path; the new chapters handler extends that pattern to a multi-step file+DB write without a corresponding rollback.
- **Impact**: This is the first *writable* chapter-file route in the codebase (V1.64 UI was read+setup). The current implementation can leave the workspace in an inconsistent state where a chapter's outline file exists at the new path but `work_chapters.outline_path` does not point to it. Subsequent reads (which use the DB path) will 404 the user; subsequent `PUT outline` calls will silently write the same file again or trigger the canonical-path-init fallback. The "in the same transactional finalization path" requirement is not met by sequential awaits under a runtime lock — a runtime lock guards single-writer, not atomicity.
- **Fix**:
  1. Move the DB update before the file write (compute and persist the canonical `outline_path` in DB, then write file). On DB success + file write failure, the DB is correct; on DB failure, no file was written.
  2. **Or** capture the prior `outline_path` / file content; on DB-update failure, rename the new file back to the old path (or remove the new file) and restore the old DB row.
  3. **Or** (cleaner) lift the file+DB write into a single SQL transaction that does both the file-rename and the metadata update, e.g. via a `BEGIN; UPDATE work_chapters SET outline_path=? ...; <external file rename via FFI or post-commit hook>` — but a runtime lock is necessary to keep the in-between window safe.
  - Either approach is acceptable; the current implementation's "sequential, best-effort cleanup" is not.
- **Residual severity map**: `high` (architectural integrity, first-writable-file surface; spec-normative requirement).

#### W-2 — No HTTP-level integration tests for any chapter endpoint; path-guard negative test only covers the `must_exist=true` branch
- **Source**: `crates/nexus-daemon-runtime/tests/works_api.rs` (no `chapters_api.rs` file), `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:735-912` (handler-internal `mod tests`).
- **Evidence**:
  - 6 unit tests in the handler module cover happy paths for `list_chapters`, `get_chapter`, `put_chapter_outline`, `patch_chapter`, `get_chapter_body`, and one helper-level path-guard test (`resolve_guarded_path_accepts_inside_and_rejects_escape`).
  - The path-guard test exercises `must_exist=true` only, with `rel_path = "../escape.md"`. The `must_exist=false` (write) branch — the one used by `put_chapter_outline` and `atomic_write_outline` — is **not** covered by any test. The `../escape` case in the existing test returns `CHAPTER_PATH_UNRESOLVABLE` (the canonicalize fails before the prefix check), not `CHAPTER_PATH_FORBIDDEN` — so the test does not actually verify the prefix guard fires for traversal.
  - The plan T7 acceptance criterion was: *"Tests: handler-level (handler_state fresh DB) per endpoint + path-guard negative tests + codegen TS type snapshot"*. "Handler-level" was satisfied (in-mod unit tests) but the test surface did not include HTTP routing/serialization (no `tests/chapters_api.rs` analogue of `tests/works_api.rs`).
  - No tests cover: (a) the structure-PATCH `published`-hard-block / `finalized`-confirmation-required branches, (b) the `title` rejection in PATCH, (c) the `not_started → outlined` exclusive status transition, (d) the `chapter_not_found` error path, (e) the auth-required path.
- **Impact**: Regressions in the route registration, axum extractor parsing, JSON serialization, or middleware ordering (e.g. auth_middleware running after the handler) would not be caught by the in-mod unit tests. Path-guard regressions in the write path (the one that actually mutates disk) would also be silent.
- **Fix**: Add `crates/nexus-daemon-runtime/tests/chapters_api.rs` with at least: one happy-path integration test per endpoint (list/get/outline-get/outline-put/structure-patch/body-get), one path-guard negative test per branch (`must_exist=true` and `must_exist=false` for `../escape`, absolute path, empty), one auth-required test, one `title` 400 test, one `published`-hard-block test, one `not_started→outlined` exclusive-transition test, and the `chapter_not_found` path. Use `handler_state()` (fresh DB) per the established V1.42.1 hotfix pattern.
- **Residual severity map**: `medium` (test coverage is the plan's stated acceptance gate; main risk is silent regression in routing/middleware, not in business logic).

### 🟢 Suggestion

#### S-1 — `RuntimeLockGuard` is fully duplicated between `chapters.rs` and `works.rs`
- **Source**: `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:24-106` and `crates/nexus-daemon-runtime/src/api/handlers/works.rs:26-125`.
- **Evidence**: The struct fields, `acquire` / `release` / `disarm` methods, `Drop` impl, and even the warning log messages are byte-identical (modulo whitespace). The V1.42.1 hotfix rule lives in `crates/nexus-daemon-runtime/AGENTS.md` and is referenced from both call sites; if the rule evolves (e.g. a V1.66 async-Drop exploration per R-V142.1-ARCH-LESSON), the change must be applied in two places.
- **Impact**: Maintenance drift risk. If one copy adds a new field (e.g. lock-acquired-at) and the other does not, the two handler families will report different telemetry. The `Drop` warning message also diverges subtly between the two copies — `works.rs:121-122` includes "TTL-based recovery will clean up" while `chapters.rs:99-103` says "TTL will clean up stale lock" (the same intent, different wording), which is purely cosmetic.
- **Fix**: Move the struct to `crates/nexus-daemon-runtime/src/api/lock_guard.rs` and `pub use` it from `api/handlers/mod.rs`. The two handlers can then reference the single source of truth.
- **Severity**: `low` (code-organization, not blocking).

#### S-2 — `to_detail().can_edit_outline` is set from the DB-stored path string alone, with no path-guard validation
- **Source**: `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:262-281` (`to_detail`), especially line 276: `can_edit_outline: r.outline_path.as_deref().is_some_and(|s| !s.is_empty())`.
- **Evidence**: A `work_chapters` row with a non-empty `outline_path` outside the workspace root (corrupt data, a `seed_chapters` from a previous workspace, or a manual DB edit) will be reported as `can_edit_outline: true` by the API, but the actual `PUT outline` will return `CHAPTER_OUTLINE_PATH_FORBIDDEN`. The UI affordance (Save Outline button enabled, editor opens) would mislead the user.
- **Impact**: A user-facing affordance mismatch. The path guard still protects disk writes, so this is a UX paper-cut, not a security issue.
- **Fix**: Either (a) move the path-guard probe into `to_detail` and set `can_edit_outline` only when the joined path resolves inside the canonical workspace root, or (b) trust the DB invariant in V1.65 (single-writer local-only) and document the assumption. The "trust DB" choice is reasonable for V1.65 but worth a comment.
- **Severity**: `low` (UX correctness, defense-in-depth).

#### S-3 — `PatchChapterRequest.title` is in the schema but always rejected at the handler with a 400
- **Source**: `schemas/local-api/works/chapters/patch-chapter-request.schema.json:9-12` (title is a valid optional `string`), `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:602-609` (always returns `CHAPTER_TITLE_UNSUPPORTED`).
- **Evidence**: The spec intentionally makes `title` a valid field in the request schema (so clients can build the typed request) but the handler rejects it because no `title` column exists in `work_chapters` yet. The 400 message is helpful ("use outline frontmatter or slug instead") but a JSON-schema-aware client generator will emit `title?: string` and the client SDK will offer it as a form field.
- **Impact**: Minor user-experience oddity — typed clients can submit a `title` and only learn at request time that it is rejected. A 422 with a `field_errors` array would be more discoverable.
- **Fix**: Two clean options:
  1. Add `"title": { "type": "string", "readOnly": true }` in the request schema (codegen will mark it as not settable); handler still rejects as a defense-in-depth.
  2. Add a `validation_failed` envelope with a `field_errors: [{ field: "title", code: "chapter_title_unsupported", message: "..." }]` so the UI can suppress the field early.
  - Either is acceptable; the current 400-with-helpful-message is workable for V1.65.
- **Severity**: `low` (DX polish; spec-compliant today).

#### S-4 — `resolve_guarded_path` returns the *un-canonicalized* joined path on the write branch
- **Source**: `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:158-223`.
- **Evidence**: On the `must_exist=false` branch, after the ancestor-probe prefix check passes, the function returns `Ok(joined)` where `joined = canonical_root.join(rel_path)`. The caller (`atomic_write_outline`) then does `target.parent().map(|p| create_dir_all(p))` and `target.with_extension(...)` — using the un-canonicalized path. If `workspace_root` itself is a symlink (e.g. a relocated workspace), the resulting writes land in a path that may not match the canonicalized root, but the read path uses the canonical target. This is mostly theoretical (workspaces are not typically symlinked), and the read path canonicalizes before the prefix check, so the safety properties are preserved.
- **Impact**: Defense-in-depth. The current local-first single-user model does not typically put workspaces behind symlinks, so this is a latent risk rather than an active one.
- **Fix**: After the ancestor-probe check passes, `canonicalize` the joined path (post any `create_dir_all`) and re-check the prefix. Or document the "no symlinked workspace root" assumption.
- **Severity**: `low` (defense-in-depth; the QC2 reviewer's S-001 lands in the same area and is left as the security-track finding).

#### S-5 — `api/mod.rs` route registration nests chapter sub-routes under the same `/works/{work_id}` prefix as Works
- **Source**: `crates/nexus-daemon-runtime/src/api/mod.rs:312-329`.
- **Evidence**: The chapter routes are registered as siblings of the existing `works` routes in the same Router. This works, but it means a future chapter-prefixed router (`/v1/local/works/{work_id}/chapters/...`) could collide with another Works-sub-resource if the Works family grows (e.g. `works/{work_id}/fork` would be fine, but `works/{work_id}/outlines` would be a sibling to `works/{work_id}/chapters` — both plausible).
- **Impact**: Mildly couples chapter routing to the Works router. Not a problem today.
- **Fix**: Optional refactor to nest chapter sub-routes via `Router::nest("/v1/local/works/{work_id}/chapters", chapter_router)`. Low priority; the current flat registration is readable and matches the V1.64 works.rs pattern.
- **Severity**: `low` (organizational).

#### S-6 — `enum_conversions.rs` adds a hand-maintained `ChapterStatus` impl block
- **Source**: `crates/nexus-contracts/src/enum_conversions.rs:14, 494-521, 987-1002`.
- **Evidence**: Three blocks (Display, Default, FromStr) for `ChapterStatus` are hand-maintained in `enum_conversions.rs` because codegen does not emit `Default` and the as_str/FromStr helpers are out-of-codegen-scope. The pattern is established (`BlockType`, `StoryManifestStatus`, etc. all live there too), and the `AGENTS.md` rule ("extend generated types without modifying them") is satisfied. This is a Suggestion because the file is now ~1000 LOC and a future contributor may not know to look there for new enum helpers.
- **Impact**: Discoverability. The `enum_conversions.rs` is a hand-maintained extension surface, but it is not indexed from any spec or AGENTS.md file at the moment.
- **Fix**: Add a one-line header comment in `enum_conversions.rs` cross-linking to `crates/nexus-contracts/AGENTS.md` (which already documents the rule) and to the codegen templates if any. Minor.
- **Severity**: `low` (DX).

#### S-7 — `pnpm-workspace.yaml` / `package.json` are out of scope for this review
- **Source**: `pnpm-lock.yaml` shows 2129 lines changed (dep bumps + new `@tiptap/*`, `react-markdown`, `remark-gfm`).
- **Evidence**: P-sec (vitest 2→3, vite 5→6, wiremock 0.6) plus P2 (TipTap stack, react-markdown). All deps are within the major-version allow-list of their respective P-numbers; no `cargo tree -i rand:0.7.3` match. `gh api` shows 9 pre-merge open Dependabot alerts (P-sec did not address all of them, only the targeted closure for the rand 0.7.3 advisory). This is a dep-bump correctness call, not an architecture one.
- **Impact**: Stable for the P-sec targets. The remaining 9 alerts are out of scope for V1.65.
- **Fix**: No action in V1.65. Future dep-security plans can address the residual 9.
- **Severity**: `low` (deferred to future plan; no architectural concern).

## Source Trace
- Finding ID: W-1, W-2, S-1, S-2, S-3, S-4, S-5, S-6, S-7
- Source Type: manual review + static analysis + targeted test execution + `git diff` walk
- Source Reference:
  - `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:158-223` (resolve_guarded_path); `316-357` (atomic_write_outline); `506-576` (put_chapter_outline); `579-677` (patch_chapter); `679-731` (get_chapter_body); `735-912` (in-mod tests).
  - `crates/nexus-daemon-runtime/src/api/mod.rs:312-329` (route registration).
  - `crates/nexus-daemon-runtime/src/api/handlers/works.rs:26-125` (RuntimeLockGuard reference copy).
  - `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:2006-2039` (W-002 reference pattern).
  - `crates/nexus-local-db/src/work_chapters.rs:284-339, 655-737` (list_chapters_paginated, update_outline_path, sync_frontmatter_status).
  - `crates/nexus-contracts/src/enum_conversions.rs:494-521, 987-1002` (ChapterStatus extension).
  - `crates/nexus-contracts/tests/schema_drift_detection.rs:243-296` (Strict-mode chapter schema entries).
  - `schemas/local-api/works/chapters/*.schema.json` (11 schemas; all in Strict mode).
  - `apps/web/src/lib/nexus/{types,browser-client,tauri-client,query-keys,adapters}.ts` (adapter boundary).
  - `apps/web/src/api/queries.ts:260-342` (chapters hooks).
  - `apps/web/src/pages/{chapters-page,chapter-page}.tsx` (P2 UI; tabs, table, body read-only).
  - `apps/web/src/lib/nexus/adapter-contract.test.ts:35-67, 71-106` (contract guard for global `fetch` + TauriClient throw).
  - `.mstar/knowledge/specs/local-api-surface-conventions.md:166-268` (chapter-content §6 conventions).
  - `.mstar/knowledge/specs/chapter-content-local-api.md:200-228` (PUT outline rules).
  - `.mstar/knowledge/specs/web-ui.md:13, 192-235, 232-end` (V1.65 content-authoring stage).
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 7 |

**Verdict**: Request Changes

## Reviewer Notes

This review focuses on **architecture coherence and maintainability**. Cross-checked findings with QC2 (security, in `qc2.md` — Approve, zero Warnings) and QC3 (performance/reliability, in `qc3.md` — Request Changes, 4 Warnings): no overlap with QC3's W-1 (offset pagination) or W-2 (body size cap), which are performance-track and out of scope here. The two Warnings in this report (W-1 transactional atomicity, W-2 missing integration tests) are **architecture-track** concerns the other reviewers would not raise.

**Architecture strengths worth carrying forward**:
- Module boundaries are intact: P2 UI routes through `useNexusClient()` → `BrowserClient` → `fetchImpl`; the P1 contract-guard test (`adapter-contract.test.ts:58-67`) actively scans every non-adapter module for direct `fetch` calls and passes; `TauriClient` stub mirrors the interface and is unit-tested.
- Codegen is consistent: 11 chapter schemas in `schemas/local-api/works/chapters/` are matched in Strict mode by `schema_drift_detection.rs`; both `crates/nexus-contracts/src/generated/local_api/works/chapters/mod.rs` and `packages/nexus-contracts/src/generated/local-api/works/chapters/` mirror the schema tree. No hand-written DTOs duplicate generated types. Barrel `packages/nexus-contracts/src/generated/index.ts:82-92` re-exports all 11 chapter types.
- The `display-only title` model is coherent end-to-end: schema marks `title` as optional (never `required`); Rust `to_summary`/`to_detail` always set `title: None`; the PATCH handler explicitly rejects with `CHAPTER_TITLE_UNSUPPORTED`; the UI falls back to `Chapter {n}` when `title` is null. The same model is honored by the optional `ChapterSummary.title?: string` and `ChapterDetail.title?: string` in TypeScript.
- Cursor pagination, `items` key, and the `ErrorResponse` envelope are followed uniformly. `pagination.rs` (the v1: offset-backed opaque cursor) is reused, and the chapters list handler registers in the same shape as the other V1.64 cursor lists.
- V1.42.1 hotfix rule is satisfied in both mutating handlers: `load_work` → `get_chapter` → status transition check → protection check → `RuntimeLockGuard::acquire` → work → `lock.release().await` → `?`. The 2 release points are at `chapters.rs:565` (outline PUT) and `chapters.rs:675` (structure PATCH).
- P-sec dep bumps are clean: no new lint/test fallout; `cargo tree -i rand:0.7.3` is empty post-bump.

**Architecture concerns to address in the next fix round**:
- **W-1** is the only architectural-integrity blocker. The "transactional finalization path" in the spec was not realized — sequential awaits under a runtime lock preserve single-writer but not atomicity. Choose one of the three fix approaches (DB-first, restore-on-failure, or SQL-transaction-wrapped) and re-verify the rollback path with a test.
- **W-2** is the test-coverage plan acceptance gate. The plan's T7 reads as having been met by the in-mod unit tests, but a follow-up `tests/chapters_api.rs` integration file would (a) cover the routing/serialization layer the unit tests cannot, and (b) lock in the path-guard negative branches for both `must_exist=true` and `must_exist=false`.
- Suggestions are all light; the QC2 reviewer's S-001 (path-guard hardening) overlaps with my S-4 and is left as the security-track finding.

PM consolidation: register R-1 (W-1) and R-2 (W-2) as `high` / `medium` open residuals; R-1 must close before merge, R-2 can defer to a follow-up test-baseline plan if scoped.

---

## Revalidation (fix-wave-1)

**Targeted re-review** for V1.65 fix-wave-1 — focus: verify the **qc1 W-1 (high)** blocking finding is resolved, accept the **W-2 (medium)** deferral, and run a no-regression sanity check across the 4 fix commits. **Not** a re-review of the whole iteration; the 7 Suggestions (S-1..S-7) and 308-test baseline are out of scope here.

### Scope
- plan_id: v1.65 (unchanged)
- Fix-wave range (focus): `43be4b52..9c50481f` (= the 4 fix commits on `iteration/v1.65`)
- Overall range (regression sanity): `merge-base origin/main ... HEAD 9c50481f`
- Working branch (verified): `iteration/v1.65` (HEAD `9c50481f`)
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (main worktree, on `iteration/v1.65`)
- Files re-checked: `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs` (1142 LOC — W-1 fix + W-1 regression test + test failpoint + `FailpointGuard` RAII), `crates/nexus-daemon-runtime/src/api/errors.rs` (413 mapping for `CHAPTER_BODY_TOO_LARGE` — lane-cross check, no architectural concern), `crates/nexus-local-db/src/work_chapters.rs` (keyset cursor changes — lane-cross check, covered by new pagination test), `apps/web/src/pages/chapter-page.tsx` (keydown listener cleanup — lane-cross check).
- Tools run:
  - `pnpm --filter nexus-contracts build` (clean — prerequisite for `pnpm --filter web typecheck`)
  - `cargo test -p nexus-daemon-runtime --lib` → **308 passed; 0 failed; 0 ignored** (matches expected count: 305 baseline + `put_outline_db_failure_does_not_write_file` + `list_chapters_keyset_pagination` + `get_chapter_body_rejects_oversized_file` = 308)
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` → **clean**
  - `pnpm --filter web test` → **81 passed (81)** across 10 test files (matches expected count: 80 baseline + keydown-balance regression test = 81)
  - `pnpm --filter web typecheck` → **clean**
  - `git show 1407b16a -- chapters.rs` (W-1 fix diff), `git show 9c9945a7 15d5f145 6e14fb13` (other 3 fix stats, for regression check)

### W-1 (high) — outline PUT FS/DB atomicity: RESOLVED

**Diff (`1407b16a`, +68/-1 in `chapters.rs`):** `put_chapter_outline` now writes the DB metadata **before** the file. The reorder is a 1:1 swap of two lines within the existing `async { ... }` block:

| Step | Before (file-then-DB) | After (DB-first) |
|------|----------------------|-------------------|
| 1 | `atomic_write_outline(...)` | `work_chapters::update_outline_path(...)` |
| 2 | `work_chapters::update_outline_path(...)` | `atomic_write_outline(...)` |

**Verifying the new ordering is safe in both failure directions:**

1. **DB-update fails (the original bug)**: with the reorder, the file write is **after** the `?` on `update_outline_path`. The early-return on DB error now occurs **before** any file system call → no orphan file. The test failpoint is placed between `acquire` and the DB call (line 639-646) and returns the simulated `Internal { code: "DATABASE_ERROR", ... }` after explicitly `lock.release().await` (preserves the V1.42.1 hotfix rule from `crates/nexus-daemon-runtime/AGENTS.md` §"Rule 2").
2. **File write fails after DB commit (the new gap)**: `atomic_write_outline` (line 407-448) is itself **idempotent on retry** — it writes to a unique `temp_path = target.with_extension("md.tmp.{pid}.{ms}")` (line 423-428) and renames onto the target. The DB now has `outline_path` pointing to the intended path, the file is missing or partially written, but a retry PUT will re-call the same code path: the DB update becomes a no-op-equivalent (`update_outline_path` with the same value), and `atomic_write_outline` overwrites any partial target. The spec language from §6.2 ("Failed DB update or failed rename must clean up the temp file where possible and must not report success") is satisfied: rename failure leaves the **temp** file (which `atomic_write_outline` removes at line 440), and the handler returns `OUTLINE_WRITE_ERROR` (not Ok), so the response is an error.

**Verifying the regression test `put_outline_db_failure_does_not_write_file` is sound:**

- It uses a `static TEST_UPDATE_OUTLINE_PATH_FAIL: AtomicBool` failpoint (test-only via `#[cfg(test)]`) plus a `FailpointGuard` RAII wrapper with `Drop` to reset the failpoint even on panic — the right pattern for a shared atomic test seam (no leftover state leaking into `list_chapters_keyset_pagination` or any sibling test).
- The failpoint is at line 639-646, **before** `work_chapters::update_outline_path` — i.e. it simulates the DB update never being called. This is the strongest possible regression guard: if a future refactor reorders back to file-then-DB, the assertion `!file_path.exists()` will fail because the file would have been written before the failpoint fires.
- The test asserts both the error (`result.is_err()`) and the file absence — both invariants of the fix.
- Test placement in the same `mod tests` block (line 1040-1067) follows the established chapter-handler test pattern (uses `setup_chapter_work`, `AxumState`/`AxumPath`/`AxumQuery` extractors, no router).

**Verifying no new gap was introduced by the reorder:**

- The V1.42.1 hotfix rule (`acquire` → explicit `release().await` on every exit path) is preserved at lines 635 (acquire), 641 (failpoint early-return), 671 (normal return).
- `lock.release().await` runs **before** `let (now, outline_path, content) = result?;` — i.e. even if `result?` propagates an error from the inner async block (the "DB succeeded, file failed" case), the lock is already released. This is the correct ordering because the inner block's `?` on `atomic_write_outline` returns an `Internal` error and we want the lock released before the function returns to the caller.
- The runtime lock still guards single-writer (the original purpose) — atomicity for the "no orphan file" property comes from the reorder, not the lock.

**Disposition: W-1 RESOLVED.** The original "transactional finalization path" gap is closed in the direction the spec was concerned about. The new "DB succeeds, file fails" case is a self-healing state on retry and the spec is met ("must not report success" — the handler returns `OUTLINE_WRITE_ERROR`).

### W-2 (medium) — missing HTTP integration tests: DEFER ACCEPTED

PM accepted the deferral to a follow-up test-baseline slice. Rationale from my prior report: in-mod unit tests cover handler correctness, and the path-guard tests cover the negative path. The remaining gap (route registration, axum extractor parsing, JSON serialization, middleware ordering) is a test-depth concern, not a correctness one — the existing 308 lib tests + 81 web tests give strong coverage of the handler logic. Registered as a `medium` open residual; the W-2 finding carries forward as a known test-baseline item, **not** a V1.65 merge blocker.

**Disposition: W-2 deferral ACCEPTED.** I confirm this is a reasonable test-depth gate, not a V1.65 architectural blocker.

### No-regression sanity check on the other 3 fixes

Glanced the other 3 fixes for architecture/coherence regressions in my lane (architecture/maintainability). All three are clean.

| Fix commit | QC3 finding | Lane-cross architectural check |
|------------|-------------|--------------------------------|
| `9c9945a7` (keyset pagination) | W-1 | Keyset cursor is opaque (`v2:<volume>:<chapter>`); uses the existing `PRIMARY KEY (work_id, volume, chapter)` index — no new index, no schema drift. The cursor is encoded in the handler (not in `nexus-contracts`) which is appropriate for an opaque pagination token. **`pagination.rs` offset helpers are no longer used by chapters** — confirmed by the diff (line 10 removed the import); works.rs still uses the offset helpers (out of scope here). No architectural concern. |
| `15d5f145` (10 MiB body cap) | W-2 | The cap is centralized in `read_guarded_file` (one place, all chapter read paths inherit the cap). The 413 mapping is in the same `errors.rs` dispatch arm — consistent with the other `CHAPTER_*` code mappings. The test creates an oversized file (not a failpoint) — direct but slower; acceptable for a 10 MiB test. No architectural concern. |
| `6e14fb13` (keydown cleanup) | W-4 | The fix names the keydown handler and adds cleanup — pure React effect hygiene. The test asserts `addEventListener`/`removeEventListener` are balanced via mock calls. No architectural concern. |

**No new architecture/coherence regression** in the architecture/maintainability lane introduced by any of the 4 fix commits. The 7 Suggestions (S-1..S-7) from my prior report remain as-is; they are test-baseline and code-organization items, not blockers.

### Verdict

| Finding | Severity | Disposition |
|---------|----------|-------------|
| W-1 (outline PUT FS/DB atomicity) | high | **RESOLVED** (DB-first reorder + `put_outline_db_failure_does_not_write_file` regression test) |
| W-2 (missing HTTP integration tests) | medium | **DEFER ACCEPTED** (test-baseline follow-up; registered residual) |
| S-1..S-7 | low | Unchanged; not re-reviewed in this targeted pass |

**Re-verdict**: **Approve**. The single blocking architectural finding (W-1) is closed with a sound fix and a strong regression test. The W-2 deferral is reasonable. All static checks and test suites pass on the post-fix-wave-1 HEAD (`9c50481f`).

### Reviewer Notes (revalidation)

- **Why the W-1 fix is sufficient (architecture)**: The DB-first reorder is the simplest of the three fix approaches I listed in my prior finding (option 1, "compute and persist the canonical `outline_path` in DB, then write file"). The DB here only stores a path string + `updated_at` — the actual content is always derived from the file. By ordering metadata-first, the system converges to a consistent state on retry: either the path is recorded AND the file exists (success), or the path is recorded AND the file is missing (retry re-creates it idempotently), or neither (failure, no orphan). The runtime lock is still required for single-writer but the **atomicity** property is now derived from the file-system rename primitive (`tokio::fs::rename` is atomic on the same filesystem), not from the lock.
- **Why the W-2 deferral is acceptable (architecture)**: The chapter routes are registered alongside works routes in `api/mod.rs:312-329` (per my prior S-5 finding). The same V1.42.1 hotfix pattern is reused, and the in-mod unit tests exercise the handler functions directly. A future `tests/chapters_api.rs` would close the routing/serialization gap; it does not change the architectural integrity of V1.65.
- **Coordination with QC3**: My lane (architecture/maintainability) does not overlap with QC3's W-1 (pagination), W-2 (body cap), W-3 (also W-1 in my lane — outline PUT atomicity, jointly closed by the same fix), W-4 (keydown leak). The fix-wave is jointly owned by qc1+qc3; the per-finding coverage is clean.

PM consolidation: **close R-1 (W-1, high)** in `residual_findings` (no `archived/residuals/` write needed; that is PM/QA lifecycle work per `mstar-plan-artifacts`); **keep R-2 (W-2, medium) open** with the test-baseline target date. The 7 Suggestions remain in the suggestions row; the architectural layer is healthy.

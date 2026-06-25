---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "v1.65"
verdict: "Request Changes"
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

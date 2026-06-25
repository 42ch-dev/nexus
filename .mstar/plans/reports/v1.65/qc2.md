---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "v1.65"
verdict: "Approve"
generated_at: "2026-06-25"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1 (per opencode.json qc-specialist-2 config)
- Review Perspective: Security and correctness risk (path traversal, write safety, auth, concurrency, status transitions, input validation, dependency advisories)
- Report Timestamp: 2026-06-25

## Scope
- plan_id: v1.65
- Review range / Diff basis: `merge-base 644acbc56856d03e8e3aaf2139f73dccfcf6ed54 ... HEAD 73e3343081ffa415b221252b5432dc1c6e21f07b` (= `git diff origin/main...HEAD`; 112 files, +8902/-422)
- Working branch (verified): `iteration/v1.65`
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 112 (focus on daemon-runtime chapter handlers, path guards, lock usage, auth middleware, contracts surface, dep manifests)
- Commit range: 644acbc5..73e33430 (feature merges for P0 + P-sec + P1 + P2)
- Tools run:
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git merge-base`, `git log --oneline`, `git diff --stat`, `git diff --name-only`
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` (clean)
  - `cargo test -p nexus-daemon-runtime --lib chapters::` (6/6 passed)
  - `pnpm --filter nexus-contracts build` (required prerequisite)
  - `pnpm --filter web typecheck` (clean after contracts build)
  - `gh api repos/42ch-dev/nexus/dependabot/alerts -q '[.[]|select(.state=="open")]|length'` → 9 (pre-merge open count)
  - `cargo tree -i rand:0.7.3` → "did not match any packages" (0.8/0.9 only)
  - Multiple `read` + `grep` on `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs`, `auth_middleware.rs`, `mod.rs`, `pagination.rs`, `preset_management.rs`, `host_tool_handlers.rs`, `work_chapters.rs`, route definitions, and dep manifests.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- **S-001 (write-path guard strength)**: `resolve_guarded_path(..., must_exist=false)` for outline PUT uses ancestor-probe + prefix check on a canonical ancestor, then returns the un-canonicalized `joined` path for the subsequent `atomic_write_outline` + temp/rename. Reads use full `canonicalize` + prefix on the target. The write path is defense-in-depth for DB-sourced `outline_path` values (or fallback construction under `Works/{work_ref}/...`). A TOCTOU window exists between ancestor probe and FS write/rename (symlink swap on an ancestor component could theoretically bypass). In the local single-user loopback threat model this is low-risk (attacker with FS write already owns the workspace), and the guard correctly rejects `../escape`, absolute paths, and empty inputs in current tests. Consider hardening the write path to a post-mkdir canonicalization + final prefix check (or use `std::fs::canonicalize` after `create_dir_all` before the temp write) for parity with the read path and W-002 body guards in `host_tool_handlers.rs`. No escape demonstrated; current behavior is acceptable for V1.65.
- **S-002 (work_profile validation)**: `work_profile` is accepted as `Option<String>` on create/patch and stored directly (migrations extend the set: novel, essay, game_bible, script). No server-side closed enum whitelist observed in the handler paths reviewed. If the Local API contract / spec intends a fixed vocabulary, add explicit validation (reject unknown values with a clear error) to prevent future drift. Current usage appears intentional for extensibility.
- **S-003 (directory fsync after rename)**: `atomic_write_outline` does `file.sync_all()` on the temp file before `rename`, which is good for content durability. On some filesystems a subsequent `fsync` on the parent directory after rename provides stronger post-crash visibility guarantees. Minor; not a correctness or security issue for the observed usage.
- **S-004 (open dependabot count)**: Pre-merge open alerts = 9 (recorded via `gh api`). P-sec bumps (vitest 3.2.6, vite 6.4.3, wiremock 0.6) eliminated the specific rand 0.7.3 advisory (`cargo tree -i` empty). The residual 9 are outside the scope of the targeted dep upgrades in this plan.

## Source Trace
- Finding ID: (N/A — no Critical/Warning)
- Source Type: static code review + targeted test/lint execution + command output
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:158` (resolve_guarded_path), `505` (put_chapter_outline), `579` (patch_chapter), `679` (get_chapter_body); `api/mod.rs:322-328` (routes); `RuntimeLockGuard` usage at 543/565 and 643/675; `auth_middleware.rs`; `pagination.rs:37` (cursor decode); dep manifests + `cargo tree`; test at `chapters.rs:792`.
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

## Additional Notes (Security + Correctness Focus)

**Path traversal / file-write safety (highest priority item)**:
- `resolve_guarded_path` implements a W-002-style guard for both reads (`must_exist=true`, full canonicalize + starts_with) and writes (`must_exist=false`, ancestor walk + prefix on first canonical ancestor, return joined).
- PUT `/.../chapters/{n}/outline` sources `outline_path` from the chapter DB row (or safe fallback under `Works/{work_ref}/Outlines/...`), then calls `atomic_write_outline` which invokes the guard, `create_dir_all`, temp write + `sync_all` + `rename`.
- Escape attempts (`../`, absolute paths) are rejected at guard time (test `resolve_guarded_path_accepts_inside_and_rejects_escape` + logic for parent walk hitting "no parent inside").
- Workspace root is taken from the *current* `WorkspaceState` at request time; re-checked on every mutating call.
- Atomic rename provides crash-safety for the content. No body-write surface was added under the chapter content routes (body remains read-only GET; writes stay in the orchestration/host-tool path which has its own W-002 guards).
- No demonstrated escape or workspace-switch bypass. The ancestor-probe write path is slightly weaker than the read path (TOCTOU + returns non-canonical joined); documented as Suggestion above.

**Body read-only enforcement**:
- Only `GET /v1/local/works/{work_id}/chapters/{n}/body` exists in the router. No PUT/POST/PATCH body endpoint was introduced in V1.65 chapter content surface. Confirmed in route table and handler inventory. Orchestration body writes continue to use the separate host tool surface.

**Status-transition correctness**:
- `validate_status_transition`: only `not_started → outlined` is allowed server-side. Other transitions return `CHAPTER_STATUS_TRANSITION_INVALID`.
- PATCH structural edits on `published` → hard `CHAPTER_STRUCTURE_EDIT_BLOCKED`.
- PATCH on `finalized` without `confirm_structural_edit=true` → `CHAPTER_STRUCTURE_CONFIRMATION_REQUIRED`.
- `title` field in PATCH is explicitly rejected with `CHAPTER_TITLE_UNSUPPORTED` (display-only in V1.65).
- Protection metadata (`ChapterProtection`) is computed server-side from status in `to_detail` / `chapter_protection`.
- All gates are in the handler before lock acquisition; not UI-trust only.

**Concurrency / runtime-lock**:
- Both mutating chapter routes (PUT outline, PATCH structure) acquire `RuntimeLockGuard` *after* existence checks (`load_work`, get_chapter) and *before* any FS or DB mutation — matches the V1.42.1 hotfix rule documented in `nexus-daemon-runtime/AGENTS.md`.
- Explicit `lock.release().await` on the success path immediately after the mutation block; error paths inside the `async { ... }?` blocks still execute release before the outer `?`.
- Same `RuntimeLockGuard` (per-work) used by orchestration body writes → soft-concurrency model is consistent; no new deadlock or leak vectors introduced.
- Drop path only warns (async Drop limitation); all call sites honor explicit release.

**Dependency security (P-sec)**:
- `apps/web/package.json`: `"vitest": "^3.2.6"`, `"vite": "^6.4.3"`.
- Rust crates: `wiremock = "0.6"` (nexus-acp-host, nexus-cloud-sync, nexus42).
- `cargo tree -i rand:0.7.3` returns no match (only 0.8.6 / 0.9.4 present) — the targeted advisory is closed.
- Pre-merge open dependabot alerts: 9 (recorded).

**Input validation**:
- Cursor pagination (`ListChaptersQuery`): `decode_offset_cursor` enforces `v1:` prefix + non-negative integer parse → `INVALID_INPUT` on malformation. `limit` clamped `.min(100)`.
- Chapter number: `parse_chapter` enforces >= 1.
- Preset YAML validate path enforces size (1 MiB) and nesting depth (10).
- `work_profile` stored as free-form string (see Suggestion S-002).
- No new injection surfaces; paths are always joined under a guarded root and validated before FS ops.

**Auth model**:
- All chapter routes live under the protected router section (after `require_api_key` middleware in `api/mod.rs`).
- Keyless-localhost model (V1.20): loopback accepted with empty/unset key; non-loopback rejected with 403. No key material is leaked or required on the chapter surface.
- Creator/work ownership is re-verified on every request via `read_active_creator_id` + `load_work`.
- Consistent with the rest of the Local API; no unguarded chapter endpoints outside the intended scope.

**Build / verification prerequisites met**:
- `pnpm --filter nexus-contracts build` executed before web typecheck.
- `cargo clippy -p nexus-daemon-runtime -- -D warnings`: clean.
- Targeted chapter lib tests: 6 passed (including path-guard escape test and the three handler round-trips).
- Web typecheck: clean.

**No other mutating chapter-content endpoints** were added that bypass the reviewed guards or lock discipline.

## Revalidation Notes
N/A (initial wave).

**Verdict**: Approve (Critical = 0, Warning = 0).

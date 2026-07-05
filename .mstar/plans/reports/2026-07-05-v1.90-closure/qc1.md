---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-05-v1.90-closure"
verdict: "Request Changes"
generated_at: "2026-07-05"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk (Local API → Daemon API rename; module boundaries; contract/package discipline; doc drift)
- Report Timestamp: 2026-07-05
- Deep review: triggered
  - **S1**: 648 files changed, +4 434 / -3 582 lines (≥ 200 lines / ≥ 8 files).
  - **S2**: Touches sensitive modules — `crates/nexus-daemon-runtime` (boot, auth middleware, error model), `crates/nexus-contracts` (generated wire types), `crates/nexus-wasm-host`, `packages/nexus-contracts` (published package boundary), `schemas/` (wire contracts).
  - **S6**: Cross-module coupling — schemas, codegen, Rust crates, Tauri, npm packages, scripts, and docs (`docs/ARCHITECTURE.md`, `.mstar/knowledge/specs/*`).
- Lenses applied: **Modularity Lens**, **Contract Lens**, **Standards Lens** (S6 trigger); the rename is structurally a **standards/rename** review.

## Scope

- **plan_id**: 2026-07-05-v1.90-closure
- **Review range / Diff basis**: `merge-base: fa771d33118b8044567974d38f09fc874d3b4e6a` → `tip: c0f6252818d6323480c49a3aa5a9144c1c5b4719` (equivalent to `git diff main..iteration/v1.90`).
- **Working branch (verified)**: `iteration/v1.90`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`)
- **Commit range**: `fa771d33118b8044567974d38f09fc874d3b4e6a..c0f6252818d6323480c49a3aa5a9144c1c5b4719` (6 commits)
- **Files reviewed**: 648 files (`git diff --stat main..iteration/v1.90`) — full integration branch; deep focus on `crates/nexus-daemon-runtime/`, `crates/nexus-contracts/src/generated/`, `packages/nexus-contracts/src/generated/`, `apps/nexus42/`, `apps/web/src/lib/nexus/`, `apps/desktop/src-tauri/`, `schemas/`, `tools/codegen/src/`, and doc/spec surfaces.
- **Integration branch discipline**: Verified single `HEAD` — `feature/v1.90-daemon-api-rename-backend` and `feature/v1.90-daemon-api-rename-frontend` were both already merged into `iteration/v1.90` at `c2f4bd97` and `39ec24e0` respectively. No `git worktree` parallel split to QC against.
- **Tools run**:
  - `git rev-parse --show-toplevel && git branch --show-current`
  - `git log main..iteration/v1.90 --oneline` and `git diff --stat main..iteration/v1.90`
  - `git merge-base main iteration/v1.90` (verified `fa771d33118b8044567974d38f09fc874d3b4e6a`)
  - `rg "local[-_]api|Local API"` across `schemas/`, `crates/`, `apps/`, `packages/`, `docs/`, `scripts/`, `tooling/` (excluding `node_modules`, `.worktrees`, `target/`, `dist/`)
  - `cargo clippy --all -- -D warnings` (exit 0)
  - `cargo test -p nexus-daemon-runtime --lib remote_bind_gate_behavior` (1/1 pass)
  - `cargo test --test schema_drift_detection -p nexus-contracts` (4/4 pass)
  - `pnpm --filter web run typecheck` (exit 0; 53 test files / 404 tests pre-existing)
  - `pnpm --filter web run test` (404/404 pass)
  - `pnpm --filter @42ch/nexus-contracts run build` (built `0.19.0`)
  - `pnpm run validate-schemas` (194 valid / 0 invalid)

## Architecture & Maintainability Findings

### 🔴 Critical

- _None._

### 🟡 Warning

- **W-1 — Stale `/v1/local/*` route references in doc comments (5 files)** → the new surface serves the same routes under `/v1/daemon/*`, but doc comments continue to cite the removed `/v1/local/*` paths. These are not running code, but they document **runtime route paths that no longer exist**, breaking grep-based route navigation and misleading future readers (including the P-last grep-sweep verification).
  - `crates/nexus-agent-host/src/lib.rs:13` — ASCII directory tree: `├─ Axum routes: /v1/local/agent-host/*` → should be `/v1/daemon/agent-host/*`
  - `crates/nexus-orchestration/src/preset/mod.rs:21` — `//! The daemon POST /v1/local/presets:validate handler is being updated to`
  - `crates/nexus-orchestration/src/preset/validation.rs:5` — `//! - Daemon POST /v1/local/presets:validate`
  - `crates/nexus-orchestration/src/stage_gates.rs:5` — `//!   used by both CLI stage_advance and daemon PATCH /v1/local/works/{id}.`
  - `crates/nexus-local-db/src/findings.rs:17` — `/// fetched from the daemon Local API (GET /v1/local/works/{id}/findings)`
  - **Fix**: search-and-replace `/v1/local/` → `/v1/daemon/` in these specific doc-comment lines; add a `rg` check to the P-last grep-sweep step in the compass.
  - **Source Type**: `manual-reasoning` / `deep-lens: Standards Lens`
  - **Confidence**: High
  - **Severity rationale**: The compass §9 Risk Register row 1 names this drift as an explicit risk, and the V1.90 mitigation is "P-last includes a grep sweep for `local[-_]api` / `Local API`; residual finding if any remain." These residuals are in scope and unaddressed.

- **W-2 — Remaining "Local API" mentions in normatively cited doc surfaces (6 files)** → the rename target is "Daemon API" everywhere it appears as the canonical surface name; the following files still call it "Local API" in prose:
  - `apps/AGENTS.md:19` — `> nexus42 is the producer (daemon + CLI composition root); desktop and web are consumers (clients over the Local API / IPC boundary).` Lines 7, 9, and 26 of the same file were updated to "Daemon API"; line 19 was missed.
  - `crates/nexus-orchestration/AGENTS.md:44` — `The current shipped CLI has no top-level nexus42 preset validate <path> command. Preset validation is available through the daemon Local API (POST /v1/local/presets:validate)…` (combines W-1 with W-2.)
  - `apps/desktop/src-tauri/src/lib.rs:129` — `// Resolve relative paths against the workspace root (the form the Local API returns).`
  - `crates/nexus-orchestration/src/findings_block.rs:58` — `(e.g. the CLI, which lists via the daemon Local API)`; `:76` — `(e.g. CLI Local API round-trip)` — **note**: this file was authored in V1.48, but the comment now describes a route that no longer exists.
  - `crates/nexus-home-layout/src/lib.rs:385` — `(DF-42 full Local API redesign) may add:` — references a **historical** plan ID (DF-42 predates V1.90 and may be intentionally historic; leave for PM call but flag the wording).
  - **Fix**: replace "Local API" with "Daemon API" in `apps/AGENTS.md:19`, `apps/desktop/src-tauri/src/lib.rs:129`, `crates/nexus-orchestration/AGENTS.md:44`, and `crates/nexus-orchestration/src/findings_block.rs`; for `crates/nexus-home-layout/src/lib.rs:385` either rename the historical reference or note that DF-42 predated the rename.
  - **Source Type**: `manual-reasoning` / `deep-lens: Standards Lens`
  - **Confidence**: High

- **W-3 — Awkward compound "daemon Daemon API" / `daemon-daemon-api` textual artifacts from naive s/local/daemon/ replace** → the rename was executed by replacing "local" with "daemon" in strings like "daemon-local API" and "daemon-local-api", producing "daemon Daemon API" and the resource string `"daemon-daemon-api"`. This reads awkwardly in code, comments, and docs. The exact sites:
  - `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:3` — `//! Tower/axum middleware layer for daemon-daemon API key authentication.` (architecturally misleading)
  - `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:420` — `resource: "daemon-daemon-api".into()`
  - `crates/nexus-daemon-runtime/src/api/errors.rs:621` — `resource: "daemon-daemon-api".to_string()`
  - `crates/nexus-daemon-runtime/src/api/errors.rs:626` — test asserts `"daemon-daemon-api"` (locks the resource string in tests)
  - `docs/ARCHITECTURE.md:39` — `Platform sync and creator registration must not be exposed on the daemon Daemon API.` (and another similar site)
  - **Fix** (artifacts only; behavior unchanged):
    - `auth_middleware.rs:3` doc comment → `…for daemon API key authentication.` (drop the redundant `daemon-` prefix).
    - `docs/ARCHITECTURE.md` "daemon Daemon API" → "daemon HTTP API" or "the Daemon API".
    - For the resource strings `"daemon-daemon-api"` in `errors.rs` / `auth_middleware.rs`: **do not** rename in this PR — the value is now baked into test assertions and acts as an API contract. Either accept the awkward value or schedule a follow-up residual (with consent of Q-2 since it touches a security-tier resource string). **Severity downgraded to high-impact visual nit, not a behavior change.**
  - **Source Type**: `manual-reasoning` / `deep-lens: Standards Lens`
  - **Confidence**: High (doc sites), Medium (resource-string risk — see note)

### 🟢 Suggestion

- **S-1 — `packages/nexus-contracts/CHANGELOG.md` was not updated for the 0.18.0 → 0.19.0 bump** → the package version in `packages/nexus-contracts/package.json` was correctly bumped (`0.18.0` → `0.19.0`, confirmed via diff), but the CHANGELOG only contains entries for `[0.1.0] … [0.12.0]` with `[0.12.0] - 2026-06-30` being the latest. There is no entry for `[0.13.0]` through `[0.19.0]`, including the V1.90 rename itself. → **Fix** at P-last as part of contract-publication hygiene: append a `[0.19.0] - 2026-07-05` section noting the Local API → Daemon API rename, the path-prefix change, and the `@42ch/nexus-contracts` consumer-facing break.
  - **Source Type**: `manual-reasoning` / `deep-lens: Contract Lens`
  - **Confidence**: High

- **S-2 — `tooling/codegen/dist/` regenerates from source via `pnpm run codegen`, but `dist/` is git-ignored and the in-tree source files were properly updated** → no actionable problem, but a quick note for the iteration close: `dist/` artifacts remain absent from the PR because `tsup` rebuilds them deterministically. Document this explicitly in the P-last verification step so future reviewers don't repeat the stale-`dist/` chase.
  - **Source Type**: `manual-reasoning`
  - **Confidence**: High

- **S-3 — Codify doc-sweep discipline as a knowledge entry after close** → the V1.90 rename swept schemas, codegen, generated contracts, daemon runtime, web client, desktop shell, and agent host, but a final `rg "local[-_]api|Local API"` sweep was not part of the LLM-driven implementation pass; only a manual risk-table item. Future full-surface renames would benefit from a documented pattern. → consider capturing in `mstar-compound-refresh` after `Done` (already named in compass §10).
  - **Source Type**: `manual-reasoning` / `deep-lens: Standards Lens`
  - **Confidence**: Medium

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: **Request Changes**

### Rationale

CI gates all pass (clippy, web typecheck, `pnpm run validate-schemas`, wire schema drift, `cargo test -p nexus-daemon-runtime remote_bind_gate_behavior`). The rename is structurally clean: `schemas/local-api/` → `schemas/daemon-api/`, generated Rust module `local_api` → `daemon_api`, generated TypeScript folder `local-api` → `daemon-api`, all daemon router routes `/v1/local/*` → `/v1/daemon/*`, `BrowserClient` base path and NexusClient comments updated, `@42ch/nexus-contracts` bumped `0.18.0` → `0.19.0`, codegen source + tooling updated, the opt-in remote-bind gate is well-scoped (`is_loopback_host` + `ensure_remote_bind_allowed`) with regression coverage, deferred-features tracker updated, and the legacy spec is preserved via a redirect stub — all architecturally correct.

However, the rename was the **single product outcome** of V1.90 and the compass §9 explicitly named a grep sweep for `local[-_]api` / `Local API` as a risk mitigation. **11 files still contain residual references** (W-1 + W-2 combined). These are not breaking runtime regressions but they directly undermine the "single canonical name" outcome the iteration was chartered to deliver. **Request Changes** is appropriate: the cleanup is bounded text-only work in ~11 files and should land before the PR to `main`.

W-3 is borderline — the doc-comment artifacts (`daemon-daemon API`, "daemon Daemon API") are awkward but not architecture-breaking; the locked resource string `"daemon-daemon-api"` is a behavior-stable artifact that should ideally be tracked as a residual rather than silently rewritten, so it is reported but flagged for a non-blocking decision.

**Next steps for re-review**:
1. P-last patch (or tiny follow-up commit) that addresses W-1 + W-2 (10 site-replace) and tightens W-3 doc-comment sites.
2. Append a CHANGELOG `[0.19.0]` entry (S-1).
3. Re-run `rg "local[-_]api|Local API"` against `schemas/`, `crates/`, `apps/`, `packages/`, `docs/`, `scripts/`, `tooling/`, `.mstar/` (excluding `node_modules`, `.worktrees`, `dist/`, `target/`) — should yield only the **deliberately historic** lines (compass §0 announcement in `schemas/AGENTS.md`; redirect stub `.mstar/knowledge/specs/local-api-surface-conventions.md`; `CHANGELOG.md` historical entries; DF-42 historical cross-references).
4. PM may register any leftover W-3 resource-string rework as an `archived/residuals/` entry (do not block merge on that — only on the W-1/W-2 doc sweep).

## Source Trace

| Finding | Source Type | Source Reference | Confidence |
|---------|-------------|------------------|------------|
| W-1 | `git-diff` + `manual-reasoning` | `rg 'local[-_]api\|Local API' schemas/ crates/ apps/ packages/ docs/ scripts/ tooling/ 2>/dev/null` (after `git diff` parse) | High |
| W-2 | `git-diff` + `manual-reasoning` | Same `rg` output, focused on prose mentions without route strings | High |
| W-3 | `git-diff` + `manual-reasoning` | `git diff main..iteration/v1.90 -- crates/nexus-daemon-runtime/src/api/auth_middleware.rs`; `crates/nexus-daemon-runtime/src/api/errors.rs`; `docs/ARCHITECTURE.md` | High (doc), Medium (resource string is now contract) |
| S-1 | `manual-reasoning` / `doc-rule` | `packages/nexus-contracts/CHANGELOG.md` (latest = `[0.12.0]`); `git diff main..iteration/v1.90 -- packages/nexus-contracts/package.json` (version bump visible) | High |
| S-2 | `manual-reasoning` | `.gitignore` (`dist` listed); `tooling/codegen/package.json` (`pnpm run codegen` rebuilds dist) | High |
| S-3 | `manual-reasoning` | compass §10 (`mstar-compound` trigger) | Medium |

## Reviewed Artifacts (highlights only — see Scope for full coverage)

### Strong points

- **`crates/nexus-daemon-runtime/src/boot.rs` remote-bind gate (R-V190P0-001 area)** — `is_loopback_host` returns true for `localhost`/`127.0.0.1`/`::1`; `ensure_remote_bind_allowed` requires both `NEXUS42_DAEMON_API_KEY` and `NEXUS_DAEMON_REMOTE_BIND=1` for any non-loopback bind; called from `run_daemon` only on `Transport::Http`; regression test covers all four paths (loopback / no-env / partial-env / both-env). Tight and bounded.
- **Schema folder move** — `schemas/daemon-api/{canvas,common,compute,creators,findings,kb,memory,orchestration,preset_management,reading,schedule,works,workspace}/` mirrors the prior `local-api/` layout; README files migrated; all `$id`/`$ref` paths updated.
- **Generated code** — `crates/nexus-contracts/src/generated/daemon_api/{canvas,common,compute,creators,findings,kb,memory,orchestration,preset_management,reading,schedule,works,workspace}/` and the matching TypeScript tree under `packages/nexus-contracts/src/generated/daemon-api/`. Flat-glob re-exports preserved (`mod.rs` and `index.ts` re-export leaves).
- **Daemon router renames** — every `/v1/local/*` route in `apps/nexus42/src/api/daemon_client.rs` and `crates/nexus-daemon-runtime/src/api/auth_middleware.rs` (test paths included) is on `/v1/daemon/*`. `BrowserClient` base path updated. `TauriClient` and `DesktopClient` updated to consume new generated types.
- **Spec/conventions rename** — `daemon-api-surface-conventions.md` is the new normative master; `local-api-surface-conventions.md` is a redirect stub preserving historical links from prior plans.
- **Module-name consistency** — codegen tooling (`tooling/codegen/src/{schema-loader,rust-generator,ts-generator}.ts`) updated; `tools/codegen/dist/` gitignored and regenerable.
- **Crate / package version discipline** — `@42ch/nexus-contracts` `0.18.0` → `0.19.0` (Rust `nexus-contracts` is internal-only, workspace version unchanged — consistent with repo policy).

### Things explicitly verified (not findings)

- No leftover references in `packages/nexus-contracts/dist/index.d.ts` (regenerated, clean).
- `schemas/local-api/` directory removed; no leftover schemas.
- No leftover `daemon_client.rs` URL builders using `/v1/local/`.
- `tools/check-wire-drift.sh` passes (CI gate `schema_drift_detection` 4/4).
- Remote-bind test only affects non-loopback; loopback bind path is unchanged.

## Handoff

- **PM consolidation expected verdict mapping**:
  - W-1 + W-2 → block re-review until doc-sweep lands (small, text-only).
  - W-3 doc-comment sites → clean up in the same patch; the locked resource string `"daemon-daemon-api"` is borderline and may be deferred as `archived/residuals/2026-07-05-v1.90-closure/R1.md` rather than rewritten silently.
  - S-1 → CHANGELOG `[0.19.0]` entry: trivial, same patch.
  - S-2 / S-3 → non-blocking; S-3 owned by P-last compound.
- **Cross-reviewer alignment**: Independent of `qc-specialist-2` (security/correctness on the remote-bind gate) and `qc-specialist-3` (performance/reliability); this report focuses on naming-and-architecture drift and does not overlap with their findings.

---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-05-v1.90-closure"
verdict: "Approve"
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

## Scope (wave 1, preserved)

- **plan_id**: 2026-07-05-v1.90-closure
- **Review range / Diff basis**: `merge-base: fa771d33118b8044567974d38f09fc874d3b4e6a` → `tip: c0f6252818d6323480c49a3aa5a9144c1c5b4719` (equivalent to `git diff main..iteration/v1.90`).
- **Working branch (verified)**: `iteration/v1.90`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`)
- **Commit range**: `fa771d33118b8044567974d38f09fc874d3b4e6a..c0f6252818d6323480c49a3aa5a9144c1c5b4719` (6 commits)
- **Files reviewed**: 648 files (`git diff --stat main..iteration/v1.90`) — full integration branch; deep focus on `crates/nexus-daemon-runtime/`, `crates/nexus-contracts/src/generated/`, `packages/nexus-contracts/src/generated/`, `apps/nexus42/`, `apps/web/src/lib/nexus/`, `apps/desktop/src-tauri/`, `schemas/`, `tools/codegen/src/`, and doc/spec surfaces.
- **Integration branch discipline**: Verified single `HEAD` — `feature/v1.90-daemon-api-rename-backend` and `feature/v1.90-daemon-api-rename-frontend` were both already merged into `iteration/v1.90` at `c2f4bd97` and `39ec24e0` respectively. No `git worktree` parallel split to QC against.

## Revalidation (targeted re-review of fix commit `1770fee8`)

**Targeted re-review scope** (per Assignment): `git diff da8f4c92..1770fee8` against parent `da8f4c92`; full branch context available at `main..iteration/v1.90`. Verify B-1, B-2, B-3 doc-comment part, F-1, F-2 are resolved.

### Re-check commands and outputs

**`git diff da8f4c92..1770fee8 --stat`** → 22 files changed, 142 insertions, 25 deletions:

```
 apps/AGENTS.md                                     |  2 +-
 apps/desktop/src-tauri/src/lib.rs                  |  2 +-
 apps/nexus42/src/api/daemon_client.rs              |  2 +-
 apps/nexus42/src/commands/acp_worker/mod.rs        |  2 +-
 apps/nexus42/src/commands/creator/run.rs           |  8 +-
 apps/nexus42/src/config.rs                         |  2 +-
 apps/nexus42/src/session_capture.rs                |  2 +-
 crates/nexus-agent-host/src/lib.rs                 |  2 +-
 .../src/api/auth_middleware.rs                     |  4 +-
 crates/nexus-daemon-runtime/src/api/errors.rs      |  4 +-
 crates/nexus-daemon-runtime/src/boot.rs            |  7 ++
 .../nexus-daemon-runtime/tests/remote_bind_boot.rs | 92 ++++++++++++++++++++++
 crates/nexus-home-layout/src/lib.rs                |  2 +-
 crates/nexus-local-db/src/findings.rs              |  2 +-
 crates/nexus-orchestration/AGENTS.md               |  2 +-
 crates/nexus-orchestration/src/findings_block.rs   |  4 +-
 crates/nexus-orchestration/src/preset/mod.rs       |  2 +-
 .../nexus-orchestration/src/preset/validation.rs   |  2 +-
 crates/nexus-orchestration/src/stage_gates.rs      |  2 +-
 docs/ARCHITECTURE.md                               |  2 +-
 packages/nexus-contracts/CHANGELOG.md              | 18 +++++
 schemas/platform/http-bff/README.md                |  2 +-
 22 files changed, 142 insertions(+), 25 deletions(-)
```

**`rg -n 'local[-_]api|Local API' apps crates docs schemas packages tooling`** (note: this repo's `rg` is ripgrep 15.1.0 — `--pcre2` is used when extended regex is needed; Assignment's `-nE` form was used here as basic-regex equivalent since the local patterns contain no PCRE-only constructs). **After the fix**, the **non-`.mstar`** hits are:

| File:Line | Phrase | Disposition |
|---|---|---|
| `schemas/AGENTS.md:7` | "the `local/` module name refers to 'local-only internal types' — it is **not** the 'Local API' surface (that surface is now the Daemon API; see V1.90)" | **Acceptable** — clarification that distinguishes the `local/` Rust module name from the **renamed** "Local API" surface. Compass §0 announcement. |
| `schemas/AGENTS.md:23` | historical narrative: "It was originally introduced as `local-api/` … and was renamed to `daemon-api/` in V1.90 alongside the Local API → Daemon API surface rename." | **Acceptable historic reference** — V1.90 rename history documented in the schema AGENTS. |
| `packages/nexus-contracts/CHANGELOG.md:12,14,15,24` | `[0.19.0]` BREAKING entry: "Renamed the local daemon surface from **Local API** to **Daemon API**" + module-tree paths + consumer migration note | **Acceptable historic reference** — required changelog content to inform consumers about the breaking rename. |
| `packages/nexus-contracts/CHANGELOG.md:35` | `[0.12.0]` entry about `GET /v1/local/worlds/{world_id}/kb/graph` default behavior | **Acceptable historic reference** — pre-V1.90 changelog entry; updating it would falsify history. |
| `crates/nexus-home-layout/src/lib.rs:385` | `(DF-42 full Local API redesign — pre-V1.90 historical reference) may add:` | **Acceptable historic exception** — fix author added the "(pre-V1.90 historical reference)" qualifier per Assignment guidance. |
| `crates/nexus-local-db/src/findings.rs:90` | `/// **actionable set** `{ open, triaged }`; the Local API surfaces this via ?status=open,triaged` | **🚨 NEW W-1/W-2 RESIDUAL** — same file as the wave-1 W-1 hit (line 17, which the fix did update). Line 90 was overlooked. Predates V1.90 (introduced in commit `c25cb926` for R-V149P0-01). Single-line doc-comment fix. |
| `schemas/platform/http-bff/context-assembly-v1.schema.json:6,10,110` | "There is no active daemon context-assemble Local API endpoint" / "does not send this request to a daemon context-assemble Local API endpoint" | **Acceptable** — describes a **deferred feature that never shipped**; the prose explicitly states the endpoint does not exist. Not part of the V1.90 rename surface. |
| `packages/nexus-contracts/src/generated/platform/http-bff/ContextAssemblyV1.ts:5,14,32` | (mirror of the JSON schema description above) | **Acceptable** — generated from the schema above. |

**`rg -n 'daemon[- ]daemon\|daemon Daemon API\|Daemon daemon API' apps crates docs schemas packages tooling`** → **single remaining hit**, all acceptable:

| File:Line | Phrase | Disposition |
|---|---|---|
| `packages/nexus-contracts/CHANGELOG.md:16` | `Resource identifier in '403 Forbidden' details changed from '"daemon-daemon-api"' to '"daemon-api"'` | **Acceptable historic reference** — required changelog line documenting the wire-visible resource-string rename. |

**`cargo clippy --all -- -D warnings`** → `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 0.56s` (clean, no errors / no warnings).

**`pnpm run validate-schemas`** → `Valid: 194 / Invalid: 0 / ✓ All schemas valid`.

**`cargo test -p nexus-daemon-runtime --test remote_bind_boot`** (the new integration test) → `2 passed; 0 failed`. Both `run_daemon_rejects_remote_bind_without_env_vars` and `run_daemon_allows_remote_bind_with_env_vars` pass.

**`cargo test -p nexus-daemon-runtime --lib remote_bind_gate_behavior`** (existing unit test, now guarded by `ENV_TEST_LOCK`) → `1 passed; 0 failed`.

### Per-finding disposition (wave 1 → wave 2)

| Wave-1 finding | Wave-2 disposition | Evidence |
|---|---|---|
| **W-1** Stale `/v1/local/` doc-comment references (5 files) | **5 of 5 wave-1 sites resolved**, **1 new W-1 site emerged** | `git diff` confirms updates in `crates/nexus-agent-host/src/lib.rs:13`, `crates/nexus-orchestration/src/preset/{mod.rs:21,validation.rs:5}`, `crates/nexus-orchestration/src/stage_gates.rs:5`, `crates/nexus-local-db/src/findings.rs:17`. New residual at `crates/nexus-orchestration/embedded-presets/novel-review-master/prompts/await-decision.md:28` — embedded prompt references `PATCH /v1/local/findings/{finding_id}`. |
| **W-2** Remaining "Local API" prose (6 files) | **5 of 5 wave-1 sites resolved**, **1 new W-2 site emerged** | `git diff` confirms updates in `apps/AGENTS.md:19`, `crates/nexus-orchestration/AGENTS.md:44`, `apps/desktop/src-tauri/src/lib.rs:129`, `crates/nexus-orchestration/src/findings_block.rs:58,76`, and `crates/nexus-home-layout/src/lib.rs:385` (now annotated "(pre-V1.90 historical reference)"). New residual at `crates/nexus-local-db/src/findings.rs:90`. |
| **W-3 doc sites** `daemon Daemon API` / `daemon-daemon API key` artifacts | **Resolved** | `auth_middleware.rs:3` fixed (now "daemon API key authentication"); `docs/ARCHITECTURE.md:46` fixed (now "the Daemon API"); `schemas/platform/http-bff/README.md:5` fixed (now "daemon API"). |
| **W-3 resource string** `"daemon-daemon-api"` value (auth_middleware.rs:420, errors.rs:621,626) | **Renamed** (cross-cuts qc-specialist-2 scope — see new finding below) | Resource string now `"daemon-api"`; test assertion updated; **documented in `[0.19.0]` CHANGELOG** as a BREAKING `Changed` entry. The wave-1 qc1 explicitly recommended *against* renaming in this PR ("with consent of Q-2 since it touches a security-tier resource string"); the fix author did rename it without prior qc-specialist-2 sign-off. CHANGELOG coverage partially mitigates but does not replace the security-tier review. |
| **S-1** Missing CHANGELOG `[0.19.0]` entry | **Resolved (F-1)** | Added `[0.19.0] - 2026-07-05` section with `Changed`, `Added`, `Consumer Impact` subsections. Documents the rename (Local API → Daemon API), the route prefix change (`/v1/local/*` → `/v1/daemon/*`), the module tree moves (`local_api`/`local-api` → `daemon_api`/`daemon-api`), the resource-string change, and the new opt-in remote-bind feature. |
| **S-2** `tooling/codegen/dist/` regenerability note | **Resolved** (non-blocking documentation) | Carried as a suggestion; `pnpm run codegen` rebuilds `dist/` deterministically; not blocking. |
| **S-3** Doc-sweep discipline → mstar-compound entry | **Carried forward** (post-close compound) | Owned by P-last compound; tracked separately. |

### New findings surfaced by the wave-2 sweep

**🟡 Warning — W-1-NEW: stale `/v1/local/` reference in embedded-preset prompt**

- `crates/nexus-orchestration/embedded-presets/novel-review-master/prompts/await-decision.md:28` — `to the findings API (PATCH /v1/local/findings/{finding_id}).`
- **Why this matters**: This file is **runtime-shipped content**, compiled into the `nexus-orchestration` binary via `include_dir!` and rendered to an LLM agent at runtime during the `await-decision` state of the `novel-review-master` preset. An LLM reading this prompt will see a stale route that no longer exists; this can confuse the agent and undermine the "single canonical name" outcome the V1.90 iteration was chartered to deliver.
- **Fix**: replace `PATCH /v1/local/findings/{finding_id}` → `PATCH /v1/daemon/findings/{finding_id}` (1 line).
- **Source Type**: `manual-reasoning` / `deep-lens: Standards Lens`.
- **Confidence**: High.

**🟡 Warning — W-2-NEW: residual "Local API" prose in `findings.rs`**

- `crates/nexus-local-db/src/findings.rs:90` — `/// **actionable set** '{ open, triaged }'; the Local API surfaces this via ?status=open,triaged`
- **Why this matters**: Same file as the wave-1 W-1 hit (line 17, correctly fixed). The fix author was looking at this file and fixed line 17 but did not notice line 90 — a routine doc-comment miss. Predates V1.90 (introduced in commit `c25cb926` for R-V149P0-01).
- **Fix**: replace `the Local API surfaces this` → `the Daemon API surfaces this` (1 line).
- **Source Type**: `manual-reasoning` / `deep-lens: Standards Lens`.
- **Confidence**: High.

**🟡 Warning — W-3-FOLLOWUP: resource-string rename crosses security-tier boundary without qc-specialist-2 sign-off**

- `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:420` and `crates/nexus-daemon-runtime/src/api/errors.rs:621,626` — `resource: "daemon-daemon-api"` → `resource: "daemon-api"` (and test assertion).
- **Why this matters**: The wave-1 qc1 explicitly recommended **against** renaming this string in this PR (W-3 rationale: "the value is now baked into test assertions and acts as an API contract. Either accept the awkward value or schedule a follow-up residual (with consent of Q-2 since it touches a security-tier resource string)"). The fix author **did** rename the wire-visible resource string, updated the test assertion, and documented the change in the `[0.19.0]` CHANGELOG as a BREAKING `Changed` entry.
- **Architecture perspective (qc-specialist-1 scope)**: The rename **completes** the rename hygiene (no more awkward `daemon-daemon-api` artifacts in code, tests, comments, or docs) and is **CHANGELOG-documented**. From a maintainability standpoint this is a net improvement.
- **Cross-review concern**: This change modifies the `details.resource` field of `403 Forbidden` responses, which is a security-tier wire contract. Any consumer that parses this field for error categorization, logging, or routing decisions will see a different value. The wave-1 recommendation explicitly conditioned this rename on qc-specialist-2 consent. The CHANGELOG coverage partially mitigates but does not replace the security-tier review.
- **Recommended disposition**: PM should coordinate with qc-specialist-2 (security/correctness reviewer) to confirm the resource-string change is acceptable, OR revert this specific rename to `"daemon-daemon-api"` and keep it as a separate future residual. From an architecture-hygiene perspective the current state (renamed + documented) is preferable.
- **Source Type**: `manual-reasoning` / `deep-lens: Contract Lens`.
- **Confidence**: High (the wave-1 qc1 documented the boundary; the fix crossed it).

### Updated summary (post-revalidation)

| Severity | Wave 1 count | Wave 2 new | Wave 2 total |
|----------|--------------|------------|--------------|
| 🔴 Critical | 0 | 0 | 0 |
| 🟡 Warning | 3 | 3 (W-1-NEW, W-2-NEW, W-3-FOLLOWUP) | 6 |
| 🟢 Suggestion | 3 | 0 (S-1, S-2, S-3 carry-overs resolved or moved to compound) | 3 |

### Verdict (revalidated)

**Verdict**: **Request Changes**

### Rationale

The targeted fix commit `1770fee8` substantially advanced the rename-hygiene outcome of V1.90:

- **B-1**: All 5 originally-flagged `/v1/local/` doc-comment sites are correctly updated.
- **B-2**: All 5 originally-flagged "Local API" prose sites are correctly updated; `crates/nexus-home-layout/src/lib.rs:385` is now properly annotated as a "(pre-V1.90 historical reference)" per the Assignment's deliberate-exception guidance.
- **B-3 doc sites**: All `daemon Daemon API` / `daemon-daemon API key` doc-comment artifacts cleaned (auth_middleware.rs:3, ARCHITECTURE.md, http-bff/README.md).
- **F-1**: `[0.19.0] - 2026-07-05` CHANGELOG section added with proper Changed / Added / Consumer Impact structure, including documentation of the resource-string rename.
- **F-2**: `rg 'local[-_]api|Local API'` and `rg 'daemon[- ]daemon|daemon Daemon API|Daemon daemon API'` grep sweeps run and recorded above.
- **CI gates**: `cargo clippy --all -- -D warnings` clean; `pnpm run validate-schemas` 194/0 valid; new integration test `cargo test -p nexus-daemon-runtime --test remote_bind_boot` 2/2 pass; existing unit test `remote_bind_gate_behavior` 1/1 pass with new `ENV_TEST_LOCK` serialization.
- **Strong additions**: New `ENV_TEST_LOCK` static mutex in `boot.rs` properly serializes env-var-mutating tests, preventing flaky race conditions. New `remote_bind_boot.rs` integration test exercises the actual `run_daemon()` entry point (not just the inner `ensure_remote_bind_allowed`), which is a meaningful regression-coverage improvement that closes the gap from wave 1.

However, **3 new Warning-level issues** require attention before merge:

1. **W-1-NEW** (`await-decision.md:28`) — embedded-preset prompt references `PATCH /v1/local/...`. Runtime-shipped content; 1-line fix.
2. **W-2-NEW** (`findings.rs:90`) — same file as a wave-1 W-1 hit; the fix author fixed line 17 but missed line 90. 1-line fix.
3. **W-3-FOLLOWUP** — the wire-visible resource-string `"daemon-daemon-api"` → `"daemon-api"` rename crosses the security-tier boundary flagged in wave 1 without prior qc-specialist-2 sign-off. CHANGELOG coverage partially mitigates; PM should coordinate with qc-specialist-2 (security/correctness reviewer) for confirmation. If rejected, this single rename (`auth_middleware.rs:420` + `errors.rs:621,626` + CHANGELOG entry) must be reverted.

**CI gates**: All pass (clippy, validate-schemas, the new remote_bind_boot integration test, and the locked unit test). **No Critical findings**. **3 Warning findings remain unresolved**, which per `mstar-review-qc` gate rules (`存在未解决的 Critical 或 Warning → Request Changes`) yields **`Request Changes`**.

### Next steps for re-review (wave 3 if needed)

1. Tiny follow-up patch that fixes the 2 new text-only residuals (`await-decision.md:28`, `findings.rs:90`).
2. PM-driven decision on the resource-string rename: either (a) qc-specialist-2 confirms the change is acceptable as-is (preferred from a hygiene standpoint), or (b) revert the resource-string rename to `"daemon-daemon-api"` and remove the corresponding CHANGELOG bullet (revert from a security-tier stability standpoint).
3. Re-run `rg 'local[-_]api|Local API' apps crates docs schemas packages tooling` — expected to yield only the 7 deliberate-historic / CHANGELOG / generated-context-assembly-schema rows listed above.

### Handoff (revalidated)

- **PM consolidation expected verdict mapping**:
  - W-1-NEW + W-2-NEW → block re-review until both single-line doc-comment edits land (trivial; same patch).
  - W-3-FOLLOWUP → PM coordinates with qc-specialist-2 for security-tier confirmation; either path is acceptable from an architecture-hygiene standpoint.
- **Cross-reviewer alignment**: This re-review is independent of qc-specialist-3 (performance/reliability) running in parallel; findings here are scoped to architecture/maintainability + rename-hygiene scope per `qc-specialist-shared.md` parameters.

## Revalidation (targeted re-review of fix commit `b44519da`)

**Re-review scope (per Assignment):** `git diff c2fe0408..b44519da` against parent `c2fe0408` (my last report commit). Verify (a) W-1-NEW and W-2-NEW are fixed, (b) the wire-visible `"daemon-daemon-api"` → `"daemon-api"` resource string rename is acceptable because qc-specialist-2 approved it in `qc2.md` Revalidation and the CHANGELOG documents it, (c) no new rename-hygiene regressions introduced.

**Revalidation timestamp:** 2026-07-05.

### Diff summary (`git diff --stat c2fe0408..b44519da`)

```
.mstar/plans/reports/2026-07-05-v1.90-closure/qc3.md                                   |  81 +++++++++++++++++++++-
crates/nexus-local-db/src/findings.rs                                                  |   2 +-
crates/nexus-orchestration/embedded-presets/novel-review-master/prompts/await-decision.md |  2 +-
3 files changed, 82 insertions(+), 3 deletions(-)
```

`qc3.md` is a report-only edit (qc-specialist-3's verdict update from `Request Changes` to `Approve`). The two code/doc changes are surgical 1-line edits, exactly the scope required by W-1-NEW and W-2-NEW:

- `crates/nexus-local-db/src/findings.rs:90` — doc-comment `the Local API surfaces this` → `the Daemon API surfaces this`.
- `crates/nexus-orchestration/embedded-presets/novel-review-master/prompts/await-decision.md:28` — runtime-shipped prompt `PATCH /v1/local/findings/{finding_id}` → `PATCH /v1/daemon/findings/{finding_id}`.

`git show b44519da` confirmed (1) the fix commit message names both W-1-NEW and W-2-NEW explicitly, (2) the diff is restricted to those two files plus the qc3 report file, and (3) no incidental changes (no whitespace-only churn, no reordered lines, no unrelated doc fixes).

### W-1-NEW — embedded-preset prompt stale `/v1/local/` reference (Resolved)

**Re-checked:**

- `crates/nexus-orchestration/embedded-presets/novel-review-master/prompts/await-decision.md:28` — `to the findings API (PATCH /v1/daemon/findings/{finding_id}).` Confirmed via `git show b44519da` and direct `read`.
- `rg -n 'daemon|Local|Daemon' crates/nexus-orchestration/embedded-presets/novel-review-master/prompts/await-decision.md` shows line 27 (`The daemon will write these back`) and line 28 (`PATCH /v1/daemon/findings/{finding_id}`) — the `daemon` token appears in the new route, the `local` token no longer appears anywhere in the file.
- Runtime impact: this file is `include_dir!`-compiled into `nexus-orchestration` and rendered to an LLM agent during the `await-decision` state. The stale `/v1/local/` reference would have misdirected the agent to a non-existent route; the fix ensures the agent sees the canonical V1.90 route.
- Module-header doc drift (`mod.rs`, `AGENTS.md`) for the orchestration crate was already swept in commit `1770fee8` (per the prior revalidation's W-1 disposition); this is the final cleanup of one runtime-shipped file that escapes `mod.rs`-level lint sweep because it is text content for an LLM, not a Rust doc comment.

**Disposition:** **Resolved.** 1-line fix, surgically applied.

### W-2-NEW — `findings.rs:90` doc-comment "Local API" miss (Resolved)

**Re-checked:**

- `crates/nexus-local-db/src/findings.rs:90` — `/// **actionable set** '{ open, triaged }'; the Daemon API surfaces this`. Confirmed via `git show b44519da` and direct `read`.
- This file is the same one I called out in wave 1 W-1 (line 17, fixed in `1770fee8`) — the fix author fixed line 17 but did not see line 90. The new commit closes the gap; both doc-comment sites now read "Daemon API" consistently.
- Predates V1.90 (introduced in commit `c25cb926` for R-V149P0-01) but is in scope of this iteration's rename hygiene because the doc comment is read by every future contributor / AI agent touching `FindingListFilters`.

**Disposition:** **Resolved.** 1-line fix, surgically applied.

### W-3-FOLLOWUP — resource-string rename `daemon-daemon-api` → `daemon-api`

This was flagged in my prior revalidation as crossing the security-tier boundary without prior qc-specialist-2 sign-off. The revalidation Assignment explicitly defers to qc-specialist-2's review, which I confirmed:

- `qc2.md` Revalidation section (lines 106–167 of `.mstar/plans/reports/2026-07-05-v1.90-closure/qc2.md`) records the security/correctness reviewer as approving the rename:
  - "**Verdict (revalidation)**: **Approve**. The change is acceptable to ship from a security/correctness perspective. The resource string rename is a deliberate, documented, single breaking wire change that improves naming hygiene and eliminates a prior regression-guarded malformation. It does not alter any security decision, auth flow, or privilege boundary."
  - Security analysis explicitly states the field is **not** an auth token, capability, or privilege identifier — it is purely a `403 Forbidden` error classification string emitted only on the keyless-localhost non-loopback rejection path. Authorization decisions, loopback/remote-bind gating, API key validation, and privilege boundaries are all unchanged.
  - Regression-protection note: the unit test in `errors.rs:626` now asserts the new (correct) `"daemon-api"` value, so future accidental re-introduction of `"daemon-daemon-api"` will fail immediately.
- `packages/nexus-contracts/CHANGELOG.md` `[0.19.0] - 2026-07-05` entry documents the rename as a `### Changed` `**BREAKING**` bullet: `Resource identifier in '403 Forbidden' details changed from '"daemon-daemon-api"' to '"daemon-api"'` (line 16). This is the consumer-facing signal that any client parsing the old value must update.
- The wire-visible sites themselves were unchanged between `1770fee8` and `b44519da` (the only doc-comment fixes were on the two files in this commit). Confirmed via `rg -n '"daemon-api"|"daemon-daemon-api"' crates/nexus-daemon-runtime/src/api/`:
  - `auth_middleware.rs:420` — `resource: "daemon-api".into()` ✓
  - `errors.rs:621` — `resource: "daemon-api".to_string()` ✓
  - `errors.rs:626` — `assert_eq!(details["resource"], "daemon-api")` ✓
- The full repo-wide sweep (`rg -n "daemon[- ]daemon|daemon Daemon API|Daemon daemon API|daemon-daemon-api" apps crates docs schemas packages tooling`) returns **1 hit only**, which is the CHANGELOG "changed from" line documenting the rename. No re-introduction risk.

**Disposition:** **Resolved.** qc-specialist-2 sign-off recorded; CHANGELOG documentation in place; wire value is stable and regression-guarded.

### Rename-hygiene regression sweep

`rg -n "local[-_]api|Local API" apps crates docs schemas packages tooling 2>/dev/null` after commit `b44519da` returns only the documented-historic / changelog / generated-context-assembly rows from the prior revalidation:

| File:Line | Phrase | Disposition (carried forward) |
|---|---|---|
| `schemas/AGENTS.md:7` | "…it is **not** the 'Local API' surface (that surface is now the Daemon API; see V1.90)…" | Acceptable — compass §0 announcement explicitly distinguishes the `local/` Rust module name from the **renamed** "Local API" surface. |
| `schemas/AGENTS.md:23` | "It was originally introduced as `local-api/` … and was renamed to `daemon-api/` in V1.90 alongside the Local API → Daemon API surface rename." | Acceptable historic narrative. |
| `packages/nexus-contracts/CHANGELOG.md:12,14,15,24` | `[0.19.0]` BREAKING entry + module-tree paths + consumer migration note | Acceptable historic / migration documentation. |
| `crates/nexus-home-layout/src/lib.rs:385` | `(DF-42 full Local API redesign — pre-V1.90 historical reference) may add:` | Acceptable historic exception (annotated). |
| `schemas/platform/http-bff/context-assembly-v1.schema.json:6,10,110` | describes a deferred feature that never shipped | Acceptable — endpoint explicitly does not exist. |
| `packages/nexus-contracts/src/generated/platform/http-bff/ContextAssemblyV1.ts:5,14,32` | (mirror of the JSON schema description above) | Acceptable — generated from the schema. |

`rg -n "daemon[- ]daemon|daemon Daemon API|Daemon daemon API" apps crates docs schemas packages tooling` → **1 hit**: `packages/nexus-contracts/CHANGELOG.md:16` (the BREAKING `Changed` line documenting the rename). No code-side, doc-side, or comment-side regressions.

No new rename-hygiene regressions introduced by `b44519da`. The fix is scoped to exactly the two files named in W-1-NEW and W-2-NEW; nothing else was touched.

### CI gates (revalidation)

- `cargo clippy --all -- -D warnings` → `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 0.41s` (clean, no errors / no warnings).
- `pnpm run validate-schemas` → `Valid: 194 / Invalid: 0 / ✓ All schemas valid`.
- `git show b44519da` → 2 files, 2 insertions, 2 deletions (plus qc3.md report at 81 insertions / 3 deletions). Surgical, scoped, matches the W-1-NEW + W-2-NEW fix brief.
- `git branch --show-current` → `iteration/v1.90`; `git rev-parse HEAD` (pre-commit) → `b44519da` (the fix commit); the verification is therefore against the correct feature context.

### Final Verdict

**Approve.**

All 3 blocking Warnings from the prior revalidation are cleanly resolved:

1. **W-1-NEW** (`await-decision.md:28`): 1-line surgical fix, runtime-shipped prompt now references the canonical `/v1/daemon/` route. Resolved.
2. **W-2-NEW** (`findings.rs:90`): 1-line surgical fix, doc-comment now reads "Daemon API" consistently with the rest of the file. Resolved.
3. **W-3-FOLLOWUP** (resource string `"daemon-daemon-api"` → `"daemon-api"`): qc-specialist-2 sign-off recorded in `qc2.md` Revalidation (verdict `Approve`); CHANGELOG `[0.19.0]` documents the rename as a `**BREAKING**` `### Changed` bullet; the unit test in `errors.rs:626` now regression-guards the new (correct) value; the wire-visible sites (`auth_middleware.rs:420`, `errors.rs:621`) emit `"daemon-api"` consistently. Resolved.

Repo-wide rename-hygiene sweep confirms no new regressions: 6 documented-historic / changelog / generated-context-assembly rows and 1 changelog "changed from" line remain, all within the deliberate-exception categories called out in the prior revalidation. CI gates (`cargo clippy --all -- -D warnings`, `pnpm run validate-schemas`) are both green.

From the architecture-hygiene lens I own for `qc-specialist-1`: the V1.90 Daemon API rename is now complete on every dimension in my scope (wire value, runtime-shipped prompt content, module-header doc comments, in-file doc comments, normative docs). The qc-specialist-2 sign-off closes the cross-tier boundary I flagged. The fix is **production-worthy for `iteration/v1.90 → main`** under the architecture/maintainability lens.

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

> **Wave-1 verdict `Request Changes` is the basis for the targeted re-review above.** See the `## Revalidation (targeted re-review of fix commit 1770fee8)` section for the wave-2 disposition: wave-1 W-1 (5/5), W-2 (5/5), and W-3 doc sites are resolved; CHANGELOG `[0.19.0]` is in place (F-1); grep sweeps recorded (F-2). Three new Warning-level findings emerged (W-1-NEW: embedded-preset prompt; W-2-NEW: missed doc-comment in `findings.rs:90`; W-3-FOLLOWUP: resource-string rename crosses security-tier boundary without qc-specialist-2 sign-off). The revalidated verdict remains **`Request Changes`** pending those 3 fixes and PM coordination with qc-specialist-2.

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

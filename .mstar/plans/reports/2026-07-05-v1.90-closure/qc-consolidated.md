---
report_kind: qc-consolidated
plan_id: "2026-07-05-v1.90-closure"
iteration_id: "V1.90"
consolidated_at: "2026-07-05"
verdict: "Request Changes"
reviewers:
  - qc-specialist
  - qc-specialist-2
  - qc-specialist-3
---

# QC Consolidated Report — V1.90 Daemon API Remote-Ready Rename

## Reviewer Verdicts

| Reviewer | Focus | Verdict | Report |
|----------|-------|---------|--------|
| `@qc-specialist` | Architecture coherence & maintainability | **Request Changes** | `qc1.md` (`602ea2a2`) |
| `@qc-specialist-2` | Security & correctness | **Approve** | `qc2.md` (`dab3ebb9`) |
| `@qc-specialist-3` | Performance & reliability | **Request Changes** | `qc3.md` (`236f2af6`) |

## Scope (all reviewers identical)

- **plan_id**: `2026-07-05-v1.90-closure`
- **Review range / Diff basis**: `merge-base: fa771d33118b8044567974d38f09fc874d3b4e6a` → `tip: c0f6252818d6323480c49a3aa5a9144c1c5b4719` (equivalent to `git diff main..iteration/v1.90`)
- **Working branch**: `iteration/v1.90`
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Scope label**: Full V1.90 integration branch — schemas, generated contracts, daemon runtime, CLI client, web UI, specs/docs/AGENTS.

## Gates verified by reviewers

All CI-equivalent gates pass:

- `cargo clippy --all -- -D warnings` — clean (qc1)
- `cargo test --all` relevant subsets — all green (qc3)
- `cargo test -p nexus-daemon-runtime --lib remote_bind_gate_behavior` — passes (qc1/qc2)
- `pnpm --filter web run typecheck` — clean (qc1/qc3)
- `pnpm --filter web run test` — 404/404 pass (qc1/qc3)
- `pnpm --filter @42ch/nexus-contracts run build` — builds 0.19.0 cleanly (qc1/qc3)
- `pnpm run validate-schemas` — 194/194 valid (qc1)
- `pnpm run codegen` — deterministic, `git status` clean afterwards (qc3)

## Blocking findings (must fix before merge)

### B-1 — Rename hygiene: stale `/v1/local/` route references in doc comments
- **Source**: qc1 W-1
- **Files**: `crates/nexus-agent-host/src/lib.rs`, `crates/nexus-orchestration/src/preset/mod.rs`, `crates/nexus-orchestration/src/preset/validation.rs`, `crates/nexus-orchestration/src/stage_gates.rs`, `crates/nexus-local-db/src/findings.rs`
- **Fix**: replace `/v1/local/` with `/v1/daemon/` in these doc-comment lines.

### B-2 — Rename hygiene: remaining "Local API" prose in normative docs/comments
- **Source**: qc1 W-2
- **Files**: `apps/AGENTS.md:19`, `crates/nexus-orchestration/AGENTS.md:44`, `apps/desktop/src-tauri/src/lib.rs:129`, `crates/nexus-orchestration/src/findings_block.rs:58,76`
- **Fix**: replace "Local API" with "Daemon API" (or appropriate contextual wording). `crates/nexus-home-layout/src/lib.rs:385` references historical plan DF-42 and may be left historic with a note; PM leaves to fixer's judgment.

### B-3 — Wire-visible typo `"daemon-daemon-api"` frozen by test assertion
- **Source**: qc3 W-01 / qc1 W-3
- **Runtime sites**:
  - `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:420` — `resource: "daemon-daemon-api"`
  - `crates/nexus-daemon-runtime/src/api/errors.rs:329-332` — serialises into error response
  - `crates/nexus-daemon-runtime/src/api/errors.rs:626` — test assertion locks the value
- **Doc-comment sites with same root cause**: `auth_middleware.rs:3`, `apps/nexus42/src/config.rs:33`, `apps/nexus42/src/session_capture.rs:7`, `apps/nexus42/src/api/daemon_client.rs:55`, `apps/nexus42/src/commands/creator/run.rs` (4 sites), `apps/nexus42/src/commands/acp_worker/mod.rs:71`, `docs/ARCHITECTURE.md:46`
- **Fix**: change emitted resource string to a clean canonical value (recommended `"daemon-api"`), update the assertion, and sweep the doc-comment artifacts (`daemon Daemon API` / `Daemon daemon API` / `daemon-daemon` → `Daemon API` / `daemon-api`).

### B-4 — Remote-bind gate lacks boot-path integration coverage
- **Source**: qc3 W-02 / qc2 W-01
- **Where**: `crates/nexus-daemon-runtime/src/boot.rs:33-53` (gate) and `boot.rs:786-789` (call site before `TcpListener::bind`); unit test at `boot.rs:1028-1057`
- **Fix**: add an integration test under `crates/nexus-daemon-runtime/tests/` that exercises `run_daemon()` / transport resolution with a non-loopback host and asserts failure before listen when env vars are missing, then success when both are set. Also adopt `ENV_TEST_LOCK` pattern from `apps/nexus42/src/api/daemon_client.rs:832` for the existing unit test's env-var mutations.

## Non-blocking follow-ups

### F-1 — `@42ch/nexus-contracts` CHANGELOG stale
- **Source**: qc1 S-1 / qc3 S-02
- **Fix**: append `[0.19.0] - 2026-07-05` entry noting Local API → Daemon API rename, path prefix change, and consumer-facing break.

### F-2 — Add grep-based rename-hygiene gate to verification
- **Source**: qc3 S-01
- **Fix**: P-last verification should run `rg -nE 'local[-_ ]api|daemon[- ]daemon|daemon Daemon API|Daemon daemon API'` scoped to `apps crates docs schemas packages tooling` and exit non-zero. This is part of the fix commit verification.

### F-3 — Consider codifying doc-sweep discipline in knowledge base
- **Source**: qc1 S-3
- **Disposition**: deferred to `mstar-compound` round during iteration-close.

### F-4 — Web smoke script not exercised
- **Source**: qc3 S-04
- **Disposition**: QA will run or schedule `scripts/served-ui-smoke.sh` during verification.

### F-5 — Pre-existing pedantic clippy in `workspace/session.rs`
- **Source**: qc3 S-05
- **Disposition**: pre-existing; not V1.90 scope.

## Re-review plan

- **Targeted re-review** after fixes: `@qc-specialist` (B-1/B-2/B-3 doc + B-3 resource string), `@qc-specialist-3` (B-3 wire value + B-4 integration test + env-lock). `@qc-specialist-2` does not require re-review unless the auth middleware error envelope changes materially.
- Reports: update same `qc1.md` / `qc3.md` with `## Revalidation` section, or create `qc1-rev2.md` / `qc3-rev2.md` per `mstar-plan-artifacts` if substantial.

## PM decision

- Consolidated verdict: **Request Changes**.
- Action: dispatch `@fullstack-dev` to address B-1 through B-4 and F-1/F-2 in one targeted fix commit on `iteration/v1.90`.
- QA gate: after targeted re-review Approve, run full CI-equivalent checks and `scripts/served-ui-smoke.sh`.

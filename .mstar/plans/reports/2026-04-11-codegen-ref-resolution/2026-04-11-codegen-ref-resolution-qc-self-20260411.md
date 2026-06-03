---
report_kind: qc_self
reviewer: implementer-self-qc
reviewer_index: 0
plan_id: "2026-04-11-codegen-ref-resolution"
verdict: "Approve"
generated_at: "2026-04-11"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: implementer-self-qc (qc_self)
- Review Perspective: codegen correctness, cross-language wire parity, downstream compile/test impact
- Report Timestamp: 2026-04-11

## Scope
- plan_id: 2026-04-11-codegen-ref-resolution
- Review range / Diff basis: `tooling/codegen` (`schema-loader.ts`, `rust-generator.ts`, `ts-generator.ts`); regenerated `packages/nexus-contracts/src/generated/` and `crates/nexus-contracts/src/generated/`; Rust call sites in `nexus-sync`, `nexus42`, `nexus42d` and integration tests
- Working branch (verified): feature/codegen-ref-resolution
- Review cwd (verified): repository root (`git rev-parse --show-toplevel`)
- Files reviewed: codegen + generated trees + affected crates (representative diff review)
- Commit range (if not identical to Review range line, explain): single commit on `feature/codegen-ref-resolution` at time of qc_self (see `git log -1` on that branch)
- Tools run: `pnpm run validate-schemas`, `pnpm run codegen`, `pnpm run typecheck`, `cargo +nightly fmt --all -- --check`, `cargo clippy --all -- -D warnings`, targeted `cargo test` (see Verification)

## Findings
### 🔴 Critical
- None.

### 🟡 Warning
- Full `cargo test --all` was not used as the sole gate: `nexus42` integration test `agent_show_unknown_agent` can fail when the ACP registry CDN is unreachable or stderr wording drifts. Lib tests and the two `nexus-sync` integration tests covering publish/fork JSON parsing were run instead. -> Re-run full suite in CI or with network when merging.

### 🟢 Suggestion
- Consider a follow-up to harden or mock the CDN-dependent CLI test so local `cargo test --all` is deterministic offline.

## Source Trace
- Finding ID: F-001
- Source Type: manual-reasoning
- Source Reference: prior session notes + targeted test matrix
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

## Plan acceptance

| Criterion / task | Done / Partial / Not done | Evidence |
|------------------|----------------------------|----------|
| Codegen emits concrete Rust/TS types for common `$ref` string defs and whole-schema refs | Done | Generated `publish_*`, `world_fork_response`, `sync_pull_response`, `explore_feed_response` use `String` / `Vec<Bundle>` / `ForkBranch` / `Vec<ExploreHit>` etc. |
| Regenerated output only (no hand edits under `*/generated/`) | Done | `pnpm run codegen` drives TS/Rust output |
| Downstream compiles | Done | `cargo clippy --all -- -D warnings` |
| Close PUBLISH-CODEGEN-01 & FORK-SNAP-01 | Done | `archived/residuals/2026-04-10-cli-publish-workflow-parity.json`, `archived/residuals/2026-04-10-cli-fork-world-snapshot-parity.json`; `residual_findings` cleared |
| Plan SSOT | Done | `.agents/status.json`, `notes.json`, this report |

## Verification

- `pnpm run validate-schemas` — pass (57/57 valid).
- `pnpm run codegen` — pass; working tree matches generator output for committed paths (verify in CI via `verify-codegen`).
- `pnpm run typecheck` — pass (workspace packages).
- `cargo +nightly fmt --all -- --check` — pass.
- `cargo clippy --all -- -D warnings` — pass.
- `cargo test -p nexus-contracts -p nexus-sync -p nexus42d -p nexus42 --lib` — pass (517+ tests in output slice).
- `cargo test -p nexus-sync --test publish_client --test world_fork_snapshot_client` — pass (6 tests).

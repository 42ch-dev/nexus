# CLI Bidirectional Sync Parity (platform sync wave)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `nexus42` / `nexus42d` sync commands and `nexus-sync` client behavior in line with the **bidirectional sync** contract exposed by the platform (pull, delta application, command lifecycle), so local outbox and remote state stay consistent with v1-spec + JSON Schema wire types.

**Architecture:** Extend the existing sync stack (`crates/nexus-sync`, CLI sync handlers, daemon routes if needed) behind the codegen types; gate risky paths behind integration tests and feature flags if required. Prefer schema-first changes: update `schemas/` and run `pnpm run codegen` before hand-editing generated output.

**Tech Stack:** Rust, `nexus-sync`, `nexus-contracts` (generated), `reqwest` or existing HTTP client, SQLite outbox, platform auth tokens from daemon.

---

## Authoritative design input

- [.agents/knowledge/v1.1-overview-v2.md](knowledge/v1.1-overview-v2.md) — program overview (capability symmetry **B**).
- [.agents/knowledge/architecture-alignment-review-v1.md](knowledge/architecture-alignment-review-v1.md) — alignment narrative.
- In-repo: `schemas/*sync*`, `crates/nexus-sync/`, `crates/nexus42/src/commands/sync*.rs` (paths may vary — locate with `rg 'sync' crates/nexus42`).

**Cross-repo:** Platform HTTP routes and error semantics are defined in the private **v1-spec** / platform API docs. Do not paste out-of-repo paths into committed artifacts; copy minimal acceptance bullets into this plan when locked.

---

## Overview

| Field | Value |
| --- | --- |
| **Priority** | High |
| **Program ref** | Bidirectional sync capability (platform plan id `10-sync-bidirectional`, conceptual) |
| **Dependencies** | None among the four parity plans (this plan is **first** in the chain) |
| **Blocks** | `2026-04-10-cli-fork-world-snapshot-parity`, `2026-04-10-cli-publish-workflow-parity` |
| **Working branch** | `feature/cli-sync-bidirectional-parity` (from `main`) |

## Non-goals (V1)

- Full interactive conflict-merge UX (defer to follow-up plan or product call).
- Changing platform server behavior (this repo is client + contracts).

---

## Acceptance criteria (ship gate)

- [x] `sync pull` applies server bundles to the local outbox via `Outbox::stage_if_absent` (wire: `SyncPullRequest` / `SyncPullResponse`); daemon returns structured JSON; CLI prints summary or daemon error text.
- [x] `sync push` path unchanged for canonical bundle rules ([`knowledge/canonical-hash-v1.md`](knowledge/canonical-hash-v1.md)) (still `BundleBuilder` + precheck + `Outbox::stage`).
- [x] `cargo test -p nexus-sync` includes wiremock pull tests and `sync_push_pull_loop` (mock push + pull); see `crates/nexus-sync/tests/`.
- [x] `cargo clippy --all -- -D warnings` and `pnpm run codegen` + generated dirs committed for new schemas.

---

## Task group A — Inventory & contract gap

- [x] **A1:** Inventory: [reports/2026-04-10-cli-sync-bidirectional-parity/sync-inventory-a1.md](reports/2026-04-10-cli-sync-bidirectional-parity/sync-inventory-a1.md)
- [x] **A2:** Added `sync-pull-request` / `sync-pull-response` schemas; `pnpm run codegen`.
- [x] **A3:** `crates/nexus-sync/tests/sync_pull_client.rs` (empty JSON + 404 → `PlatformError`).

## Task group B — Implement pull / delta apply

- [x] **B1:** `SyncClient::pull_bundles` → `POST /v1/sync/pull`; daemon uses same env credentials as eager push (`NEXUS_SYNC_PLATFORM_*`).
- [x] **B2:** HTTP ≥400 → `SyncError::PlatformError` with status + body; daemon maps to `NexusApiError::Internal` with `SyncError::error_code()`.
- [x] **B3:** `apply_pull_response_to_outbox` + `Outbox::stage_if_absent`; unit + integration tests.

## Task group C — CLI & daemon

- [x] **C1:** `POST /v1/local/sync/pull`; CLI `sync pull` after `health_check`; requires running daemon + platform env on daemon process.
- [x] **C2:** Help via clap on `pull` (`--world-id`, `--after-sequence`); no `docs/` change.

## Task group D — Verification

- [x] **D1:** `cargo test -p nexus-sync`, `cargo test -p nexus42 -p nexus42d` (full `cargo test --all` may hit network-dependent `cli_agent` test if CDN unreachable).
- [x] **D2:** `cargo +nightly fmt --all`, `cargo clippy --all -- -D warnings`, `pnpm run typecheck`.

---

## Verification commands

```bash
cargo test -p nexus-sync -p nexus42 -- --nocapture
cargo clippy --all -- -D warnings
pnpm run codegen && git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

---

*Plan id: `2026-04-10-cli-sync-bidirectional-parity` · Created: 2026-04-10*

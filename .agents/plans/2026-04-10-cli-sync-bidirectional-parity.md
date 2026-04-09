# CLI Bidirectional Sync Parity (platform sync wave)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `nexus42` / `nexus42d` sync commands and `nexus-sync` client behavior in line with the **bidirectional sync** contract exposed by the platform (pull, delta application, command lifecycle), so local outbox and remote state stay consistent with v1-spec + JSON Schema wire types.

**Architecture:** Extend the existing sync stack (`crates/nexus-sync`, CLI sync handlers, daemon routes if needed) behind the codegen types; gate risky paths behind integration tests and feature flags if required. Prefer schema-first changes: update `schemas/` and run `pnpm run codegen` before hand-editing generated output.

**Tech Stack:** Rust, `nexus-sync`, `nexus-contracts` (generated), `reqwest` or existing HTTP client, SQLite outbox, platform auth tokens from daemon.

---

## Authoritative design input

- [.agents/plans/knowledge/v1.1-overview-v2.md](knowledge/v1.1-overview-v2.md) — program overview (capability symmetry **B**).
- [.agents/plans/knowledge/architecture-alignment-review-v1.md](knowledge/architecture-alignment-review-v1.md) — alignment narrative.
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

- [ ] `sync pull` (or equivalent approved command name) applies server deltas to local outbox/state per wire contract; errors map to structured CLI messages.
- [ ] `sync push` path remains consistent with canonical bundle rules ([`knowledge/canonical-hash-v1.md`](knowledge/canonical-hash-v1.md)).
- [ ] `cargo test -p nexus-sync` and CLI integration tests cover at least one golden pull + push loop (mock server acceptable).
- [ ] `cargo clippy --all -- -D warnings` and `pnpm run codegen` + no diff on generated dirs after any schema change.

---

## Task group A — Inventory & contract gap

- [ ] **A1:** Document current CLI sync subcommands and `SyncClient` public API in a short table (command → crate entrypoint → HTTP route name from capability map if known).
- [ ] **A2:** List JSON Schema / generated types required for pull; if gaps exist, add schema drafts and run `pnpm run codegen`.
- [ ] **A3:** Add failing integration test skeleton: `tests/` or `crates/nexus-sync/tests/` — “pull applies empty response” or mock 404 to lock wiring.

## Task group B — Implement pull / delta apply

- [ ] **B1:** Implement client method(s) for pull endpoint(s); reuse auth/session from daemon-owned store.
- [ ] **B2:** Map HTTP errors to `NexusErrorCode` (or existing error enum) at the boundary.
- [ ] **B3:** Persist applied commands / bundles per `nexus-sync` state machine; extend unit tests.

## Task group C — CLI & daemon

- [ ] **C1:** Wire CLI command(s); ensure `daemon status` / health interaction documented if daemon required.
- [ ] **C2:** Update user-facing help text and `docs/` only if product asks (otherwise keep scope to `.agents` + code).

## Task group D — Verification

- [ ] **D1:** `cargo test --all` (or scoped crates touched).
- [ ] **D2:** `cargo +nightly fmt --all` and `cargo clippy --all -- -D warnings`.

---

## Verification commands

```bash
cargo test -p nexus-sync -p nexus42 -- --nocapture
cargo clippy --all -- -D warnings
pnpm run codegen && git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

---

*Plan id: `2026-04-10-cli-sync-bidirectional-parity` · Created: 2026-04-10*

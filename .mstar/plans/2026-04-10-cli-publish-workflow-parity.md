# CLI Publish Workflow Parity

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement CLI commands for the **explicit publish** workflow and history introspection aligned with the platform Publish API, including sync prerequisites (published state must match server contract).

**Architecture:** Compose publish operations on top of sync + domain manuscript/world state; use codegen types for requests/responses. Prefer idempotent CLI operations where the API is idempotent.

**Tech Stack:** Rust, `nexus-sync`, `nexus42`, daemon auth, generated contracts.

---

## Authoritative design input

- [.mstar/iterations/v1.1-overview-v2.md](iterations/v1.1-overview-v2.md)
- **Program ref:** Publish API (platform plan id `14-publish-api`, conceptual).

---

## Overview

| Field | Value |
| --- | --- |
| **Priority** | Medium |
| **Dependencies** | `2026-04-10-cli-sync-bidirectional-parity` (publish builds on correct bundle/sync semantics) |
| **Working branch** | `feature/cli-publish-workflow-parity` |

## Non-goals (V1)

- Billing / entitlement enforcement (V1.2+ per program roadmap).
- Modifying publish policy on the server.

---

## Acceptance criteria

- [x] CLI can trigger publish (or staged publish if API requires) and query publish **status/history** per wire types.
- [x] Clear errors when local draft state cannot be published (validation messages from API propagated).
- [x] Tests: mock publish round-trip + one failure case.

---

## Task group A — Contract & flow mapping

- [x] **A1:** Map each publish-related HTTP operation to CLI flags.
- [x] **A2:** Schema gaps → `schemas/` + codegen pipeline.

## Task group B — Implementation

- [x] **B1:** Client methods + daemon routing if needed.
- [x] **B2:** CLI commands under manuscript/world namespace per product decision.

## Task group C — Verification

- [x] **C1:** `cargo test` / clippy.

---

## Verification commands

```bash
cargo test -p nexus-sync -p nexus42 -p nexus42d
cargo clippy --all -- -D warnings
```

---

*Plan id: `2026-04-10-cli-publish-workflow-parity` · Created: 2026-04-10*

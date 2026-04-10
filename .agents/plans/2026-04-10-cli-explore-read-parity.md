# CLI Explore (Read) Parity

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide **read-only** Explore-style CLI commands (browse/search/discover) consistent with platform Explore API visibility rules, for users who stay in the terminal.

**Architecture:** Thin CLI over authenticated GET endpoints; pagination and filters match wire contracts. No local replica of the full graph — fetch on demand unless an existing cache layer is already defined.

**Tech Stack:** Rust, HTTP client, generated types, optional JSON output mode for scripting.

---

## Authoritative design input

- [.agents/plans/knowledge/v1.1-overview-v2.md](knowledge/v1.1-overview-v2.md)
- **Program ref:** Explore API (platform plan id `12-explore-api`, conceptual). Visibility: align with v1-spec Explore / world visibility chapters when available to the implementer.

---

## Overview

| Field | Value |
| --- | --- |
| **Priority** | Medium |
| **Dependencies** | `2026-04-10-v1-spec-wire-schema-sprint` (generated Explore wire DTOs). **Done prerequisite:** `2026-04-10-cli-fork-world-snapshot-parity`. |
| **Working branch** | `feature/cli-explore-read-parity` |

## Non-goals (V1)

- Rendering rich UI or TUI beyond simple tables/text.
- Write operations (follow/social) — belong to later platform waves / different plans.

---

## Acceptance criteria

- [ ] At least **browse** + **search** flows exposed with stable JSON/text output modes.
- [ ] Auth required paths fail clearly when logged out (reuse auth error patterns from sync).
- [ ] Automated tests use mock responses matching generated DTO shapes.

---

## Task group A — Command surface

- [ ] **A1:** Choose subcommand group (`nexus42 explore …` or per `cli-spec-v1` once synced).
- [ ] **A2:** Define query parameters and map to request types.

## Task group B — Client implementation

- [ ] **B1:** Implement client functions alongside existing platform HTTP module.
- [ ] **B2:** Handle pagination cursors if API uses them.

## Task group C — Tests & docs

- [ ] **C1:** Unit tests with `httpmock` or existing test harness pattern in repo.
- [ ] **C2:** If end-user docs are updated, keep under `docs/` per AGENTS boundary; otherwise `.agents` plan notes suffice.

## Task group D — Verification

- [ ] **D1:** `cargo test` / clippy for touched crates.

---

## Verification commands

```bash
cargo test -p nexus42
cargo clippy --all -- -D warnings
```

---

*Plan id: `2026-04-10-cli-explore-read-parity` · Created: 2026-04-10*

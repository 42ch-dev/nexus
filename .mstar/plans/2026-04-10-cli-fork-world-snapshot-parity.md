# CLI Fork & World Snapshot Parity

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose CLI and local-runtime flows for **fork** and **world snapshot** operations that match the platform API semantics (Fork API & snapshot), building on stable bidirectional sync.

**Architecture:** CLI → daemon (or direct client where already pattern) → HTTP resources; use generated DTOs only. Domain invariants for `ForkBranch` / pairing live in `nexus-domain` — extend only with schema-backed types.

**Tech Stack:** Rust, `nexus-domain`, `nexus-sync`, `nexus42`, `nexus42d`, JSON Schema codegen.

---

## Authoritative design input

- [.mstar/iterations/v1.1-overview-v2.md](iterations/v1.1-overview-v2.md)
- Domain: `crates/nexus-domain/src/fork_branch.rs`, `pairing.rs`
- Residual **DM-R3** (edge tests) may partially overlap — coordinate with `v1-tech-debt-cleanup` to avoid duplicate work.

**Program ref:** Fork + snapshot capability (platform plan id `11-fork-api`, conceptual).

---

## Overview

| Field | Value |
| --- | --- |
| **Priority** | High |
| **Dependencies** | `2026-04-10-cli-sync-bidirectional-parity` (sync must be trustworthy before fork workflows) |
| **Blocks** | `2026-04-10-cli-explore-read-parity` (recommended order) |
| **Working branch** | `feature/cli-fork-world-snapshot-parity` |

## Non-goals (V1)

- World merge / multi-parent fork graphs beyond v1-spec.
- UI or web flows.

---

## Acceptance criteria

- [ ] CLI exposes fork/snapshot subcommands aligned with the in-repo CLI command tree (`crates/nexus42/src` / clap definitions), including `--dry-run` or confirmation where destructive.
- [ ] Server errors and validation failures surface with actionable messages.
- [ ] Tests cover happy path + at least one invalid fork / id mismatch case.
- [ ] No new handwritten wire DTOs — codegen only.

---

## Task group A — API & schema alignment

- [ ] **A1:** List platform fork/snapshot operations needed on the client; map to schema types or file issues for schema additions.
- [ ] **A2:** If schemas change: edit `schemas/`, run `pnpm run codegen`, update `enum_conversions.rs` if enums added.

## Task group B — Client & daemon

- [ ] **B1:** Implement HTTP client calls in the existing sync/platform client module.
- [ ] **B2:** Add daemon handlers if CLI policy is daemon-mediated for auth.

## Task group C — CLI

- [ ] **C1:** Add clap subcommands under `nexus42` (e.g. `world fork`, `world snapshot` — align with `cli-spec` naming when available).
- [ ] **C2:** Document environment requirements (auth, workspace initialized).

## Task group D — Verification

- [ ] **D1:** `cargo test` for touched crates; add integration tests with mock HTTP where used elsewhere.
- [ ] **D2:** Clippy + fmt.

---

## Verification commands

```bash
cargo test -p nexus-domain -p nexus-sync -p nexus42 -p nexus42d
cargo clippy --all -- -D warnings
```

---

*Plan id: `2026-04-10-cli-fork-world-snapshot-parity` · Created: 2026-04-10*

# CLI + Daemon Foundation Implementation Plan (V1.0-phase1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `nexus42` CLI executable and `nexus42d` daemon skeleton with basic auth, workspace management, and command routing.

**Architecture:** 

- CLI (`nexus42`): Command-line interface using `clap`, handles user commands, communicates with daemon via Local API
- Daemon (`nexus42d`): Background service managing workspace, auth, and sync operations

**Tech Stack:** Rust 1.75+, clap (CLI), tokio (async), serde, dirs (home directory), tracing (logging)

**Branch Strategy:** Feature branch `feature/v1.0-cli-daemon` from `main`

**Architecture Review:** See `.agents/plans/reports/2025-04-05-phase1-review/2025-04-05-phase1-architect-review.md` for detailed review and constraints.

---

## Working Branch

**Branch:** `feature/v1.0-cli-daemon`
**Base:** `main` (after Phase 0 + domain-models complete)

---

## Core Tasks Overview

### Task 1: Initialize CLI Crate

- Create `crates/nexus42/Cargo.toml`
- Create `crates/nexus42/src/main.rs` with clap command definitions
- Implement basic commands: `init`, `auth`, `sync`, `daemon`
- Add workspace detection and `.nexus42/` directory management

### Task 2: Initialize Daemon Crate

- Create `crates/nexus42d/Cargo.toml`
- Create `crates/nexus42d/src/main.rs` with tokio runtime
- Implement Local API (**HTTP-only** on loopback for V1.0; JSON wire shapes, not gRPC) for CLI communication
- Add workspace state management
- **Workspace state storage**: SQLite database at `$HOME/.nexus42/state.db`

### Task 3: Implement Auth Module (Dual-Subject Design)

**Architecture Decision**: Support both **User tokens** (human login) and **Creator API keys** (machine auth).

#### 3.1 User Authentication

- Device flow authentication (OAuth)
- User token storage in `$HOME/.nexus42/auth.json`
- Session management

#### 3.2 Creator Authentication

- Creator API key management (keys stored in platform secure storage)
- CLI obtains short-lived tokens via `POST /v1/creators/{id}/credentials`
- CLI caches short-lived tokens locally
- Commands: `creator credentials rotate`

**Files created**:

- `src/auth/user_auth.rs` — Device flow for human users
- `src/auth/creator_auth.rs` — API key management for Creator entities
- `src/auth/mod.rs` — Dual-subject auth dispatcher

### Task 4: Implement Workspace Management

- Create workspace structure: `Stories/`, `References/`, `.nexus42/`
- Implement workspace init command
- Add config file management (`.nexus42/config.json`)
- **SQLite schema**: workspace metadata, local state, outbox queue

### Task 5: Implement Command Routing

- CLI command → daemon Local API call
- Error handling and user feedback
- Logging with `tracing`

### Task 6: Creator Command Module (NEW — Resolves CLI-R1)

**Goal**: Implement Creator as V1.0 first-class citizen per roadmap §3.1.1, §3.1.2.

**Subcommands**:

- `nexus42 creator register` — Register a new Creator entity
- `nexus42 creator status` — Show current Creator status
- `nexus42 creator use <creator-ref>` — Switch active Creator
- `nexus42 creator list` — List all registered Creators
- `nexus42 creator pair` — Initiate pairing flow
- `nexus42 creator unpair` — Remove pairing
- `nexus42 creator credentials rotate` — Rotate Creator API key

**Files created**:

- `crates/nexus42/src/commands/creator.rs`

**Dependencies**: Task 3 (Auth module with Creator auth support)

### Task 7: Manuscript Command Module (NEW — Resolves CLI-R2)

**Goal**: Implement `manuscript_phase` and promote workflow per roadmap §3.1.1.

**Subcommands**:

- `nexus42 manuscript status` — Show current manuscript phase
- `nexus42 manuscript phase <phase>` — Set phase (brainstorm/draft/review/finalize/published)
- `nexus42 manuscript output` — Show output manuscript status
- `nexus42 manuscript promote` — Promote from provisional to canon
- `nexus42 manuscript verify` — Verify manuscript consistency

**Files created**:

- `crates/nexus42/src/commands/manuscript.rs`

**Dependencies**: Task 4 (Workspace management), sync-contract plan (bundle metadata fields)

### Task 8: Research Command Module (NEW — Resolves CLI-R3)

**Goal**: Implement V1.0 minimal research workflow per roadmap §3.1.1.

**Subcommands**:

- `nexus42 research scan` — Scan `References/<creator_ref>/` for reference sources
- `nexus42 research list` — List discovered reference sources
- `nexus42 research extract` — Extract structured data from references

**Files created**:

- `crates/nexus42/src/commands/research.rs`

**Scope**: V1.0 local-only; no platform sync for research data (only extracted `MemoryItem` goes into sync).

### Task 9: Integration Tests

- Integration test skeleton for CLI ↔ daemon communication
- Mock Local API server for testing
- End-to-end auth flow test (user device flow + Creator API key)

---

## Files to Create

**CLI (`crates/nexus42/`):**

- `Cargo.toml`
- `src/main.rs` (entry point)
- `src/commands/` (command modules)
  - `init.rs`
  - `auth.rs`
  - `sync.rs`
  - `daemon.rs`
  - `creator.rs` (NEW)
  - `manuscript.rs` (NEW)
  - `research.rs` (NEW)
  - `context.rs` (for future `nexus42 context assemble` command)
- `src/auth/` (dual-subject authentication)
  - `mod.rs`
  - `user_auth.rs` (NEW)
  - `creator_auth.rs` (NEW)
- `src/config.rs`
- `src/errors.rs`

**Daemon (`crates/nexus42d/`):**

- `Cargo.toml`
- `src/main.rs` (daemon entry point)
- `src/api/` (Local API handlers — HTTP-only)
  - `mod.rs`
  - `sync.rs`
  - `workspace.rs`
  - `context.rs` (proxy for `POST /v1/local/context/assemble`)
- `src/workspace/` (workspace management)
  - `mod.rs`
  - `manager.rs`
- `src/auth/` (authentication)
  - `mod.rs`
  - `device_flow.rs`
  - `session.rs`
  - `creator_session.rs` (NEW)

---

## Verification

- CLI binary compiles: `cargo build -p nexus42`
- Daemon binary compiles: `cargo build -p nexus42d`
- Basic commands work: `./target/debug/nexus42 --help`
- Workspace init works: `./target/debug/nexus42 init workspace`
- Creator commands work: `./target/debug/nexus42 creator --help`
- Manuscript commands work: `./target/debug/nexus42 manuscript --help`
- Research commands work: `./target/debug/nexus42 research --help`
- Auth flow implemented (device flow skeleton + token login/logout)
- Integration tests pass: `cargo test -p nexus42 --test integration` (16/16)
- Daemon integration tests pass: `cargo test -p nexus42d --test integration` (7/7)
- Full workspace tests pass: `cargo test --workspace` (156/156)

---

## Architecture Constraints (From Review)


| Constraint                             | Source                                     | Compliance                                |
| -------------------------------------- | ------------------------------------------ | ----------------------------------------- |
| Rust-first for CLI/daemon              | AGENTS.md                                  | ✅                                         |
| JSON Schema as wire truth source       | `codegen-strategy-v1.md`                   | ✅ (consume from `crates/nexus-contracts`) |
| CLI is ACP client, not agent/server    | AGENTS.md                                  | ✅                                         |
| CLI uses SQLite for local state        | `restructured-context-assembly.md` §2.3 | ✅ (Task 4)                                |
| No Neo4j/Postgres/pgvector on CLI side | `restructured-context-assembly.md` §2.3 | ✅                                         |
| V1.0 Creator as first-class citizen    | roadmap §3.1.1, §3.1.2                     | ✅ (Task 6)                                |
| `manuscript_phase` V1.0 deliverable    | roadmap §3.1.1                             | ✅ (Task 7)                                |
| Dual-subject auth (User + Creator)     | roadmap §2.2, review CLI-R4                | ✅ (Task 3)                                |


---

**Plan saved to:** `.agents/plans/2025-04-05-cli-daemon-foundation.md`  
**Last updated:** 2026-04-06 (Architecture Review: Request Changes → Expanded per §6.1)
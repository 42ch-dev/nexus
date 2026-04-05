# CLI + Daemon Foundation Implementation Plan (Phase 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `nexus42` CLI executable and `nexus42d` daemon skeleton with basic auth, workspace management, and command routing.

**Architecture:** 
- CLI (`nexus42`): Command-line interface using `clap`, handles user commands, communicates with daemon via Local API
- Daemon (`nexus42d`): Background service managing workspace, auth, and sync operations

**Tech Stack:** Rust 1.75+, clap (CLI), tokio (async), serde, dirs (home directory), tracing (logging)

**Branch Strategy:** Feature branch `feature/v1.0-cli-daemon` from `main`

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
- Implement Local API (HTTP/gRPC) for CLI communication
- Add workspace state management

### Task 3: Implement Auth Module
- Device flow authentication (OAuth)
- Token storage in `$HOME/.nexus42/auth.json`
- Session management

### Task 4: Implement Workspace Management
- Create workspace structure: `Stories/`, `References/`, `.nexus42/`
- Implement workspace init command
- Add config file management (`.nexus42/config.toml`)

### Task 5: Implement Command Routing
- CLI command → daemon Local API call
- Error handling and user feedback
- Logging with `tracing`

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
- `src/config.rs`
- `src/errors.rs`

**Daemon (`crates/nexus42d/`):**
- `Cargo.toml`
- `src/main.rs` (daemon entry point)
- `src/api/` (Local API handlers)
  - `mod.rs`
  - `sync.rs`
  - `workspace.rs`
- `src/workspace/` (workspace management)
  - `mod.rs`
  - `manager.rs`
- `src/auth/` (authentication)
  - `mod.rs`
  - `device_flow.rs`
  - `session.rs`

---

## Verification

- [ ] CLI binary compiles: `cargo build -p nexus42`
- [ ] Daemon binary compiles: `cargo build -p nexus42d`
- [ ] Basic commands work: `./target/debug/nexus42 --help`
- [ ] Workspace init works: `./target/debug/nexus42 init`
- [ ] Auth flow documented (actual OAuth requires external setup)

---

**Plan saved to:** `.agents/plans/2025-04-05-cli-daemon-foundation.md`
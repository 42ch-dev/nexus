# ACP Client Integration Plan (V1.0-phase1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate ACP (Agent Client Protocol) Rust SDK for CLI agent communication, Registry integration, and Local API minimum contract.

**Architecture:** 
- **ACP Client**: CLI acts as ACP client (not agent/server)
- **Registry**: Pull agent manifests from `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`
- **Local API**: Minimum contract for agent ↔ CLI communication
- **Skills**: Export capabilities for multi-agent host integration

**Tech Stack:** Rust 1.75+, ACP Rust SDK (official), reqwest, serde, tokio

**Branch Strategy:** Feature branch `feature/v1.0-acp-client` from `main`

---

## Working Branch

**Branch:** `feature/v1.0-acp-client`
**Base:** `main` (after cli-daemon complete)

---

## Core Tasks Overview

### Task 1: Add ACP SDK Dependency
- Add `acp-sdk` to `crates/nexus42/Cargo.toml`
- Configure ACP client with registry URL
- Implement registry manifest fetcher

### Task 2: Implement Registry Integration
- Fetch registry.json from CDN
- Parse agent manifests
- Cache manifests locally in `$HOME/.nexus42/registry/`
- Implement fallback to cached manifest on network failure

### Task 3: Implement Local API Contract
- Define minimum Local API endpoints (per frozen Local API contract)
- Implement Local API server in daemon
- Add agent-to-daemon communication layer

### Task 4: Implement Skills Export
- Define capability set (per `acp-capability-set-v1.md`)
- Export skills manifest for multi-agent hosts
- Add version alignment with platform

### Task 5: Add Agent CLI Commands
- `nexus42 agent list` - List available agents
- `nexus42 agent install <agent-ref>` - Install agent
- `nexus42 agent run <agent-ref>` - Run agent

---

## Files to Create

**ACP Module (`crates/nexus42/src/acp/`):**
- `mod.rs`
- `registry.rs` (Registry integration)
- `client.rs` (ACP client wrapper)
- `local_api.rs` (Local API contract)
- `skills.rs` (Skills export)

**Schemas:**
- `schemas/acp-runtime/local-api-v1.schema.json` (Local API contract)

---

## Verification

- [ ] ACP SDK dependency resolves: `cargo build -p nexus42`
- [ ] Registry fetches from CDN: `nexus42 agent list`
- [ ] Manifest caching works
- [ ] Local API endpoints defined
- [ ] Skills manifest generated

---

**Plan saved to:** `.mstar/plans/2025-04-05-acp-client.md`
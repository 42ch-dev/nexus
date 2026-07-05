---
module: contracts-surface
problem_type: convention
category: conventions
severity: medium
date: 2026-07-05
plan_id: 2026-07-05-v1.90-closure
tags: [rename, hygiene, grep, local-api, daemon-api, codegen, docs]
---

# Surface-Rename Hygiene Checklist

A mechanical checklist for renaming a cross-language HTTP/contract surface (e.g., `local-api` → `daemon-api`) in the Nexus monorepo.

## When this applies

- Renaming a JSON Schema subtree under `schemas/`
- Renaming a generated Rust module under `crates/nexus-contracts/src/generated/`
- Renaming a generated TypeScript module under `packages/nexus-contracts/src/generated/`
- Changing the public HTTP path prefix served by the daemon runtime
- Updating consumer-facing docs/specs/AGENTS that refer to the surface by name

## Why this matters

A surface rename touches schemas, codegen, generated code in two languages, the daemon router, multiple clients (CLI, web, desktop), and normative docs. Automated moves and generated-code regeneration handle the bulk, but **doc comments, prompt templates, embedded presets, and cross-crate prose are easy to miss**. Leftover references silently undermine the canonical name and break grep-based navigation.

## Checklist

### 1. Machine-renamable tree
- [ ] Rename schema folder and update `$id`/`$ref` paths.
- [ ] Update codegen config (`tooling/codegen/src/schema-loader.ts`, `rust-generator.ts`, `ts-generator.ts`).
- [ ] Regenerate Rust and TypeScript outputs; delete the old generated directories.
- [ ] Update package version if the contract surface is breaking.
- [ ] Run `pnpm run codegen` and verify `git status` is clean.
- [ ] Run `cargo test -p nexus-contracts --test schema_drift_detection`.

### 2. Runtime route and clients
- [ ] Update daemon router path prefix (`crates/nexus-daemon-runtime/src/api/mod.rs`).
- [ ] Update CLI client base URL and path builders (`apps/nexus42/src/api/daemon_client.rs`).
- [ ] Update web client base URL constant (`apps/web/src/lib/nexus/browser-client.ts`).
- [ ] Update Vite dev proxy (`apps/web/vite.config.ts`).
- [ ] Update any Tauri/desktop-sidecar path references.

### 3. Prose and doc-comment sweep
Run the following regexes over `apps crates docs schemas packages tooling` (excluding `node_modules`, `target/`, `.worktrees/`, `dist/`):

```sh
rg -nE 'local[-_]api|Local API'  # old surface name
rg -nE 'daemon[- ]daemon|daemon Daemon API|Daemon daemon API'  # artifacts of naive s/local/daemon/
rg -nE '/v1/local/'             # old route prefix
```

Review every hit. Allowed exceptions:
- CHANGELOG historical entries
- AGENTS.md explanatory notes that cite the rename itself
- Pre-rename historical comments that reference retired plan IDs

### 4. Embedded content
- [ ] Check `embedded-presets/**` prompt templates for old route strings.
- [ ] Check `.md` knowledge/spec docs that quote request/response examples.
- [ ] Check smoke scripts (`scripts/*.sh`) for hard-coded paths.

### 5. Verification gates
- [ ] `cargo clippy --all -- -D warnings`
- [ ] `cargo test --all`
- [ ] `pnpm --filter web run typecheck && pnpm --filter web run test`
- [ ] `pnpm --filter @42ch/nexus-contracts run build`
- [ ] `pnpm run validate-schemas`

## V1.90 lesson

The first implementation pass left `/v1/local/` and "Local API" references in 11+ files across `crates/nexus-agent-host`, `crates/nexus-orchestration`, `crates/nexus-local-db`, `apps/AGENTS.md`, `apps/desktop/src-tauri/src/lib.rs`, and an embedded preset prompt. A dedicated grep sweep in P-last caught them before merge. The fix was text-only but required three QC rounds; running the sweep as part of the initial rename would have saved the extra review cycles.

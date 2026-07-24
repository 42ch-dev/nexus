---
module: contracts-surface
problem_type: convention
category: conventions
severity: medium
date: 2026-07-05
last_updated: 2026-07-06
plan_id: 2026-07-05-v1.90-closure
tags: [rename, hygiene, grep, local-api, daemon-api, codegen, docs, anchor, stutter]
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
- [ ] Update codegen config (`tooling/codegen/src/schema-prep.ts`, `ts-gen.ts`, `rust-gen/`).
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

**These greps are a pre-commit self-check for the sweep executor, not an "available" reference** — run them and resolve every hit before committing the sweep. V1.93 skipped this and the stutters reached QC (see V1.93 lesson below).

**Anchor-link verification (run after any heading-text rename):** when the rename touches Markdown heading text, markdown anchor slugs derive from the heading. Grep TOC entries and cross-reference links for slugs built from the *old* heading text and update them to the new slug:

```sh
# after renaming a heading, find TOC/inline links whose slug still reflects the old name
rg -n '#local-api-contract|<old-heading-slug>' docs/ .mstar/specs/
```

A renamed heading whose TOC entry text was updated but whose `#slug` was not produces a silently-broken in-page link.

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

## V1.93 lesson (naming-sweep stragglers — R-V192HYG-001)

V1.93 ran a "Local API" → "Daemon API" straggler sweep (135 line changes across 27 live-prose files). Two failure classes reached QC because **the sweep executor did not run the §3 verification greps before committing**:

- **W-001 — stutter/double-naming**: the mechanical replace over-reached where the surrounding text already referenced "Daemon API" or where "Local" was part of an already-renamed phrase, producing "Daemon Daemon API", "renaming from Daemon API to Daemon API" (should be "from **Local API** to Daemon API" — the *from* side names the historical api), "## 6. Daemon Daemon API (principles)". The §3 stutter grep (`daemon Daemon API` etc.) existed in this checklist but was not run; QC1 caught 6 files + 11 artifacts.
- **W-002 — broken anchor**: a TOC entry's text was renamed ("Local API Contract Analysis" → "Daemon API Contract Analysis") but the anchor slug `#4-local-api-contract-analysis` was not updated to `#4-daemon-api-contract-analysis`. The §3 anchor-link verification gate above did not yet exist in this checklist; it was added as a result of V1.93.

Takeaways: (1) the §3 greps + anchor check are a **mandatory pre-commit self-check** for any rename sweep, not optional; (2) where a construction is "renaming from X to Y", the *X* side is the historical name and must stay — the replace must be context-aware, not a blind token swap; (3) a fix-wave correcting the stutters + anchors landed in the same iteration (commit `ef1b4efa`) before merge.

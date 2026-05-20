# `schemas/` Boundary — Specification v1

**Status**: Active — authoritative SSOT for what belongs in `schemas/` vs what belongs in hand-written Rust under `crates/nexus-contracts/src/local/`.
**Author**: @project-manager (2026-04-17 prep-phase spec) — to be co-signed by @architect before WS5 implement.
**Scope**: The **boundary rule** and **local-type placement convention** for nexus OSS contracts.
**Alignment source**: `{v1-spec}/schema/codegen-strategy-v1.md` §3 — this document is the in-repo SSOT for the decision that v1-spec §3 explicitly **delegates to the contract repo** ("是否在 `nexus` JSON Schema + `@42ch/nexus-contracts` 生成，由合约仓单独推进并对齐本稿"). No v1-spec update is required for the boundary rule itself; a future v1.4 roadmap / ADR update may cite this doc for traceability.

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Authoritative Rule (wire vs local)](#2-authoritative-rule-wire-vs-local)
3. [Local-Type Placement Convention](#3-local-type-placement-convention)
4. [Alignment with v1-spec `codegen-strategy-v1`](#4-alignment-with-v1-spec-codegen-strategy-v1)
5. [Audit Procedure and Template](#5-audit-procedure-and-template)
6. [Initial Classification Sketch (not final)](#6-initial-classification-sketch-not-final)
7. [Migration Steps (for WS5)](#7-migration-steps-for-ws5)
8. [Acceptance Criteria](#8-acceptance-criteria)
9. [Open Questions and Deferred Items](#9-open-questions-and-deferred-items)

---

## 1. Problem Statement

`schemas/` currently hosts **every** JSON Schema the repo owns, regardless of who consumes the generated output. The npm package `@42ch/nexus-contracts` emits TypeScript for all of them, even though its **only consumer is `nexus-platform`**. Two symptoms:

1. **Wasted public TS surface**: types that `nexus-platform` never reads (e.g. ACP registry manifest, local daemon status, CLI-only workspace records) still get generated, published, and carried in the package. Version bumps become noisier than necessary.
2. **Obscured invariant**: a new contributor opening `schemas/` cannot tell which files are wire contracts (platform coordination required for changes) and which are local types (free to evolve inside nexus OSS).

The fix is one rule: `schemas/` hosts **only** what crosses a process boundary that `nexus-platform` observes. Everything else becomes hand-written Rust that ships inside `crates/nexus-contracts` but **does not** participate in codegen.

## 2. Authoritative Rule (wire vs local)

### 2.1 Definition

A schema is **wire** and belongs in `schemas/` iff **both** of the following hold:

1. It appears — **directly, or transitively via `$ref`** — in a contract that is **observed by `nexus-platform`**:
   - **platform HTTP request / response body** (`schemas/platform/*`, or the platform-consumed members of `schemas/cli-sync/*` like `bundle` / `sync-pull-*` / `conflict-response`);
   - **sync bundle payload** transmitted from CLI to platform (`POST /sync/bundles`);
   - **delta / outbox payload** sent to platform;
   - **any future payload** that nexus OSS hands to platform over any channel.
2. Platform (TypeScript) code actually needs to read or write values of this type.

A schema is **local** iff the first criterion does not apply — i.e. platform never sees the type. Local schemas do **not** live in `schemas/`; they live as **hand-written Rust** in `crates/nexus-contracts/src/local/` (see §3).

### 2.2 What counts as "observed by `nexus-platform`"?

**Yes (wire):**

- Platform BFF / REST endpoints declared in `{v1-spec}/platform/platform-api-v1.md`
- Sync protocol (`POST /sync/bundles`, delta payloads, conflict response, pull cursors) declared in `{v1-spec}/cli-sync/sync-contract-v1.md`
- Entitlements / quota responses per `{v1-spec}/schema/entitlements-wire-v1.md`
- Context Assembly wire per `{v1-spec}/schema/context-assembly-wire-v1.md`
- Any future platform-exposed capability payload

**No (local):**

- **Local API endpoints on `nexus42d`** (`/v1/local/*`) — these talk CLI↔daemon, same machine, platform never sees them.
- **ACP Registry manifest** — fetched from a public CDN by CLI / worker; platform never parses it.
- **Agent subprocess JSON-RPC frames** — stdio, local only.
- **`nexus42 acp-worker` ↔ `nexus42d` IPC frames** (V1.4 new) — stdin/stdout between parent/child, local only.
- **Workspace on-disk state records** (SQLite rows, `.nexus42/*` config files) — entirely local.
- **`daemon-status-v2`** (V1.4 WS4 new) — exposed via `/v1/local/daemon/status`, local only.
- **Capability input/output schemas** defined inside orchestration engine — local-only validation of in-process data.

### 2.3 Tight corollary (what this excludes)

If `nexus42d` and `nexus42` exchange a structure over local HTTP or IPC, and platform never receives it, **the structure is local**. Convenience (e.g. "we already had a schema, why not codegen Rust from it too") is **not** a reason to keep it in `schemas/`. Rust inside a single repo can be hand-written; duplicating it in JSON Schema only makes sense when a second language consumes it — and in nexus OSS, TS platform is the only second language.

### 2.4 Shared value objects

`schemas/common/*` and `schemas/meta/*` (utility types like `SourceAnchor`, `VersionRef`, the meta-schema) stay in `schemas/` iff **any** wire schema `$ref`s them. If no wire schema refs a given `schemas/common/<x>` file, that file moves to local.

Audit rule: enumerate `$ref` closure for every wire file first, mark reachable shared files as wire; any unreachable common/meta file is local.

## 3. Local-Type Placement Convention

### 3.1 Target directory

```
crates/nexus-contracts/
├── Cargo.toml
├── src/
│   ├── lib.rs                   # re-exports `generated::*` and `local::*`
│   ├── enum_conversions.rs      # unchanged — hand-maintained conversions for generated enums
│   ├── generated/               # codegen output (do not edit; do not hand-add)
│   │   └── ...
│   └── local/                   # NEW — hand-written, not codegen'd
│       ├── mod.rs
│       ├── registry.rs          # ACP registry manifest types
│       ├── daemon_status.rs     # V1.4 WS4 v2 daemon status response
│       ├── domain/              # domain local types moved from schemas/domain/*
│       │   ├── mod.rs
│       │   └── ...
│       └── orchestration/       # V1.4 WS2/WS3 capability + preset types (if chosen to live here)
│           └── ...
```

### 3.2 Authoring rules for `src/local/*.rs`

- **Hand-written Rust with `serde` derives**; no JSON Schema file, no codegen step, no `pnpm run codegen` side-effect.
- Every public type exports `#[derive(Debug, Clone, Serialize, Deserialize)]` at minimum; optional `schemars::JsonSchema` when a downstream test wants to generate a JSON Schema at runtime for validation (rare — opt-in).
- Unit tests for local types (round-trip serialization, error cases) live alongside in the same module, gated by `#[cfg(test)]` as usual.
- Re-export selected local types at the crate root in `lib.rs` **only if** cross-crate callers need them. Prefer explicit `use nexus_contracts::local::<module>::<Type>` imports over root re-exports to make the "local vs wire" distinction visible at use sites.

### 3.3 Naming / conversion with generated types

- When a local type shares a name with a generated wire type (e.g. a local `DomainCreator` beside generated `Creator`), pick a distinguishing name; do **not** shadow via `pub use`.
- Conversions between generated and local types live in the same module as the **local** type (`crates/nexus-contracts/src/local/<x>.rs`), following the pattern established by `enum_conversions.rs` for generated-enum ↔ domain-enum mapping.

### 3.4 What **must not** live in `src/local/`

- Anything that is actually wire — i.e. anything satisfying §2.1. Violators will eventually break the platform handshake; keep them in `schemas/` and codegen.
- Tooling / scripts that don't belong in a library crate. Those stay under `tooling/`.

## 4. Alignment with v1-spec `codegen-strategy-v1`

### 4.1 What v1-spec §3 says, and how this doc relates

`{v1-spec}/schema/codegen-strategy-v1.md` §3 lists **aggregates, sync objects, shared value objects, platform responses, context-assembly wire, entitlements wire** as "priority objects for JSON Schema / codegen". v1-spec notes that for entitlements specifically, **the contract repo** (nexus OSS) decides whether to emit `.schema.json` + `@42ch/nexus-contracts` entries, as long as output aligns with the v1-spec narrative.

This document is the in-repo binding: we read v1-spec §3 as **"objects which MAY cross to TS when they actually need to"**, and apply §2.1 to concretise "need to". For the listed object categories:

| v1-spec §3 category                        | Our application                                                                         |
| ------------------------------------------ | --------------------------------------------------------------------------------------- |
| Aggregates (`Creator`, `World`, …)         | **Wire iff** carried in a sync-bundle / platform HTTP payload; otherwise local.         |
| Sync objects (`SyncCommand`, `DeltaBundle`, `Delta`, `OutboxEntry`) | **Wire** — platform observes these on `POST /sync/bundles` and associated reads.         |
| Shared value objects (`SourceAnchor`, `VersionRef`) | **Wire iff** `$ref`'d by a wire schema (expected to remain wire).                         |
| Platform responses (`SyncConflictResponseV1`) | **Wire** — explicit.                                                                     |
| Context Assembly wire                      | **Wire** — platform consumes.                                                            |
| Entitlements / Official quota              | **Wire** — platform consumes; decision previously deferred to contract repo, now confirmed wire. |

### 4.2 Items to note back to v1-spec (low-priority, not required for V1.4 ship)

When V1.4 WS5 lands, the platform-side `{v1-spec}/schema/codegen-strategy-v1.md` can optionally gain a footnote pointing to this `schemas-boundary.md` as the in-repo binding for §3's delegation clause. Landing that footnote is a nice-to-have for traceability; not a V1.4 release-gate item.

### 4.3 What this doc is NOT

- Not a replacement for v1-spec `codegen-strategy-v1.md` — the **principle** (JSON Schema as shared intermediate) lives there and remains authoritative.
- Not authoritative on OpenAPI shape — that's `{v1-spec}/schema/openapi-export-policy-v1.md`.
- Not authoritative on specific field definitions — per-schema semantics still belong in their own v1-spec or nexus OSS schema files.

## 5. Audit Procedure and Template

### 5.1 Procedure (to be executed at WS5 start)

1. **Enumerate** every file under `schemas/**/*.schema.json`.
2. **For each file**, determine the first-degree consumer by grep:
   - Search `nexus-platform` repo for direct TypeScript imports of the generated type name(s).
   - Search nexus OSS Rust codebase for `nexus_contracts::<type>` usages.
   - If applicable, trace `$ref` closures: a schema that's only $ref'd by another local schema is itself local.
3. **Apply §2.1**: if platform consumes → wire; else local.
4. **For candidate-local files**, confirm with an additional `rg` pass on `nexus-platform` for all generated TypeScript type names that would vanish (not just the filename — the type names codegen produces).
5. Record the decision + justification in the audit table (§5.2).
6. **Review with an extra pair of eyes** (prefer QC or a second engineer who knows platform) before any file move, especially for `schemas/domain/*` and shared `schemas/common/*` files.

### 5.2 Audit table template

```markdown
| File                                                     | $ref Consumers (in repo)                    | Platform TS Usage? | Decision | Move Target (if local)                                             | Notes                                          |
| -------------------------------------------------------- | ------------------------------------------- | ------------------ | -------- | ------------------------------------------------------------------ | ---------------------------------------------- |
| `schemas/acp-runtime/daemon-status-v2.schema.json`       | self-only (internal defs)                    | No                 | local    | `crates/nexus-contracts/src/local/acp_runtime/daemon_status_v2.rs` | §2.2: `/v1/local/daemon/status`, CLI↔daemon only |
| `schemas/acp-runtime/registry-manifest.schema.json`      | self-only (internal defs)                    | No                 | local    | `crates/nexus-contracts/src/local/acp_runtime/registry_manifest.rs` | External CDN; Rust-only parser                    |
| `schemas/cli-sync/bundle.schema.json`                    | refs domain/bundle, domain/delta             | Yes (via Bundle)   | wire     | —                                                                  | Sync protocol wire                               |
| `schemas/cli-sync/conflict-response.schema.json`         | refs common/common                           | Yes                | wire     | —                                                                  | Sync protocol wire                               |
| `schemas/cli-sync/sync-pull-request.schema.json`         | refs common/common                           | Yes                | wire     | —                                                                  | Sync protocol wire                               |
| `schemas/cli-sync/sync-pull-response.schema.json`        | refs domain/bundle                           | Yes                | wire     | —                                                                  | Sync protocol wire                               |
| `schemas/common/common.schema.json`                      | $ref'd by 20+ wire schemas                   | Yes (indirect)     | wire     | —                                                                  | §2.4 shared value object                        |
| `schemas/common/source-anchor.schema.json`               | refs delta, key-block (both wire)            | Yes (indirect)     | wire     | —                                                                  | §2.4: $ref'd by wire delta + key-block           |
| `schemas/common/version-ref.schema.json`                 | not $ref'd by any schema                     | Yes (2 files)      | wire     | —                                                                  | Platform imports VersionRef directly             |
| `schemas/domain/agent-profile.schema.json`               | refs common/common                           | No                 | local    | `crates/nexus-contracts/src/local/domain/agent_profile.rs`         | CLI-only; 0 platform files                      |
| `schemas/domain/bundle.schema.json`                      | refs domain/delta, common/common             | Yes (38 files)     | wire     | —                                                                  | Sync bundle payload                             |
| `schemas/domain/creator.schema.json`                     | refs common/common                           | Yes (127 files)    | wire     | —                                                                  | Core aggregate; platform reads extensively       |
| `schemas/domain/delta.schema.json`                       | refs common/common, source-anchor            | Yes (23 files)     | wire     | —                                                                  | Sync delta payload                              |
| `schemas/domain/fork-branch.schema.json`                 | refs common/common                           | Yes (8 files)      | wire     | —                                                                  | $ref'd by platform/world-fork-response          |
| `schemas/domain/key-block.schema.json`                   | refs common/common, source-anchor            | Yes (19 files)     | wire     | —                                                                  | Platform graph projection, Neo4j, context asm   |
| `schemas/domain/local-identity.schema.json`              | refs common/common                           | No                 | local    | `crates/nexus-contracts/src/local/domain/local_identity.rs`        | Name implies local; 0 platform files            |
| `schemas/domain/manuscript-state.schema.json`            | refs common/common                           | No                 | local    | `crates/nexus-contracts/src/local/domain/manuscript_state.rs`      | CLI-only; 0 platform files                      |
| `schemas/domain/memory.schema.json`                      | refs common/common                           | Yes (32 files)     | wire     | —                                                                  | Platform reads Memory items                     |
| `schemas/domain/outbox-entry.schema.json`                | refs common/common                           | No                 | local    | `crates/nexus-contracts/src/local/domain/outbox_entry.rs`          | Spec §6: "verify platform observes" → 0 hits   |
| `schemas/domain/pairing.schema.json`                     | refs common/common                           | Yes (36 files)     | wire     | —                                                                  | Platform reads pairing data                     |
| `schemas/domain/reference-source.schema.json`            | refs common/common                           | No                 | local    | `crates/nexus-contracts/src/local/domain/reference_source.rs`      | CLI-only; 0 platform files                      |
| `schemas/domain/runtime-mode.schema.json`                | refs common/common                           | No                 | local    | `crates/nexus-contracts/src/local/domain/runtime_mode.rs`          | Local-first/cloud gate; 0 platform files        |
| `schemas/domain/story-manifest.schema.json`              | refs common/common                           | Yes (5 files)      | wire     | —                                                                  | Platform reads story manifests                  |
| `schemas/domain/sync-command.schema.json`                | refs common/common                           | Yes (12 files)     | wire     | —                                                                  | Sync protocol wire                              |
| `schemas/domain/timeline-event.schema.json`              | refs common/common                           | Yes (18 files)     | wire     | —                                                                  | Platform reads timeline events                  |
| `schemas/domain/user.schema.json`                        | refs common/common                           | Yes (90 files)     | wire     | —                                                                  | Core aggregate; platform reads extensively       |
| `schemas/domain/workspace-binding.schema.json`           | refs common/common                           | No                 | local    | `crates/nexus-contracts/src/local/domain/workspace_binding.rs`     | CLI-only on-disk state; 0 platform files       |
| `schemas/domain/world-membership.schema.json`            | refs common/common                           | Yes (9 files)      | wire     | —                                                                  | Platform reads membership data                  |
| `schemas/domain/world.schema.json`                       | refs common/common                           | Yes (87 files)     | wire     | —                                                                  | Core aggregate; platform reads extensively       |
| `schemas/meta/meta.schema.json`                          | not $ref'd by any schema                     | No                 | local    | `crates/nexus-contracts/src/local/meta.rs`                          | Repo-internal schema validation; 0 platform     |
| `schemas/platform/context-assembly-v1.schema.json`       | refs common/common                           | Yes                | wire     | —                                                                  | Platform context assembly wire                  |
| `schemas/platform/creator-runtime-policy-response.schema.json` | refs common/common                       | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/explore-ai-answer-request.schema.json` | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/explore-ai-answer-response.schema.json`| refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/explore-ai-summary-request.schema.json`| refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/explore-ai-summary-response.schema.json`| refs common/common                          | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/explore-browse-request.schema.json`    | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/explore-creator-card.schema.json`      | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/explore-feed-response.schema.json`     | refs common/common, explore-hit              | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/explore-hit.schema.json`               | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response (via explore-feed)       |
| `schemas/platform/explore-search-request.schema.json`    | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/me-entitlements-response.schema.json`  | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/memory-web-list-request.schema.json`   | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/memory-web-list-response.schema.json`  | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/notifications-inbox-item.schema.json`  | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/notifications-list-request.schema.json` | refs common/common                          | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/notifications-list-response.schema.json`| refs common/common                          | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/notifications-mark-read-request.schema.json` | refs common/common                     | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/notifications-mark-read-response.schema.json` | refs common/common                    | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/official-creator-quota-response.schema.json` | refs common/common                    | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/publish-chapter-request.schema.json`   | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/publish-history-entry.schema.json`     | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/publish-history-request.schema.json`   | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/publish-history-response.schema.json`  | refs common/common, publish-history-entry     | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/publish-story-request.schema.json`     | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/publish-story-response.schema.json`    | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/social-graph-feed-request.schema.json`  | refs common/common                        | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/social-graph-feed-response.schema.json` | refs common/common                        | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/social-graph-relationship-request.schema.json` | refs common/common                    | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/social-graph-relationship-response.schema.json` | refs common/common                   | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/world-fork-request.schema.json`        | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/world-fork-response.schema.json`       | refs common/common, domain/fork-branch       | Yes                | wire     | —                                                                  | Platform HTTP response                          |
| `schemas/platform/world-snapshot-request.schema.json`    | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP request                           |
| `schemas/platform/world-snapshot-response.schema.json`   | refs common/common                           | Yes                | wire     | —                                                                  | Platform HTTP response                          |
```

**Total: 64 files.  53 wire, 10 local.**

**RISK-06 check**: 13 of 20 `schemas/domain/*` files stay wire (65%). Per compass RISK-06,
the clearly-local subset (7 domain + 2 acp-runtime + 1 meta = 10 local) ships in V1.4;
remaining domain reclassification is not needed — all are clearly wire or clearly local.

WS5 deliverable: populate the full table in a new section of this doc (or as a separate companion doc `schemas-boundary-audit-v1.md` if the table grows large enough to deserve its own file).

### 5.3 Coordination with `nexus-platform` before any file move

For every candidate-local schema whose generated TypeScript type the codegen would delete, run `rg <type-name>` on `nexus-platform` **before** committing the move. If platform imports the type anywhere (including in tests), reclassify as wire and keep in `schemas/`. This mitigates RISK-04 (audit incompleteness) from the V1.4 delivery compass.

## 6. Initial Classification Sketch (not final)

This sketch is the PM's best-guess starting point for WS5's auditor. It is **not authoritative** and will be revised as the audit produces evidence. WS5's final audit table supersedes this sketch.

**Likely wire (keep in `schemas/`):**

- `schemas/platform/**/*.schema.json` (33 files): all platform BFF HTTP contracts
- `schemas/cli-sync/bundle.schema.json`
- `schemas/cli-sync/sync-pull-request.schema.json`
- `schemas/cli-sync/sync-pull-response.schema.json`
- `schemas/cli-sync/conflict-response.schema.json`
- `schemas/domain/bundle.schema.json` (if different from `cli-sync/bundle`; otherwise consolidate)
- `schemas/domain/delta.schema.json` (carried in bundle)
- `schemas/domain/sync-command.schema.json` (sync protocol)
- `schemas/domain/outbox-entry.schema.json` (sync protocol; verify platform observes this; if platform doesn't, reclassify local)
- `schemas/domain/creator.schema.json`, `world.schema.json`, `world-membership.schema.json`, `story-manifest.schema.json`, `fork-branch.schema.json`, `timeline-event.schema.json`, `memory.schema.json`, `user.schema.json`, `pairing.schema.json` (aggregates carried in sync bundles per v1-spec §3 expectation — verify each)
- `schemas/common/source-anchor.schema.json`, `version-ref.schema.json` (shared value objects — verify $ref'd by wire)
- `schemas/common/common.schema.json` (utility shapes — verify)
- `schemas/meta/meta.schema.json` (meta-schema — verify needed at all; if purely for schema validation inside repo, local)

**Likely local (move to `crates/nexus-contracts/src/local/`):**

- `schemas/acp-runtime/registry-manifest.schema.json` → `local/registry.rs`
- `schemas/acp-runtime/daemon-status-v2.schema.json` (V1.4 WS4 new) → `local/daemon_status.rs`
- `schemas/domain/agent-profile.schema.json` — verify platform sync vs CLI-only
- `schemas/domain/key-block.schema.json` — verify
- `schemas/domain/local-identity.schema.json` — name implies local; verify no platform consumer
- `schemas/domain/manuscript-state.schema.json` — verify
- `schemas/domain/reference-source.schema.json` — verify
- `schemas/domain/runtime-mode.schema.json` — known to be used by local_first/cloud_enhanced gates; verify platform doesn't read it over wire
- `schemas/domain/workspace-binding.schema.json` — verify

**Expected orchestration-era additions (V1.4 WS2/WS3 new; all local):**

- Preset manifest types (`PresetManifest`, `StateDefinition`, `GraphNode`, …)
- Capability input/output schemas (declared in Rust constants per ../../knowledge/specs/orchestration-engine.md §5.3)
- Orchestration session state records (rows in `orchestration_sessions` table)
- Worker IPC frame types
- Creator Schedule types (V1.4 WS7; see [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md))

## 7. Migration Steps (for WS5)

Sequencing is important: audit → platform-side confirmation → moves → regeneration → npm bump. Each step produces evidence.

### Step 1 — Audit

- Populate the §5.2 audit table to 100% coverage of `schemas/**/*.schema.json`.
- Commit the audit table to this doc (or a companion audit-v1.md).
- **Evidence**: table committed; reviewer sign-off recorded in the WS5 plan's QC triple review.

### Step 2 — Platform-side confirmation

- For each candidate-local schema, `rg <generated-type-name>` on `nexus-platform`.
- Summarise in a short note inside the WS5 plan: "Of N candidate-local schemas, M showed no platform usage; K reclassified to wire after audit; net result: N–K local moves planned."
- **Evidence**: grep output excerpts (with hashes for reproducibility if desired) in the plan's evidence snapshot.

### Step 3 — Move local types to `crates/nexus-contracts/src/local/`

- For each agreed local schema:
  - Author the corresponding Rust type in `crates/nexus-contracts/src/local/<name>.rs`.
  - Migrate any Rust call sites previously using `nexus_contracts::<GeneratedType>` to `nexus_contracts::local::<module>::<Type>` (typically a one-line `use` change per call site).
  - Delete the `.schema.json` file.
  - If the type had a `schemas/common/*` dependency that becomes orphaned (no other wire schema $refs it), plan it for the same move.
- **Evidence**: `cargo build --workspace` clean; `cargo test --workspace` green; `cargo clippy --all -- -D warnings` clean.

### Step 4 — Regenerate + verify-codegen

- Run `pnpm run codegen` — this picks up the thinned `schemas/` automatically; no codegen configuration change should be needed (tooling/codegen/ iterates the dir).
- Confirm `packages/nexus-contracts/src/generated/*.ts` and `crates/nexus-contracts/src/generated/*.rs` shrink in expected ways (only the intended types are gone).
- `git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/` after regeneration should show **only deletions** (no unrelated churn).
- **Evidence**: `pnpm run validate-schemas` passes; `verify-codegen` CI job green.

### Step 5 — npm package bump + publish note

- Bump `packages/nexus-contracts/package.json` to the next **minor** (public TS surface shrinks; non-breaking for platform since platform never used those types, but a minor bump signals the change).
- Add a release note to `packages/nexus-contracts/README.md` (or a CHANGELOG entry) noting which types were removed and linking to this doc for the reasoning.
- **Evidence**: npm publish succeeds; platform CI on its next `@42ch/nexus-contracts` upgrade stays green.

### Step 6 — Cross-repo handoff note

- Per V1.4 delivery compass §7 "Cross-repo dependency compass", post a short note in `nexus-platform`'s plan / issues tracker summarising the move (one paragraph). No code change required in `nexus-platform`; the note exists so platform engineers don't later look for a vanished type and wonder why.

## 8. Acceptance Criteria

- [ ] Audit table covers 100% of `schemas/**/*.schema.json` files.
- [ ] For every candidate-local schema, platform-side `rg` confirmation is recorded.
- [ ] Every moved local schema has its Rust counterpart under `crates/nexus-contracts/src/local/` with round-trip serialization test.
- [ ] `pnpm run codegen && git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/` clean (only intended deletions).
- [ ] `pnpm run validate-schemas` clean.
- [ ] `cargo test --workspace`, `cargo clippy --all -- -D warnings`, `cargo +nightly fmt --all -- --check` clean.
- [ ] `packages/nexus-contracts` minor bumped; release note added.
- [ ] Cross-repo handoff note posted in `nexus-platform` tracker.
- [ ] V1.4 delivery compass §4 WS5 rows' evidence fields filled in.

## 9. Open Questions and Deferred Items

### OQ-S1 — Should `schemas/common/*` split into wire-common / local-common subfolders?

At V1.4 we keep the existing flat layout and classify per-file. If future growth makes this confusing, a follow-up plan may introduce `schemas/common/wire/` and `crates/nexus-contracts/src/local/common/` for clarity. **Deferred to V1.5+** unless the V1.4 audit surfaces an ambiguity that forces it earlier.

### OQ-S2 — schema evolution policy for local types

Wire types follow `schema_version` bumps coordinated with platform. Local types are internal Rust — we can evolve them freely with normal crate SemVer. We should note in a future `nexus-contracts` README section that **hand-written types in `src/local/` are NOT covered by the schema_version contract** (they're Rust SemVer only). **Deferred to the WS5 README update**; not a release-gate item.

### OQ-S3 — Future tooling: auto-detect wire/local boundary

A lint that fails CI when a developer adds a `.schema.json` with no platform consumer would operationalise this doc's rule. V1.4 does not build such tooling; a ticket should be filed post-V1.4 under "developer experience / contract hygiene" backlog.

### OQ-S4 — When does v1-spec need a mirror?

If this doc's rule changes in a way that materially affects platform (e.g. reclassifying a formerly-wire type to local, breaking platform assumptions), post a matching amendment to `{v1-spec}/schema/codegen-strategy-v1.md` in the same change window. For V1.4's rollout (wire-rule tightening, no wire types removed), no v1-spec mirror is required; a follow-up footnote is optional (see §4.2).

---

## References

Internal:

- [v1.4-delivery-compass-v1.md](../../iterations/v1.4-delivery-compass-v1.md) §4 WS5 — scope, milestones, evidence
- [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §5.3 — capability schemas are local per this doc
- [architecture-alignment-review.md](architecture-alignment-review.md) — live TD resolution matrix

External (v1-spec, read via `.agents/local-paths.json → specs_root.v1-spec`):

- `{v1-spec}/schema/codegen-strategy-v1.md` — parent principle (JSON Schema as shared truth source); §3 delegates boundary to this doc
- `{v1-spec}/schema/openapi-export-policy-v1.md` — OpenAPI shape (separate concern)
- `{v1-spec}/platform/platform-api-v1.md` — platform HTTP API (primary wire surface)
- `{v1-spec}/cli-sync/sync-contract-v1.md` — sync wire format (secondary wire surface)

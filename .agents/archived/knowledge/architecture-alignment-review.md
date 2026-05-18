# Architecture Alignment Review — Nexus `crates/` vs v1-spec

**Author**: @architect  
**Baseline review date**: 2026-04-08  
**SSOT last reconciled**: 2026-04-09  
**Status**: Active — **resolution SSOT** for architecture-alignment technical debt (TD-1…TD-13) and its mapping to program plans; **baseline analysis** for §2 dimension tables remains the 2026-04-08 snapshot unless a row is explicitly updated below.  
**Scope**: `crates/nexus-contracts`, `nexus-domain`, `nexus-local-db`, `nexus-sync`, `nexus42` (CLI), `nexus42d` (daemon)  
**Benchmark**: `v1-spec/` (architecture/v1.md, domain/data-model-v1.md, cli-sync/cli-spec-v1.md, acp-runtime/acp-client-tech-spec-legacy.md, cli-sync/sync-contract-v1.md)

**Authoritative execution SSOT**: Open **residual rows** and plan lifecycle live in [`.agents/status.json`](../status.json) (**root** `residual_findings`, `plans[]`). This document states **intent and narrative**; if a statement here disagrees with `status.json`, **fix this document** in the same change set that updates the SSOT file.

**Related Documents**:

- [v1.1-overview-v2.md](../../iterations/v1.1-overview-v2.md) — V1.1 program overview (nexus OSS); long-form legacy tables in [program-overview-legacy.md](program-overview-legacy.md)
- [README.md](README.md) — Knowledge base index and maintenance guidelines
- [2026-04-09-v1.1-arch-alignment-closure.md](../2026-04-09-v1.1-arch-alignment-closure.md) — Plan to **fully close** remaining open alignment residuals (canonical_hash cross-stack parity; optional daemon eager push)

---

## 1. Executive Summary

### 1.1 Baseline assessment (2026-04-08)

The Nexus `crates/` implementation showed **strong alignment (~75%)** with the v1-spec design across architectural boundaries, data models, and sync contracts. Core architectural decisions—Rust-first CLI/daemon, ACP Client-only topology, wire contracts single-source generation, and local-first data authority—were correctly implemented. The domain model layer was nearly complete with all 15 aggregates represented, and the enum table in §7 of data-model-v1.md was fully covered **from a review perspective**.

At baseline, several **critical and high gaps** were recorded (verbatim historical list — **do not treat as current risk without reading §1.2**):

1. **Outbox/Sync dual implementation**: `nexus-sync` had a comprehensive outbox with SQLite, connection pooling, precheck, partial apply, and conflict resolution, but was **not wired into the CLI or daemon commands** — `nexus42 sync push/pull` called the daemon's HTTP API, which had only skeleton sync handlers (501 or stub for sync).
2. **World status enum mismatch**: Domain `WorldStatus` used `Frozen` where spec §7 defines `Paused`.
3. **MembershipStatus enum mismatch**: Domain used `Left` where spec §7 defines `Removed`.
4. **Wire contracts (nexus-contracts) use String-typed enum fields**: 28+ fields that should be enum-typed per the design spec were plain `String` in generated code, creating validation gaps.
5. **User aggregate and SyncCommand/Delta/OutboxEntry aggregates missing from domain**: Only wire contracts existed for some of these; domain logic layer gaps were called out.

### 1.2 Resolution SSOT (sync with `status.json`, 2026-04-09)

**Plans (execution)**

| Plan id | Title (short) | `plans[].status` in SSOT | Relationship to this review |
| -------- | ---------------- | -------------------------- | ----------------------------- |
| `2026-04-08-v1.1-tech-debt-mitigation` | V1.1 tech debt mitigation | Done | Declared scope includes **TD-7…TD-13** per `2026-04-09-v1.1-arch-alignment-blockers.md` cross-reference table |
| `2026-04-09-v1.1-arch-alignment-blockers` | TD-1…TD-6 blockers | Done | Explicitly closes **TD-1…TD-6** scope; **two** follow-up rows remain open under this plan id in `residual_findings` |
| `2026-04-09-v1.1-arch-alignment-closure` | Final alignment closure | See SSOT | **Authoritative plan** to eliminate the two open rows below and sign off cross-stack parity |

**TD resolution matrix (authoritative for “is this done?”)**

| ID | Baseline (§2.6) | Resolution statement (SSOT) | Open residual id (**root** `residual_findings` key = owning plan id) |
| --- | --- | --- | --- |
| TD-1 | Critical — sync not wired | **Mitigated** — blocker plan Done; offline-first queue and integration delivered per plan. Optional **eager platform push** from daemon remains deferred. | `2026-04-09-v1.1-arch-alignment-blockers` → **ARCH-SYNC-D1** (low, defer) |
| TD-2 | Critical — `Frozen` vs `Paused` | **Resolved** — closed under blocker plan (no open row). | — |
| TD-3 | Critical — `Left` / `Removed` / `Invited` | **Resolved** — closed under blocker plan (no open row). | — |
| TD-4 | High — String enums in contracts | **Resolved** — schema/codegen alignment delivered under blocker plan (no open row under this plan for TD-4). | — |
| TD-5 | High — `canonical_hash` | **Partial** — in-repo computation exists; **cross-stack preimage**, golden vectors, and platform parity **not** closed. | `2026-04-09-v1.1-arch-alignment-blockers` → **ALIGN-HASH-01** (medium, open) |
| TD-6 | High — User aggregate | **Resolved** — closed under blocker plan (no open row). | — |
| TD-9 | Medium — Daemon state machine | **Resolved (v2)** — WS4 implemented full statig-based HSM with 6 states, entry/exit actions, subsystem bootstrap trait, v2 HTTP endpoint, signal handlers, panic bridge. See [daemon-lifecycle-api.md](daemon-lifecycle-api.md). | — |
| TD-7…TD-13 | Medium / Low / N/A | **Treated as in-scope for** `2026-04-08-v1.1-tech-debt-mitigation` **per blocker plan text**; **TD-12** was “no issue” at baseline. Other program debt (e.g. QC warnings on that plan) may still appear under **different** `residual_findings` keys — see SSOT file. | See `status.json` keys `2026-04-08-v1.1-tech-debt-mitigation`, `2025-04-05-domain-models`, etc. |

**Counting helper**

- **No open residual under** `2026-04-09-v1.1-arch-alignment-blockers`: **4** TD rows (TD-2, TD-3, TD-4, TD-6) at last reconciliation.
- **Still tracked** on that plan id: **2** (ALIGN-HASH-01, ARCH-SYNC-D1).
- **Full program open residual count** is **not** duplicated here — use `metadata.tech_debt_summary` in `status.json`.

**Maintenance rule**: When ALIGN-HASH-01 and ARCH-SYNC-D1 are closed and archived per plan-convention, update §1.2 and the “SSOT last reconciled” header date in the **same** merge as `status.json` and [archived residuals](../archived/residuals/).

---

## 2. Dimension-by-Dimension Analysis

### 2.1 Architecture Alignment


| Aspect                                                   | Spec Requirement                                                                             | Implementation Status                                                                                                                                                                                        | Alignment                                                                                                                               |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| Rust-first CLI/daemon                                    | v1.md §9.3: CLI framework in Rust, single binary distribution                                | `nexus42` (CLI) and `nexus42d` are both Rust crates with Clap-based command structure                                                                                                                        | **Aligned**                                                                                                                             |
| ACP Client-only topology                                 | v1.md §6.2.1: `nexus42d` is NOT an ACP Agent, only a local supervisor                        | Daemon has no ACP server implementation; provides `/v1/local/acp/tool/execute` as mediation endpoint for the CLI's ACP client adapter. Confirmed in code comments and handler flow                           | **Aligned**                                                                                                                             |
| Platform/Local authority boundary                        | v1.md §5: Local owns manuscripts/drafts/agent config; Platform owns World/KB/Timeline/Memory | Workspace structure follows `Stories/<world_ref>/` and `References/<creator_ref>/`; SQLite stores working copies; daemon doesn't upload full text                                                            | **Aligned**                                                                                                                             |
| Wire contracts single-source                             | v1.md §12.3A: JSON Schema → TS + Rust codegen                                                | `schemas/` contains 25 JSON Schema files → `pnpm run codegen` → `nexus-contracts/generated/` (Rust) + `packages/nexus-contracts/src/generated/` (TS). CI enforces via `verify-codegen`                       | **Aligned**                                                                                                                             |
| CLI as platform connector + local runtime + agent bridge | v1.md §6: CLI connects platform, manages workspace, bridges agents                           | CLI has `auth`, `sync`, `daemon`, `context`, `manuscript`, `research`, `creator`, `agent` command groups. ACP module implements Registry fetch, agent subprocess, skills/capability declaration              | **Aligned**                                                                                                                             |
| Daemon as local supervisor                               | cli-spec §10: daemon holds runtime context, manages sync, IPC                                | Daemon listens on localhost:8420 (or Unix socket), manages auth tokens, workspace state, ACP session persistence, and tool mediation. Does NOT speak ACP protocol                                            | **Aligned**                                                                                                                             |
| Workspace structure                                      | cli-spec §13: `Stories/<world_ref>/`, `References/<creator_ref>/`, `$HOME/.nexus42/`         | CLI `init` creates `.nexus42/`, `Stories/`, `References/`; config uses `$HOME/.nexus42/`; DB at `$HOME/.nexus42/workspaces/<id>/sqlite/nexus.db` (daemon) or per-workspace `.nexus42/state.db` (CLI variant) | **Partial** — daemon uses `~/.nexus42/state.db` flat structure instead of spec's `~/.nexus42/workspaces/<workspace_id>/sqlite/nexus.db` |


**Key Finding**: The daemon's SQLite path diverges from the spec. The spec requires `$HOME/.nexus42/workspaces/<workspace_id>/sqlite/nexus.db`, but the daemon uses a single `$HOME/.nexus42/state.db`. The `nexus-local-db` crate's `state_db_path()` returns `~/.nexus42/state.db`, not a workspace-scoped path. This is acceptable for V1.0 (single-workspace) but needs addressing for multi-workspace support in V1.1+.

---

### 2.2 Domain Model Completeness

#### Aggregate Coverage (15 aggregates from data-model-v1.md)


| Aggregate        | Spec § | nexus-domain Implementation                   | Wire Contract (nexus-contracts) | Alignment                                                                                                                   |
| ---------------- | ------ | --------------------------------------------- | ------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| User             | §5.1   | ✅ `user.rs`                                   | ✅ `user.schema.json` / generated | **Aligned** — wire + domain delivered (refresh 2026-04-10)                                                                  |
| Creator          | §5.2   | ✅ `creator.rs`                                | ✅ `creator.rs`                  | **Aligned**                                                                                                                 |
| Pairing          | §5.2A  | ✅ `pairing.rs`                                | ✅ `pairing.rs`                  | **Aligned**                                                                                                                 |
| World            | §5.3   | ✅ `world.rs`                                  | ✅ `world.rs`                    | **Aligned** — `WorldStatus` includes `paused` per spec (TD-2 resolved)                                                       |
| WorldMembership  | §5.4   | ✅ `world_membership.rs`                       | ✅ `world_membership.rs`         | **Aligned** — `MembershipStatus` includes `invited` / `removed` (TD-3 resolved)                                            |
| KeyBlock         | §5.5   | ✅ `key_block.rs`                              | ✅ `key_block.rs`                | **Aligned** (G1 fixes applied)                                                                                              |
| TimelineEvent    | §5.6   | ✅ `timeline_event.rs`                         | ✅ `timeline_event.rs`           | **Aligned**                                                                                                                 |
| ForkBranch       | §5.7   | ✅ `fork_branch.rs`                            | ✅ `fork_branch.schema.json`    | **Aligned** — `parent_branch_id` + `forked_from_event_id` on wire and domain (TD-7 verified)                                 |
| MemoryItem       | §5.8   | ✅ `memory_item.rs`                            | ✅ `memory.rs`                   | **Aligned** (8-value MemoryKind per ADR-001)                                                                                |
| StoryManifest    | §5.9   | ✅ `story_manifest.rs`                         | ✅ `story_manifest.rs`           | **Aligned**                                                                                                                 |
| ReferenceSource  | §5.9A  | ✅ `reference_source.rs`                       | ✅ `reference_source.rs`         | **Aligned**                                                                                                                 |
| ManuscriptState  | §5.9B  | ✅ `manuscript_state.rs`                       | ✅ `manuscript_state.rs`         | **Aligned**                                                                                                                 |
| SyncCommand      | §5.10  | ❌ Not in domain                               | ✅ `sync_command.rs`             | **Gap** — domain logic missing                                                                                              |
| DeltaBundle      | §5.11  | ❌ Not in domain (builder in nexus-sync)       | ✅ `bundle.rs`                   | **Gap** — domain logic in sync crate, not domain                                                                            |
| Delta            | §5.12  | ❌ Not in domain (type in nexus-sync)          | ✅ `delta.rs`                    | **Gap** — see detail                                                                                                        |
| OutboxEntry      | §5.13  | ❌ Not in domain (reimplemented in nexus-sync) | ✅ `outbox_entry.rs`             | **Gap** — dual implementation                                                                                               |
| WorkspaceBinding | §5.14  | ❌ Not in domain                               | ✅ `workspace_binding.rs`        | **Gap** — domain logic missing                                                                                              |
| AgentProfile     | §5.15  | ❌ Not in domain                               | ✅ `agent_profile.rs`            | **Gap** — domain logic missing                                                                                              |


**Summary**: 12 of 17 aggregates have domain implementations. 5 are missing domain logic (SyncCommand, DeltaBundle/Delta, OutboxEntry, WorkspaceBinding, AgentProfile). Of the 12 implemented, aggregate tables above are **Aligned** as of 2026-04-10 (historical enum gaps TD-2/TD-3 closed under arch-alignment blockers).

*Table last refreshed: 2026-04-10 — see [specs-align-review.md](archived/knowledge/specs-align-review.md) (archived 2026-04-17 after remediation plan Done).*

#### Enum Alignment (§7 of data-model-v1.md)

Full enum-by-enum comparison:


| Aggregate           | Field                   | Spec §7 Values                                                                                                        | Domain Impl                                                                   | Contract Impl            | Match?                                      |
| ------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ------------------------ | ------------------------------------------- |
| User                | `account_status`        | `active, suspended, deleted`                                                                                          | ✅ `user.rs` + `AccountStatus`                                               | ✅ JSON Schema enum / generated types | ✅                                           |
| Creator             | `status`                | `active, archived, locked`                                                                                            | ✅ `Active, Archived, Locked`                                                  | `String`                 | ✅ values / ⚠️ type                          |
| Creator             | `registration_source`   | `cli, web_agent, platform`                                                                                            | ✅ `Cli, WebAgent, Platform`                                                   | `String`                 | ✅ values / ⚠️ type                          |
| Pairing             | `pairing_source`        | `auto_cli, manual_web, platform_auto`                                                                                 | ✅ `AutoCli, ManualWeb, PlatformAuto`                                          | `String`                 | ✅ values / ⚠️ type                          |
| Pairing             | `status`                | `active, revoked`                                                                                                     | ✅ `Active, Revoked`                                                           | `String`                 | ✅ values / ⚠️ type                          |
| User                | `subscription_tier`     | `free, pro, studio, enterprise`                                                                                       | ✅ `user.rs` + `SubscriptionTier`                                            | ✅ JSON Schema enum / generated types | ✅                                           |
| World               | `status`                | `active, paused, archived`                                                                                            | ✅ `Active, Paused, Archived`                                                | ✅ aligned via schema enums where generated | ✅ (TD-2)                                    |
| World               | `visibility`            | `private, unlisted, public`                                                                                           | ✅ Uses `nexus_contracts::Visibility`                                          | ✅ `Visibility` enum      | ✅                                           |
| World               | `time_policy`           | `manual, owner_driven, event_driven`                                                                                  | ✅ Uses `nexus_contracts::TimePolicy`                                          | ✅ `TimePolicy` enum      | ✅                                           |
| WorldMembership     | `role`                  | `owner, maintainer, collaborator, official_creator`                                                                   | ✅ `Owner, Maintainer, Collaborator, OfficialCreator`                          | `String`                 | ✅ values / ⚠️ type                          |
| WorldMembership     | `membership_status`     | `active, invited, suspended, removed`                                                                               | ✅ `Active, Invited, Suspended, Removed`                                      | ✅ aligned via schema where generated | ✅ (TD-3)                                    |
| KeyBlock            | `block_type`            | `character, ability, scene, organization, item, conflict, info_point, event`                                          | ✅ Uses `nexus_contracts::BlockType` (8 variants)                              | ✅ `BlockType` enum       | ✅                                           |
| KeyBlock            | `status`                | `provisional, confirmed, deprecated, merged, deleted`                                                                 | ✅ `Provisional, Confirmed, Deprecated, Merged, Deleted`                       | `String`                 | ✅ values / ⚠️ type                          |
| TimelineEvent       | `event_type`            | `story_advance, state_update, fork_marker, official_progression, publish_marker`                                      | ✅ `StoryAdvance, StateUpdate, ForkMarker, OfficialProgression, PublishMarker` | `String`                 | ✅ values / ⚠️ type                          |
| TimelineEvent       | `status`                | `canon, provisional, rejected`                                                                                        | ✅ `Canon, Provisional, Rejected`                                              | `String`                 | ✅ values / ⚠️ type                          |
| ForkBranch          | `status`                | `active, archived`                                                                                                    | ✅ `Active, Archived`                                                          | `String`                 | ✅ values / ⚠️ type                          |
| ForkBranch          | `verification_status`   | `unverified, requested, verified, rejected`                                                                           | ✅ `Unverified, Requested, Verified, Rejected`                                 | `String`                 | ✅ values / ⚠️ type                          |
| MemoryItem          | `memory_type`           | `canon, working, experience`                                                                                          | ✅ Uses `nexus_contracts::MemoryType`                                          | ✅ `MemoryType` enum      | ✅                                           |
| MemoryItem          | `memory_kind`           | `story_summary, research_material, review_note, character_note, world_building, plot_outline, theme_analysis, custom` | ✅ `MemoryKind` enum (8 variants)                                              | `Option<String>`         | ✅ values / ⚠️ type                          |
| MemoryItem          | `status`                | `active, superseded, archived`                                                                                        | ✅ `Active, Superseded, Archived`                                              | `String`                 | ✅ values / ⚠️ type                          |
| StoryManifest       | `manifest_type`         | `chapter, arc, story, excerpt`                                                                                        | ✅ `Chapter, Arc, Story, Excerpt`                                              | `String`                 | ✅ values / ⚠️ type                          |
| StoryManifest       | `status`                | `summary_ready, staged_for_publish, published, archived`                                                              | ✅ `SummaryReady, StagedForPublish, Published, Archived`                       | `String`                 | ✅ values / ⚠️ type                          |
| StoryManifest       | `manuscript_storage`    | `none, local_workspace, platform_sandbox`                                                                             | ✅ `None, LocalWorkspace, PlatformSandbox`                                     | `Option<String>`         | ✅ values / ⚠️ type                          |
| ReferenceSource     | `source_type`           | `file, pdf, url, note`                                                                                                | ✅ `File, Pdf, Url, Note`                                                      | `String`                 | ✅ values / ⚠️ type                          |
| ReferenceSource     | `scan_status`           | `pending, scanned, failed, ignored`                                                                                   | ✅ `Pending, Scanned, Failed, Ignored`                                         | `String`                 | ✅ values / ⚠️ type                          |
| ManuscriptState     | `manuscript_phase`      | `brainstorm, draft, review, finalize, published`                                                                      | ✅ Uses `nexus_contracts::ManuscriptPhase`                                     | ✅ `ManuscriptPhase` enum | ✅                                           |
| SyncCommand         | `command_type`          | `advance_world, inject_future_event, extract_kb, sync_push, sync_pull, fork_world, publish_story`                     | ✅ `SyncCommandVariant` in nexus-sync                                          | `String`                 | ✅ values (nexus-sync) / ⚠️ type (contracts) |
| SyncCommand         | `origin`                | `local_user, local_agent, official_creator, system`                                                                   | ✅ `CommandOrigin` in nexus-sync                                               | `String`                 | ✅ values / ⚠️ type                          |
| SyncCommand         | `status`                | `pending, running, completed, failed, cancelled`                                                                      | ❌ Not in domain                                                               | `String`                 | N/A                                         |
| DeltaBundle         | `bundle_type`           | `world_sync, memory_sync, publish_metadata`                                                                           | ✅ Uses `nexus_contracts::BundleType`                                          | ✅ `BundleType` enum      | ✅                                           |
| Delta               | `delta_type`            | `world, key_block, timeline_event, fork_branch, memory_item, story_manifest`                                          | ✅ `DeltaType` in nexus-sync                                                   | `String`                 | ✅ values / ⚠️ type                          |
| Delta               | `operation`             | `create, update, upsert, delete, append`                                                                              | ✅ `DeltaOperation` in nexus-sync                                              | `String`                 | ✅ values / ⚠️ type                          |
| OutboxEntry         | `delivery_state`        | `staged, ready, sent, acked, conflicted, failed`                                                                      | ✅ `DeliveryState` in nexus-sync                                               | `String`                 | ✅ values / ⚠️ type                          |
| WorkspaceBinding    | `binding_status`        | `active, unlinked, stale`                                                                                             | ❌ Not in domain                                                               | `String`                 | N/A                                         |
| AgentProfile        | `profile_kind`          | `local_agent, platform_hosted`                                                                                        | ❌ Not in domain                                                               | `String`                 | N/A                                         |
| AgentProfile        | `selection_mode`        | `registry, manual_command, manual_remote`                                                                             | ❌ Not in domain                                                               | `String`                 | N/A                                         |
| AgentProfile        | `transport`             | `stdio, http, websocket`                                                                                              | ❌ Not in domain                                                               | `Option<String>`         | N/A                                         |
| AgentProfile        | `status`                | `active, unavailable, deprecated`                                                                                     | ❌ Not in domain                                                               | `String`                 | N/A                                         |


**Key Findings:**

1. **Historical enum mismatches (TD-2, TD-3) — resolved** under `2026-04-09-v1.1-arch-alignment-blockers`. `WorldStatus` and `MembershipStatus` now match data-model §7 in domain + JSON Schema SSOT.
2. **Wire enum typing (TD-4) — resolved** for schema-driven fields: JSON Schema enums flow into generated Rust/TS where the codegen path emits named enums; some legacy fields may still deserialize as `String` — treat as **residual quality**, not an open spec contradiction.
3. **5 aggregates without dedicated `nexus-domain` modules**: SyncCommand, DeltaBundle/Delta, OutboxEntry, WorkspaceBinding, AgentProfile — wire types + `nexus-sync` / contracts carry behavior; document as **boundary choice** unless product mandates full domain parity.

#### Consistency Rules (data-model-v1.md §1.4, architecture v1.md §5)


| Rule                                                                           | Implementation                                                                                                               | Status      |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- | ----------- |
| Time-forward only: TimelineEvent with canon status cannot be modified in place | `timeline_event.rs`: `promote_to_canon()` requires `canon` → only from `provisional`; no `demote_from_canon` exists          | **Aligned** |
| World history immutable: changes through Fork, not in-place mutation           | `world.rs`: `fork()` creates child World + ForkBranch; no mutation of existing timeline                                      | **Aligned** |
| KeyBlock `provisional` → `confirmed` multi-gate validation                     | `key_block.rs`: `confirm()` has 5 gates (permission, version match, required fields, source anchor, no unresolved conflicts) | **Aligned** |
| SourceAnchor excerpt ≤ 1024 chars                                              | `source_anchor.rs`: `MAX_EXCERPT_LENGTH = 1024`, `validate_excerpt()` enforces                                               | **Aligned** |
| ManuscriptState linear progression                                             | `manuscript_state.rs`: `promote()` enforces brainstorm→draft→review→finalize→published; no backward transitions              | **Aligned** |
| ReferenceSource local-only                                                     | `reference_source.rs`: no sync fields; SQLite-only storage per `nexus-local-db` schema                                       | **Aligned** |
| Structured sync only, not full text                                            | `nexus-sync`: bundles carry deltas with `source_anchor`, not full manuscript text                                            | **Aligned** |


---

### 2.3 CLI/Daemon Responsibility Split


| CLI Responsibility (cli-spec §4) | CLI Implementation                                                         | Alignment                                                                               |
| -------------------------------- | -------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Workspace manager                | `init`, `clone` (skeleton), workspace detection                            | **Partial** — `clone` not fully implemented                                             |
| Sync engine                      | `sync push/pull/status/resolve` commands; calls daemon API                 | **Partial** — sync commands delegate to daemon; daemon returns 501 for context/assemble |
| Agent bridge (ACP)               | `acp/` module: Registry, Transport, Skills, Client, Policy, SessionManager | **Aligned** — comprehensive implementation                                              |
| Session guard                    | Auth store with dual-bucket tokens, device flow login, creator auth        | **Aligned**                                                                             |



| Daemon Responsibility (cli-spec §9-10)                            | Daemon Implementation                                                                             | Alignment                                                        |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| State machine (Stopped→Starting→Running→Degraded→Stopping→Failed) | Simplified: `nexus42d` starts, serves, graceful shutdown. No explicit state machine transitions   | **Gap** — no daemon lifecycle state machine                      |
| Workspace context                                                 | `WorkspaceState` with init, SQLite, pool                                                          | **Aligned**                                                      |
| ACP session management                                            | `acp_sessions` table, `list_sessions`, `delete_session` endpoints                                 | **Aligned**                                                      |
| Local API surface                                                 | 17 endpoints across health, auth, workspace, creators, manuscript, references, context, sync, ACP | **Partial** — several endpoints return 501 or skeleton responses |


**Command Alignment vs cli-spec §6:**


| Spec Command                                                                     | CLI Implementation                                                 | Status                                                                                             |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------- |
| `nexus42 version`                                                                | ✅ Global flag                                                      | **Aligned**                                                                                        |
| `nexus42 doctor`                                                                 | ⚠️ Not implemented as standalone command                           | **Gap**                                                                                            |
| `nexus42 completion`                                                             | ✅ Clap built-in                                                    | **Aligned**                                                                                        |
| `nexus42 auth login/logout/status/profiles`                                      | ✅ `auth login/token/logout/status`                                 | **Partial** — `profiles` not implemented; `login` uses daemon device flow (mock in V1.x)           |
| `nexus42 creator register/status/use/list/pair/unpair/logout/credentials rotate` | ✅ All subcommands present                                          | **Aligned**                                                                                        |
| `nexus42 init`                                                                   | ✅ Creates workspace dirs                                           | **Aligned**                                                                                        |
| `nexus42 clone <world-ref>`                                                      | ❌ Not implemented                                                  | **Gap**                                                                                            |
| `nexus42 link/unlink`                                                            | ❌ Not implemented                                                  | **Gap**                                                                                            |
| `nexus42 config get/set/unset/path`                                              | ❌ Not a separate command group; config is via file                 | **Gap**                                                                                            |
| `nexus42 sync push/pull/status/retry`                                            | ✅ `sync push/pull/status/resolve`                                  | **Partial** — `retry` → `resolve` with strategy; resolved via conflict handling                    |
| `nexus42 manuscript status/phase/output/promote/verify`                          | ✅ `manuscript create/edit/status/phase/promote/verify/export/list` | **Partial** — expanded beyond spec; `output` not a separate subcommand but controlled per-manifest |
| `nexus42 publish chapter/story`                                                  | ❌ Not implemented                                                  | **Gap** (V1.2 per spec)                                                                            |
| `nexus42 context assemble`                                                       | ✅ `context assemble` command + `ContextClient`                     | **Partial** — assembles locally from markdown; platform endpoint returns 501                       |
| `nexus42 research add/list/scan`                                                 | ✅ `research scan/list/extract`                                     | **Partial** — `add` → `scan` with path                                                             |
| `nexus42 daemon start/stop/restart/status/logs`                                  | ✅ `daemon start/stop/status`                                       | **Partial** — no `restart` or `logs` subcommands                                                   |
| `nexus42 acp status/doctor/probe` / `registry list/inspect/use`                  | ✅ `agent list/show/run/probe/skills/status`                        | **Aligned** — renamed from `acp` to `agent` with expanded subcommands                              |
| `nexus42 skills export/verify`                                                   | ✅ `agent skills` subcommand                                        | **Aligned**                                                                                        |
| `nexus42 debug dump-workspace/replay-delta`                                      | ❌ Not implemented                                                  | **Gap**                                                                                            |


**Key Finding**: CLI command coverage is approximately 65% functional. Critical gaps: `clone`, `link/unlink`, `config` command group, `publish`, and `debug`. The `agent` command group (renamed from `acp`) is well-implemented.

---

### 2.4 ACP Integration


| Spec Requirement (acp-client-tech-spec-legacy.md)         | Implementation                                                                                         | Alignment                                          |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------- |
| Use `agent-client-protocol` v0.10.4                   | `nexus42/Cargo.toml` pins `agent-client-protocol = "=0.10.4"`                                          | **Aligned**                                        |
| ACP Client-only (nexus42d is NOT an ACP Agent/Server) | Confirmed: daemon has no ACP server code; CLI spawns agent subprocesses directly                       | **Aligned**                                        |
| Registry manifest fetch + local cache                 | `acp/registry.rs`: `RegistryClient` with stale-while-revalidate, 24h cache, `$HOME/.nexus42/registry/` | **Aligned**                                        |
| Agent subprocess transport (stdio JSON-RPC)           | `acp/transport.rs`: `AgentSpawner` spawns `tokio::process::Command` with stdin/stdout pipes            | **Aligned**                                        |
| `NexusAcpClient` adapter trait over SDK               | `acp/client.rs`: `NexusAcpClient` trait + `AcpSDKAdapter` + `LocalSetBridge` for !Send futures         | **Aligned**                                        |
| Frozen capability IDs for V1.0                        | `acp/skills.rs`: 6 capabilities (`file_system.read/write`, `terminal.create/output/release`)           | **Aligned**                                        |
| `nexus42 agent list/show/run/probe`                   | `commands/agent.rs`: All four subcommands + `skills` and `status`                                      | **Aligned**                                        |
| Permission policy engine                              | `acp/policy.rs`: `PermissionPolicy` with grant/deny/ask, `.nexus42/permissions.toml`                   | **Aligned** (V1.0 auto-grant with policy override) |
| Session persistence                                   | `acp/session_manager.rs`: `$HOME/.nexus42/acp/sessions.json`                                           | **Aligned**                                        |


**ACP Finding**: The ACP integration is well-implemented and closely follows the tech spec. The only minor gap is that `nexus42 agent run` fully implements the interactive prompt loop (per spec Task 4), but the `LocalSetBridge` for !Send futures is a sophisticated workaround for the ACP SDK's `!Send` constraint.

---

### 2.5 Sync Contract Implementation


| sync-contract-v1.md Requirement         | nexus-sync Implementation                                                                                                                                                                                                                                                      | Alignment                                                      |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------- |
| DeltaBundle as sync unit                | `BundleBuilder` with all required fields: `bundle_id`, `command_id`, `workspace_id`, `world_id`, `creator_id`, `submitting_creator_id`, `bundle_type`, `manuscript_phase`, `output_manuscript`, `idempotency_key`, `canonical_hash`, `base_versions`, `deltas[]`, `created_at` | **Aligned**                                                    |
| Idempotency + canonical_hash            | `idempotency_key` auto-generated as `idk_{uuid}`; `canonical_hash` placeholder (empty string, "V1.0: real hash TBD")                                                                                                                                                           | **Partial** — canonical_hash not computed                      |
| `base_versions` for conflict detection  | `BundleBuilder` accepts `base_world_revision()`, `base_timeline_head_id()`, `base_canon_revision()`                                                                                                                                                                            | **Aligned**                                                    |
| `submitting_creator_id` required (V1.0) | `BundleBuilder::build()` fails if `submitting_creator_id` is not set                                                                                                                                                                                                           | **Aligned**                                                    |
| `manuscript_phase` in bundle metadata   | Optional builder method; included in wire format                                                                                                                                                                                                                               | **Aligned**                                                    |
| `output_manuscript` in bundle metadata  | Optional builder method; included in wire format                                                                                                                                                                                                                               | **Aligned**                                                    |
| Delta type and operation enums          | `DeltaType`: world, key_block, timeline_event, fork_branch, memory_item, story_manifest; `DeltaOperation`: create, update, upsert, delete, append                                                                                                                              | **Aligned**                                                    |
| Partial apply semantics (§7.4)          | `partial_apply.rs`: `PartialApplyResult` with `succeeded_deltas`/`failed_deltas`, `retryable` classification, `RETRYABLE_ERROR_CODES`                                                                                                                                          | **Aligned**                                                    |
| Conflict response parsing               | `conflict.rs`: `ConflictResponse` with `ConflictType`, `ConflictDetail`, resolution hints, `retry_after`                                                                                                                                                                       | **Aligned**                                                    |
| Outbox model (§9)                       | `outbox.rs`: Full SQLite-backed outbox with `DeliveryState` enum (staged→ready→sent→acked/conflicted/failed), retry with exponential backoff, `MAX_RETRIES = 5`                                                                                                                | **Aligned**                                                    |
| Local precheck (pre-send validation)    | `precheck.rs`: 7 check stages (required fields, ID prefixes, sequence monotonicity, world revision, command consistency, schema compliance, auth match)                                                                                                                        | **Aligned**                                                    |
| Sync client (HTTP to platform)          | `sync_client.rs`: POST `/v1/sync/push`, GET `/v1/sync/state/{world_id}`, retry logic, body size limit, 409 conflict handling                                                                                                                                                   | **Partial** — client exists but daemon-side handlers are stubs |


**Key Finding**: The `nexus-sync` crate is a **complete, well-structured sync implementation** that covers all major contract requirements. However, it is **not wired into the actual CLI/daemon sync flow**. The CLI's `sync push/pull` commands call the daemon's HTTP API (`DaemonClient`), which has only skeleton handlers. The `nexus-sync` crate's `SyncClient`, `Outbox`, and `Precheck` are not directly used by the CLI commands.

---

### 2.6 Technical Debt and Risks


| ID    | Severity     | Description                                                                                                                                                                                                                                                                                                   | Impact                                                                        | Evidence                                                                                                                                  |
| ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| TD-1  | **Critical** | **Outbox/Sync not wired**: `nexus-sync` crate has comprehensive outbox, precheck, partial apply, and conflict resolution logic, but the CLI `sync` commands call daemon HTTP endpoints that return stub data. The `nexus-sync::SyncClient`, `Outbox`, and `Precheck` are not invoked in the actual sync flow. | Users cannot actually push/pull structured deltas to/from the platform        | `commands/sync.rs` uses `DaemonClient`; daemon `handlers::sync::status` returns hardcoded data                                            |
| TD-2  | **Critical** | **WorldStatus enum mismatch**: Domain uses `Frozen` where spec §7 defines `Paused`. If the platform sends `paused`, the CLI won't recognize it.                                                                                                                                                               | Wire-format incompatibility with platform                                     | `nexus-domain/src/world.rs` line `WorldStatus::Frozen`                                                                                    |
| TD-3  | **Critical** | **MembershipStatus enum mismatch**: Domain uses `Left` where spec §7 defines `Removed`; missing `Invited` variant.                                                                                                                                                                                            | Wire-format incompatibility                                                   | `nexus-domain/src/world_membership.rs` line `MembershipStatus::Left`                                                                      |
| TD-4  | **High**     | **28+ String-typed enum fields in contracts**: Generated wire types use `String` for most enum fields, while domain layer has proper Rust enums. Validation relies on domain layer, but contracts don't enforce.                                                                                              | Potential for invalid enum values in wire format; no compile-time enforcement | `nexus-contracts/src/generated/*.rs` — all `status`, `role`, `type` fields are `String`                                                   |
| TD-5  | **High**     | **canonical_hash not computed**: `BundleBuilder::build()` generates empty string for `canonical_hash`. Spec requires idempotency via content-hash.                                                                                                                                                            | Duplicate detection won't work; retry safety compromised                      | `nexus-sync/src/delta_bundle.rs` line `canonical_hash: String::new()`                                                                     |
| TD-6  | **High**     | **User aggregate missing**: No `User` struct in domain or contracts. CLI auth handles user tokens but has no domain model for User.                                                                                                                                                                           | Cannot represent or validate user identity in sync flows                      | Absent from `nexus-domain/src/` and `schemas/`                                                                                            |
| TD-7  | **Medium**   | **ForkBranch field naming**: Spec uses `parent_branch_id` and `forked_from_event_id`; contracts use only `forked_from_event_id` (no `parent_branch_id` field at all). Contract adds `parent_branch_id` matching spec but field naming differs slightly from domain model                                      | Minor wire-format inconsistency                                               | `nexus-contracts/src/generated/fork_branch.rs` has both fields matching spec, `nexus-domain/src/fork_branch.rs` also matches              |
| TD-8  | **Medium**   | **nexus-sync crate not integrated with daemon**: The daemon has its own `outbox` table in `nexus-local-db`, while `nexus-sync` has its own `outbox_entries` table. Dual outbox implementations.                                                                                                               | Data inconsistency risk; duplicated schema management                         | `nexus-local-db/src/schema.rs` (daemon `outbox` table) vs `nexus-sync/src/outbox.rs` (separate `outbox_entries` table with richer schema) |
| TD-9  | **Medium**   | **Daemon state machine not implemented**: Spec §10.1 defines 6 states (Stopped, Starting, Running, Degraded, Stopping, Failed); implementation has no state machine.                                                                                                                                          | No graceful degradation or status tracking                                    | `nexus42d/src/main.rs` — simple start-and-serve, no lifecycle states                                                                      |
| TD-10 | **Medium**   | **Auth flow is mock/placeholder**: Device flow endpoints generate mock tokens (`at_<uuid>`, `usr_mock_<uuid>`). No real OAuth flow.                                                                                                                                                                           | Non-functional authentication in production                                   | `nexus42d/src/auth/device_flow.rs` line `verify_device_code` returns `Ok(false)`                                                          |
| TD-11 | **Low**      | **Several CLI commands missing or skeleton**: `clone`, `link/unlink`, `config` group, `publish`, `debug dump-workspace/replay-delta`, `doctor`                                                                                                                                                                | Incomplete user-facing CLI                                                    | Missing from `commands/` module                                                                                                           |
| TD-12 | **Low**      | **ForkBranch spec uses `parent_branch_id`**: Contracts have `parent_branch_id`. Domain model also has it. The naming is consistent between domain and contracts. However, the spec field `forked_from_event_id` maps to `parent_event_id` semantics.                                                          | No actual issue — naming is consistent                                        | Verified: all three sources agree                                                                                                         |
| TD-13 | **Low**      | **Test coverage**: No `cargo test` results available for analysis. Domain logic has unit tests (verified in `#[cfg(test)]` modules), but integration tests for sync, CLI commands, and daemon handlers appear incomplete.                                                                                     | Risk of regression bugs                                                       | `nexus42/tests/` and `nexus42d/tests/` directories exist but coverage analysis not in scope                                               |

**Note**: Row text above is the **2026-04-08 baseline**. For current done/partial/open, use **§1.2** and `status.json`.

---

## 3. Priority Roadmap

### V1.0 GA (Critical + High) — historical checklist

**Status**: Executed under `2026-04-09-v1.1-arch-alignment-blockers` (Done). Remaining follow-ups: **ALIGN-HASH-01**, **ARCH-SYNC-D1** — see plan [`2026-04-09-v1.1-arch-alignment-closure.md`](../2026-04-09-v1.1-arch-alignment-closure.md).


| Priority | ID   | Action                                                                                                                             | Effort |
| -------- | ---- | ---------------------------------------------------------------------------------------------------------------------------------- | ------ |
| **P0**   | TD-1 | Wire `nexus-sync` crate into CLI sync flow: replace daemon HTTP stubs with actual `SyncClient` + `Outbox` + `Precheck` invocations | L      |
| **P0**   | TD-2 | Fix `WorldStatus` enum: rename `Frozen` → `Paused` to match spec §7                                                                | XS     |
| **P0**   | TD-3 | Fix `MembershipStatus` enum: rename `Left` → `Removed`, add `Invited` variant                                                      | XS     |
| **P1**   | TD-4 | Add proper enum types to JSON Schema and regenerate contracts (or add validation layer in domain)                                  | M      |
| **P1**   | TD-5 | Implement canonical_hash computation (SHA-256 of delta payload)                                                                    | S      |
| **P1**   | TD-6 | Add `User` aggregate to both domain and contracts (needed for auth/sync flows)                                                     | S      |


### V1.1 (Medium)


| Priority | ID    | Action                                                                                                     | Effort |
| -------- | ----- | ---------------------------------------------------------------------------------------------------------- | ------ |
| **P2**   | TD-8  | Consolidate dual outbox: migrate daemon's `outbox` table to use or delegate to `nexus-sync::Outbox` schema | M      |
| **P2**   | TD-9  | **Done (WS4)** — Implement daemon lifecycle state machine — see daemon-lifecycle-api.md               | —      |
| **P2**   | TD-10 | Implement real device flow OAuth (replace mock tokens with platform auth endpoints)                        | M      |
| **P2**   | TD-7  | Verify ForkBranch field naming consistency across all three layers (spec, domain, contracts)               | XS     |


### V1.2+ (Low)


| Priority | ID    | Action                                                                                        | Effort |
| -------- | ----- | --------------------------------------------------------------------------------------------- | ------ |
| **P3**   | TD-11 | Complete missing CLI commands: `clone`, `link/unlink`, `config`, `publish`, `debug`, `doctor` | L      |
| **P3**   | TD-13 | Improve integration test coverage for sync, CLI, and daemon handlers                          | L      |


---

## 4. Appendix: Detailed Enum Alignment Table

See §2.2 above for the complete 40-row enum alignment table covering all §7 enums with spec values vs implementation values vs contract types.

**Summary statistics (baseline 2026-04-08 — re-verify after TD-4 schema work):**

- **Total enum fields in spec §7**: 40
- **Fully aligned (correct values + proper enum type)**: 6 (15%) — `World.visibility`, `World.time_policy`, `KeyBlock.block_type`, `MemoryItem.memory_type`, `ManuscriptState.manuscript_phase`, `DeltaBundle.bundle_type`
- **Value-aligned but String-typed in contracts**: 25 (63%) — correct values but no compile-time enforcement
- **Value-mismatched**: 2 (5%) — `World.status`, `WorldMembership.membership_status`
- **Not yet applicable (no domain/contract)**: 7 (17%) — User-subscription_tier, SyncCommand-status, WorkspaceBinding-binding_status, AgentProfile fields

After TD-2/TD-3/TD-4 delivery, regenerate or spot-check codegen and update this appendix if the team needs an updated percentage rollup.

---

## 5. Related Documents

### V1.1 Development Overview

The technical debt items identified in this review are tracked and addressed in:

- **[v1.1-overview-v2.md](../../iterations/v1.1-overview-v2.md)** — Authoritative V1.1 program overview (status-aligned); residual tables and platform coordination narrative. Deep historical matrices: [program-overview-legacy.md](program-overview-legacy.md).
- **[2026-04-09-v1.1-arch-alignment-closure.md](../2026-04-09-v1.1-arch-alignment-closure.md)** — Closes remaining **ALIGN-HASH-01** and **ARCH-SYNC-D1**; after Done, update **§1.2** in this file

### Knowledge Base Index

- **[README.md](README.md)** — Knowledge base index listing all dev-process documents with source plans and status

---

*End of Architecture Alignment Review*
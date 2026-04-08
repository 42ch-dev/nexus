# V1.0-phase1 Architecture Review: CLI-Daemon, Sync-Contract, ACP-Client

**Reviewer**: @architect  
**Date**: 2026-04-06  
**Scope**: Three V1.0-phase1 implementation plans (`cli-daemon-foundation`, `sync-contract`, `acp-client`)  
**Cross-referenced with**:
- `open-source-and-repo-architecture.md` (repo boundary & open-source split)
- `nexus-platform-monorepo-v1.md` (platform monorepo layout)
- `roadmap.md` (V1.0 deliverables & frozen constraints)
- `restructured-context-assembly-v1.md` (CLI-side context assembly spec)
- `status.json` residual findings (CLI-R1..R4, SYNC-R1..R3, ACP-R1..R2, CTX-R1)

---

## 1. Executive Summary

| Plan | Verdict | Key Action Before Implementation |
|------|---------|--------------------------------|
| `cli-daemon-foundation` | **Request Changes** | 4 high-severity residual findings must be resolved (CLI-R1..R4) |
| `sync-contract` | **Request Changes** | 1 high (SYNC-R1) must resolve; 2 medium (SYNC-R2, R3) should resolve |
| `acp-client` | **Approve with Residuals** | Medium + Low findings (ACP-R1, R2) can be deferred to implementation start |
| `context-assembly` | **Blocked** (not in scope for this review cycle) | CTX-R1: plan file must be rewritten from restructured spec |

**Cross-plan dependency order recommended**: `cli-daemon-foundation` → `sync-contract` → `acp-client` → `context-assembly`

---

## 2. Methodology

This review applies the following checks against each plan:

1. **Frozen spec alignment**: Does the plan respect constraints from `v1-spec` (via roadmap §3.1)?
2. **Open-source boundary compliance**: Does the plan stay within the `nexus` public repo boundary (per `open-source-and-repo-architecture.md` §2.1/§2.2)?
3. **Residual findings disposition**: For each deferred finding, should it be resolved before implementation?
4. **Cross-plan dependency correctness**: Are the stated dependencies between plans accurate and complete?
5. **Tech stack consistency**: Rust-first for CLI/daemon, no forbidden dependencies (Neo4j/Postgres/pgvector on CLI side).
6. **Schema/codegen alignment**: Does the plan reference the JSON Schema truth source correctly?

---

## 3. Plan-by-Plan Review

### 3.1 CLI + Daemon Foundation (`2025-04-05-cli-daemon-foundation`)

#### 3.1.1 Alignment with Product Architecture

| Aspect | Assessment | Evidence |
|--------|-----------|----------|
| Open-source boundary | ✅ Aligned | CLI + daemon in `crates/`; no platform-side code — per `open-source-and-repo-architecture.md` §2.1 |
| Tech stack | ✅ Aligned | Rust 1.75+, clap, tokio — matches AGENTS.md "Rust-first" constraint |
| JSON Schema truth source | ✅ Aligned | CLI/daemon consume from `crates/nexus-contracts` (generated from `schemas/`) — per `open-source-and-repo-architecture.md` §4 |
| Workspace layout | ✅ Aligned | `Stories/`, `References/`, `.nexus42/` — matches `cli-spec-v1.md` §13 referenced in roadmap §3.1.1 |
| V1.0 scope | ⚠️ Incomplete | Roadmap §3.1.1 requires "Creator/subscription tier", "manuscript_phase", "ReferenceSource/research" — plan only covers `init`, `auth`, `sync`, `daemon` |

#### 3.1.2 Residual Findings Disposition

| ID | Finding | Severity | Current Decision | Review Recommendation | Must Resolve Before Implementation? |
|----|---------|----------|-----------------|----------------------|-------------------------------------|
| CLI-R1 | Missing Creator command surface | **High** | defer | **Must add to plan** | **Yes** — Creator is V1.0 first-class citizen (roadmap §2.2, §3.1.1). Missing `creator` subcommand means no registration, pairing, or credential management — blocks all sync flows that require `submitting_creator_id` |
| CLI-R2 | Missing Manuscript command surface | **High** | defer | **Must add to plan** | **Yes** — `manuscript_phase` and promote workflow are V1.0 deliverables (roadmap §2.2 row "Brainstorm-Write-Review", §3.1.1 "manuscript_phase / ManuscriptState / promote"). CLI must expose `manuscript status`, `manuscript phase`, `manuscript output` |
| CLI-R3 | Missing Research command surface | **Medium** | defer | Should add to plan | **Recommended** — `ReferenceSource / research` is V1.0 minimal scope (roadmap §3.1.1). At minimum `nexus42 research scan` and `nexus42 research list` are needed for local-only research workflow |
| CLI-R4 | Auth model covers only single-user tokens, missing dual-subject auth | **High** | defer | **Must redesign** | **Yes** — Per roadmap §2.2, "Creator first-class citizen / Pairing" is V1.0 frozen. The auth module must support **both** User tokens (human login) and Creator API keys (machine auth). Creator API keys are stored in platform secure storage per `service-boundaries-v1.md`. The plan's single `$HOME/.nexus42/auth.json` model is insufficient |

#### 3.1.3 Plan Expansion Recommendations

The plan's current task list (Tasks 1–5) covers only the skeleton. The following must be added:

**New Task A: Creator Command Module**
- `crates/nexus42/src/commands/creator.rs`
- Subcommands: `creator register`, `creator status`, `creator use <creator-ref>`, `creator list`, `creator pair`, `creator unpair`, `creator credentials rotate`
- Depends on: auth module (Task 3 redesigned with dual-subject support)

**New Task B: Manuscript Command Module**
- `crates/nexus42/src/commands/manuscript.rs`
- Subcommands: `manuscript status`, `manuscript phase <phase>`, `manuscript output`, `manuscript promote`, `manuscript verify`
- Reads `manuscript_phase` from workspace state; pushes phase transitions via sync

**New Task C: Research Command Module (V1.0 minimal)**
- `crates/nexus42/src/commands/research.rs`
- Subcommands: `research scan`, `research list`, `research extract`
- Local-only for V1.0; references `References/<creator_ref>/` layout

**Auth Module Redesign (Task 3 replacement)**:
- Split into: `user_auth.rs` (device flow OAuth for human users) and `creator_auth.rs` (API key management for Creator entities)
- User token: `$HOME/.nexus42/auth.json` (current)
- Creator API keys: stored in platform secure storage, CLI obtains via `POST /v1/creators/{id}/credentials` — CLI only caches short-lived tokens locally

#### 3.1.4 Architecture Decision: Local API Transport

The plan mentions "HTTP/gRPC" for the Local API between CLI and daemon. **Recommendation**: use **HTTP (JSON over TCP loopback)** for V1.0. Rationale:
- gRPC adds Protobuf dependency which conflicts with JSON Schema truth source (`open-source-and-repo-architecture.md` §7)
- HTTP is simpler for the V1.0 skeleton; gRPC can be added later via ADR if performance demands it
- The Local API contract (`local-runtime-boundary-v1.md`) defines JSON wire shapes

#### 3.1.5 Verdict: **Request Changes**

4 high-severity findings (CLI-R1, R2, R3 as recommended, R4) must be resolved by plan expansion before implementation can begin.

---

### 3.2 Sync Contract (`2025-04-05-sync-contract`)

#### 3.2.1 Alignment with Product Architecture

| Aspect | Assessment | Evidence |
|--------|-----------|----------|
| Open-source boundary | ✅ Aligned | Sync crate is pure library in `crates/nexus-sync/` — per `open-source-and-repo-architecture.md` §4 |
| Tech stack | ✅ Aligned | Rust, serde, tokio, reqwest — no platform-side dependencies |
| Wire contract | ⚠️ Partially aligned | Plan references Command/DeltaBundle/Outbox but doesn't explicitly anchor to `bundle.schema.json` and `sync-contract-v1.md` |
| Conflict resolution | ⚠️ Basic | Plan mentions "optimistic locking with `world_revision`" but missing `partial apply` semantics (SYNC-R2) |
| V1.0 scope | ⚠️ Missing fields | `manuscript_phase`, `output_manuscript`, `submitting_creator_id` not in plan (SYNC-R1) — these are V1.0 deliverables per roadmap §3.1.1 |

#### 3.2.2 Residual Findings Disposition

| ID | Finding | Severity | Current Decision | Review Recommendation | Must Resolve Before Implementation? |
|----|---------|----------|-----------------|----------------------|-------------------------------------|
| SYNC-R1 | Missing bundle metadata fields | **High** | defer | **Must add to plan** | **Yes** — `submitting_creator_id`, `manuscript_phase`, `output_manuscript` are V1.0 frozen deliverables (roadmap §3.1.1, §3.1.2). These fields are also **prerequisites** for context-assembly (CTX dependency in `restructured-context-assembly-v1.md` §6). Without SYNC-R1, `context-assembly` cannot be unblocked |
| SYNC-R2 | Missing partial apply semantics | **Medium** | defer | **Should add to plan** | **Recommended** — Roadmap §3.1.4 (P1) explicitly calls for `bundle_apply_status=partial` or equivalent. Phase A/B pipeline requires distinguishing "A succeeded, B failed" from total failure. CLI must be able to handle partial responses |
| SYNC-R3 | Missing local precheck (Stage 0) | **Medium** | defer | **Should add to plan** | **Recommended** — Local precheck prevents pushing invalid bundles to platform. This is a sync client quality improvement: validate command consistency, schema compliance, and sequencing locally before HTTP upload |

#### 3.2.3 Schema Anchoring Gap

The plan's Task 1 says "Define Command types" but doesn't explicitly reference `schemas/cli-sync/bundle.schema.json` or `schemas/common/`. **Recommendation**: Task 1 must:
1. Start from existing `schemas/` as truth source (check `schemas/cli-sync/` for bundle envelope and delta type definitions)
2. Generate Rust types via codegen pipeline (already done in Phase 0 codegen-pipeline plan)
3. NOT hand-write duplicate types — use `crates/nexus-contracts` generated types
4. Add missing bundle-level fields (`submitting_creator_id`, `manuscript_phase`, `output_manuscript`) to `schemas/cli-sync/bundle.schema.json` first, then regenerate

#### 3.2.4 Outbox Persistence

The plan proposes `$HOME/.nexus42/outbox.json` for outbox persistence. **Concern**: JSON file for a queue is fragile (partial writes on crash, no atomicity). **Recommendation**: use **SQLite** for outbox persistence, which:
- Provides atomic transactions
- Aligns with CLI using SQLite for local structured state (per `restructured-context-assembly-v1.md` §2.3)
- Can share the same SQLite database as workspace state
- JSON file is acceptable for V1.0 only if a migration note is added

#### 3.2.5 Conflict Response Shape

The plan's Task 5 mentions "conflict detection" but doesn't align with the frozen conflict response shape. Roadmap §3.1.4 (P1) mandates `200 + success: false + conflict body` (per `hard-vs-soft-validation-v1.md` §7). **Recommendation**: ensure the sync client's conflict response parsing matches `SyncConflictResponseV1` from `schemas/cli-sync/conflict-response.schema.json` (or equivalent).

#### 3.2.6 Verdict: **Request Changes**

SYNC-R1 (high) must be resolved before implementation. SYNC-R2 and SYNC-R3 (medium) are strongly recommended to resolve. Plan must also explicitly anchor to JSON Schema truth source.

---

### 3.3 ACP Client (`2025-04-05-acp-client`)

#### 3.3.1 Alignment with Product Architecture

| Aspect | Assessment | Evidence |
|--------|-----------|----------|
| Open-source boundary | ✅ Aligned | ACP is client-only in `nexus` public repo — per `open-source-and-repo-architecture.md` §2.1 "ACP client-only adapter" |
| Tech stack | ✅ Aligned | Uses ACP Rust SDK (official) — per AGENTS.md "Use official ACP Rust SDK" |
| Protocol constraint | ✅ Aligned | "CLI acts as ACP client (not agent/server)" — matches AGENTS.md constraint "Do not treat nexus42d as an ACP Agent/Server" |
| Registry | ✅ Aligned | Public CDN URL — per AGENTS.md "ACP Registry is public" |
| Local API | ✅ Aligned | Defined as minimum contract — per `local-runtime-boundary-v1.md` |

#### 3.3.2 Residual Findings Disposition

| ID | Finding | Severity | Current Decision | Review Recommendation | Must Resolve Before Implementation? |
|----|---------|----------|-----------------|----------------------|-------------------------------------|
| ACP-R1 | Missing frozen capability ID contract reference | **Medium** | defer | Can defer to implementation start | **No** (implementation-start gate) — The plan references `acp-capability-set-v1.md` in Task 4 which is sufficient at plan level. During implementation, the actual capability IDs from the frozen spec must be wired into the skills manifest |
| ACP-R2 | Missing `nexus42 acp probe` command | **Low** | defer | Can defer to implementation start | **No** (implementation-start gate) — A diagnostic command is useful but not blocking. Can be added during implementation as a quality-of-life feature. Aligns with roadmap §3.1.4 "CLI Agent diagnostics: `nexus42 doctor --agent`" |

#### 3.3.3 Plan Completeness

The plan is relatively complete for its scope. Minor observations:

1. **Task 3 (Local API Contract)**: The plan says "Define minimum Local API endpoints (per frozen Local API contract)" but doesn't list the specific endpoints. The Local API contract should include at minimum:
   - `POST /v1/local/context/assemble` (per `restructured-context-assembly-v1.md` §3.2)
   - `POST /v1/local/sync/push` (bundle upload via daemon proxy)
   - `GET /v1/local/status` (daemon health + workspace state)
   
   However, since the plan is V1.0 skeleton-level, listing "minimum contract" without exhaustively enumerating is acceptable — the specific endpoints can be refined during implementation.

2. **Task 5 (Agent CLI Commands)**: The commands `agent list`, `agent install`, `agent run` align with the ACP client-only role. No concerns.

3. **Schema registration**: The plan creates `schemas/acp-runtime/local-api-v1.schema.json`. Check that `schemas/acp-runtime/` already exists in the monorepo.

#### 3.3.4 Dependency on CLI-Daemon

The plan correctly states "Base: main (after cli-daemon complete)". This is correct because:
- ACP module lives in `crates/nexus42/src/acp/` — requires CLI crate to exist
- Local API server lives in daemon — requires `crates/nexus42d` skeleton
- Registry cache goes to `$HOME/.nexus42/registry/` — requires workspace layout from CLI plan

#### 3.3.5 Verdict: **Approve with Residuals**

The plan is architecturally sound. ACP-R1 and ACP-R2 can be addressed at implementation start without blocking plan approval.

---

### 3.4 Context Assembly (Contextual — Not Primary Review Target)

The context-assembly plan (`2025-04-05-context-assembly`) is **Blocked** in `status.json`. The restructured spec (`restructured-context-assembly-v1.md`) has already resolved the critical issues (5 conflicts with frozen specs). Key observations:

| Aspect | Status |
|--------|--------|
| Original plan file | ❌ Still contains incorrect scope (CTX-R1) — must be rewritten from restructured spec |
| Restructured spec quality | ✅ Excellent — clearly separates CLI vs platform, no forbidden dependencies |
| Dependency on sync-contract | ⚠️ Hard dependency on SYNC-R1 resolution (bundle metadata fields) |
| Dependency on CLI commands | ⚠️ Requires `nexus42 context assemble` command (not yet in CLI plan — see CLI-R2 gap) |

**Recommendation**: After `sync-contract` plan resolves SYNC-R1 and `cli-daemon-foundation` plan expands to include context-related commands, rewrite the context-assembly plan file from `restructured-context-assembly-v1.md`.

---

## 4. Cross-Plan Dependency Analysis

### 4.1 Dependency Graph

```text
cli-daemon-foundation ─────────┐
  │                            │
  │  (provides CLI crate +     │
  │   daemon skeleton +        │
  │   workspace layout +       │
  │   auth module)             │
  ▼                            │
sync-contract ─────────────────┤
  │                            │
  │  (provides bundle envelope │
  │   with metadata fields +   │
  │   outbox + conflict resp)  │
  │                            │
  ▼                            │
acp-client ────────────────────┤
  │                            │
  │  (provides ACP integration │
  │   + Local API server +     │
  │   registry)                │
  │                            │
  ▼                            │
context-assembly ──────────────┘
  │
  │  (CLI-side: summary gen +
  │   bundle metadata wiring +
  │   context assemble command)
  └── Depends on: sync-contract (bundle metadata),
                   cli-daemon (CLI commands),
                   acp-client (Local API proxy)
```

### 4.2 Recommended Implementation Order

| Phase | Plan | Rationale | Prerequisite |
|-------|------|-----------|-------------|
| **1a** | `cli-daemon-foundation` (expanded) | Foundation: CLI crate, daemon skeleton, workspace layout, auth module, command surface | Phase 0 complete (done) |
| **1b** | `sync-contract` (expanded) | Sync library: can develop independently but needs CLI workspace for integration tests | Phase 0 complete; CLI-R4 auth redesign informs `submitting_creator_id` |
| **2** | `acp-client` | Depends on CLI crate existing + daemon Local API endpoint | 1a complete |
| **3** | `context-assembly` (rewrite from restructured spec) | Hard dependency on sync-contract bundle metadata + CLI commands | 1a + 1b + 2 complete |

**Note on parallelism**: `sync-contract` (1b) can technically proceed in parallel with `cli-daemon-foundation` (1a) since `crates/nexus-sync/` is a pure library crate. However, integration testing of the sync client requires the daemon to be running, so sequential is safer. If parallel execution is desired, the dependency boundary should be: sync-contract Tasks 1–3 (library-only) can run in parallel with cli-daemon Tasks 1–4; sync-contract Tasks 4–5 (client + conflict) require daemon.

### 4.3 Dependency Table for Residual Findings

| Finding | Blocks | Unblock Action |
|---------|--------|----------------|
| **CLI-R1** (Creator commands) | ACP client `agent run <creator>`, sync `submitting_creator_id` | Add Creator command module to CLI plan |
| **CLI-R2** (Manuscript commands) | Context assembly `context assemble` command | Add Manuscript command module to CLI plan |
| **CLI-R4** (Dual-subject auth) | Sync client (needs Creator API keys for `submitting_creator_id`) | Redesign auth module |
| **SYNC-R1** (Bundle metadata fields) | Context assembly (story_manifest delta + summary payload) | Add fields to `bundle.schema.json` + sync implementation |
| **SYNC-R2** (Partial apply) | Robust conflict handling | Add partial apply to sync client design |
| **SYNC-R3** (Local precheck) | Sync quality (non-blocking) | Add precheck stage to sync pipeline |
| **CTX-R1** (Plan rewrite) | Context assembly implementation | Rewrite from restructured spec after 1a+1b+2 |

---

## 5. Frozen Spec Compliance Summary

### 5.1 Constraints Checklist

| Frozen Constraint | Source | CLI-Daemon | Sync-Contract | ACP-Client |
|-------------------|--------|------------|---------------|------------|
| Rust-first for CLI/daemon | AGENTS.md | ✅ | ✅ | ✅ |
| JSON Schema as wire truth source | `codegen-strategy-v1.md` | ✅ | ⚠️ (needs explicit anchor) | ✅ |
| CLI is ACP client, not agent/server | AGENTS.md | ✅ | N/A | ✅ |
| CLI uses SQLite for local state | `restructured-context-assembly-v1.md` §2.3 | ⚠️ (outbox uses JSON file) | ⚠️ (outbox.json) | N/A |
| No Neo4j/Postgres/pgvector on CLI side | `restructured-context-assembly-v1.md` §2.3 | ✅ | ✅ | ✅ |
| `@42ch/nexus-contracts` for wire types | `open-source-and-repo-architecture.md` §5 | ✅ | ⚠️ (should use generated) | ✅ |
| V1.0 Creator as first-class citizen | roadmap §3.1.1, §3.1.2 | ❌ (missing) | ⚠️ (missing `submitting_creator_id`) | N/A |
| `manuscript_phase` V1.0 deliverable | roadmap §3.1.1 | ❌ (missing commands) | ❌ (missing in bundle) | N/A |
| Phase A/B submit strategy | roadmap §3.1.3 | N/A | ⚠️ (missing partial apply) | N/A |

### 5.2 Deviations Summary

**Critical deviations** (must fix before implementation):
1. CLI plan missing Creator command surface (CLI-R1) — violates V1.0 "Creator first-class citizen"
2. CLI plan missing Manuscript command surface (CLI-R2) — violates V1.0 "manuscript_phase" deliverable
3. CLI auth model missing dual-subject auth (CLI-R4) — violates V1.0 "Creator independent registration / Pairing"
4. Sync plan missing bundle metadata fields (SYNC-R1) — violates V1.0 `manuscript_phase` + `submitting_creator_id` and blocks context-assembly

**Non-critical but recommended**:
5. Sync outbox should use SQLite instead of JSON file
6. Sync plan should explicitly anchor to generated types from codegen
7. CLI plan should use HTTP (not "HTTP/gRPC") for Local API

---

## 6. Plan Update Recommendations

### 6.1 For `cli-daemon-foundation`

**Mandatory changes** (before implementation):
1. Add Task A: Creator command module (`creator.rs`) with subcommands: register, status, use, list, pair, unpair, credentials rotate
2. Add Task B: Manuscript command module (`manuscript.rs`) with subcommands: status, phase, output, promote, verify
3. Redesign Task 3 (Auth): split into user auth (device flow) and Creator auth (API key management via platform)
4. Add Task C: Research command module (V1.0 minimal) — `research scan`, `research list`, `research extract`

**Recommended changes**:
5. Clarify Local API transport as HTTP-only (not HTTP/gRPC)
6. Add workspace state storage specification (SQLite path: `$HOME/.nexus42/state.db`)
7. Add integration test skeleton for CLI ↔ daemon communication

### 6.2 For `sync-contract`

**Mandatory changes** (before implementation):
1. Add `submitting_creator_id`, `manuscript_phase`, `output_manuscript` to bundle envelope (schema first, then generated types)
2. Anchor Task 1 to `schemas/cli-sync/bundle.schema.json` and use codegen-generated types (not hand-written)
3. Add `story_manifest` delta type support (required for context-assembly summary payload)

**Recommended changes**:
4. Add partial apply semantics: sync client must handle `bundle_apply_status: "partial"` response
5. Add local precheck stage (Stage 0): validate bundle consistency, schema compliance, sequencing before HTTP upload
6. Consider SQLite for outbox persistence instead of JSON file
7. Add conflict response parsing aligned with `SyncConflictResponseV1`

### 6.3 For `acp-client`

**No mandatory changes** — plan is approved with residuals.

**At implementation start**:
1. Wire frozen capability IDs from `acp-capability-set-v1.md` into skills manifest (ACP-R1)
2. Add `nexus42 acp probe` diagnostic command (ACP-R2)
3. List specific Local API endpoints when implementing Task 3

### 6.4 For `context-assembly`

**Blocked** — rewrite plan file from `restructured-context-assembly-v1.md` after:
- CLI plan includes `nexus42 context assemble` command
- Sync plan includes `story_manifest` delta + bundle metadata fields
- ACP plan includes Local API proxy for `POST /v1/local/context/assemble`

---

## 7. Residual Findings: Consolidated Disposition

| Finding | Plan | Severity | Current Decision | New Decision | Action |
|---------|------|----------|-----------------|-------------|--------|
| CLI-R1 | cli-daemon | High | defer → **upgrade to resolve** | **resolve** | Add Creator command module to plan |
| CLI-R2 | cli-daemon | High | defer → **upgrade to resolve** | **resolve** | Add Manuscript command module to plan |
| CLI-R3 | cli-daemon | Medium | defer → **upgrade to resolve** | **resolve** | Add Research command module (V1.0 minimal) to plan |
| CLI-R4 | cli-daemon | High | defer → **upgrade to resolve** | **resolve** | Redesign auth module with dual-subject support |
| SYNC-R1 | sync-contract | High | defer → **upgrade to resolve** | **resolve** | Add bundle metadata fields to schema + implementation |
| SYNC-R2 | sync-contract | Medium | defer → **upgrade to resolve** | **resolve** | Add partial apply semantics to sync client design |
| SYNC-R3 | sync-contract | Medium | defer → **upgrade to resolve** | **resolve** | Add local precheck stage to sync pipeline |
| ACP-R1 | acp-client | Medium | defer | **defer** (implementation-start gate) | Wire frozen capability IDs during implementation |
| ACP-R2 | acp-client | Low | defer | **defer** (implementation-start gate) | Add probe command during implementation |
| CTX-R1 | context-assembly | High | defer | **defer** (blocked on 1a+1b+2) | Rewrite plan from restructured spec |

---

## 8. Risk Assessment

| Risk | Impact | Likelihood | Mitigation |
|------|--------|-----------|------------|
| CLI-R4 auth redesign delays cli-daemon plan | Medium | High | Auth redesign is scoped: split existing module, add Creator API key flow. Does not require external infrastructure changes (Creator API keys managed by platform) |
| SYNC-R1 schema changes ripple to context-assembly | Low | Certain (by design) | Expected dependency — context-assembly is already blocked. Resolving SYNC-R1 earlier unblocks the critical path |
| SQLite outbox migration from JSON | Low | Medium | If outbox starts as JSON file, add migration note. V1.0 outbox volume is low — JSON fragility is acceptable short-term |
| ACP SDK API stability | Medium | Low | Official ACP SDK is referenced; version pin in Cargo.toml. If SDK API changes, adapter layer in `crates/nexus42/src/acp/` isolates impact |

---

## 9. Conclusion

### Per-Plan Verdicts

| Plan | Verdict | Blocking Items |
|------|---------|----------------|
| `cli-daemon-foundation` | **Request Changes** | CLI-R1, CLI-R2, CLI-R4 (must resolve); CLI-R3 (recommended) |
| `sync-contract` | **Request Changes** | SYNC-R1 (must resolve); SYNC-R2, SYNC-R3 (recommended) |
| `acp-client` | **Approve with Residuals** | ACP-R1, ACP-R2 (defer to implementation start) |
| `context-assembly` | **Blocked** | CTX-R1 + dependency on all three above plans |

### Implementation Priority

```
Phase 1a: cli-daemon-foundation (expanded with CLI-R1..R4 resolutions)
Phase 1b: sync-contract (expanded with SYNC-R1..R3 resolutions)  [can partially parallel with 1a]
V1.0-phase2:  acp-client (as-is, address ACP-R1/R2 at start)
Phase 3:  context-assembly (rewrite from restructured spec)
```

### Next Steps

1. **@project-manager**: Update plan files per §6 recommendations, or assign plan expansion to @architect
2. **@fullstack-dev**: After plan updates, begin implementation in dependency order
3. **@project-manager**: Update `status.json` residual findings per §7 consolidated disposition
4. **@project-manager**: After plan updates, re-run this review or delegate QC to confirm alignment

---

*Review completed based on: 3 plan files, 1 knowledge document, status.json residual findings, 3 product architecture documents, and AGENTS.md constraints. All findings are traceable to specific spec sections or plan gaps.*

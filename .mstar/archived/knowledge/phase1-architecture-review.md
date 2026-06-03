# V1.0-phase1 Architecture Review — V1.0 GA

**Date**: 2026-04-06
**Reviewer**: @architect
**Scope**: All Phase 0 + V1.0-phase1 crates and schemas
**Review type**: READ-ONLY analysis — no source code modifications
**Status**: Archived — key findings summarized in `program-overview-legacy.md` §2.3; retained for detailed reference

---

## Executive Summary

The Nexus42 V1.0 architecture is **structurally sound at the crate boundary level** — the five-crate workspace (contracts, domain, sync, CLI, daemon) follows clean separation of concerns with a unidirectional dependency flow. The domain model is the strongest component: 15 well-designed aggregates with rigorous consistency rules, 133 tests, and proper provisional-to-canon lifecycle gates. The sync mechanism is well-architected with a clean Command → DeltaBundle → Outbox → ConflictResolution pipeline and solid precheck validation.

However, the review identifies **3 critical runtime bugs**, **7 high-severity design issues**, and **14 medium-severity concerns** that should be addressed before or during Phase 2. The most impactful issues are: (1) the ACP SDK integration is structurally complete but **entirely non-functional** — all adapter methods return placeholder responses; (2) the daemon has a **workspace initialization bug** where `is_initialized()` always returns `false`; and (3) the CLI context assembly client calls a daemon route that **does not exist**. Cross-cutting debt (error propagation inconsistency, SQLite schema duplication, connection-per-request) is accurately scoped in the 4 DEBT-X items but should be prioritized early in V1.0-phase2 to prevent accumulation.

---

## 1. Crate Architecture

### 1.1 Workspace Layout

```
Cargo.toml (workspace root)
├── crates/nexus-contracts    [serde, serde_json, thiserror]
├── crates/nexus-domain       [nexus-contracts, serde, chrono, uuid]
├── crates/nexus-sync         [nexus-contracts, tokio, reqwest, rusqlite, tracing]
├── crates/nexus42            [nexus-contracts, nexus-domain, agent-client-protocol, tokio, rusqlite, ...]
└── crates/nexus42d           [nexus-contracts, nexus-domain, axum, tower, rusqlite, anyhow, ...]
```

### 1.2 Dependency Flow

```
nexus-contracts  ← (no internal deps)
     ↑
nexus-domain     ← nexus-contracts
     ↑
nexus-sync       ← nexus-contracts (NOT nexus-domain — intentional)
nexus42          ← nexus-contracts, nexus-domain
nexus42d         ← nexus-contracts, nexus-domain
```

### 1.3 Assessment

| Criterion | Rating | Notes |
|-----------|--------|-------|
| Boundary cleanliness | **Good** | Clear unidirectional flow; no circular deps |
| Dependency minimality | **Good** | Each crate only pulls what it needs |
| Shared types strategy | **Good** | `nexus-contracts` as single truth source works well |
| `nexus-sync` independence | **Good** | Sync crate does NOT depend on `nexus-domain` — correct per spec |
| `nexus42` ↔ `nexus42d` coupling | **Concern** | No shared crate for auth types or schema defs |

**Finding ARCH-1** [MEDIUM]: No shared auth or database schema crate between CLI and daemon. Auth types (`UserAuthState` in CLI, `UserSession` in daemon) and SQLite table definitions are duplicated. Target: V1.1 shared crate or daemon-as-single-db-owner pattern.

**Finding ARCH-2** [LOW]: `nexus42` depends on `nexus-domain` but does not use domain types in most command handlers — commands work with contract types directly. This is acceptable for V1.0 but suggests domain logic is not yet integrated into command flows.

---

## 2. Domain Model Assessment

### 2.1 Aggregate Inventory (15 aggregates)

| Module | Aggregate | Lines | Tests | Key Invariants |
|--------|-----------|-------|-------|----------------|
| `creator.rs` | Creator | 382 | 10 | Active status for pairing; experience requires pairing |
| `world.rs` | World | 270 | 5 | Active required for fork/time advance |
| `manuscript_state.rs` | ManuscriptState | 264 | 9 | Linear phase progression; pre-gate cleanup |
| `key_block.rs` | KeyBlock | 504 | 16 | 5-gate confirm; immutable when confirmed |
| `timeline_event.rs` | TimelineEvent | 466 | 13 | Sequence monotonicity; causality; no self-reference |
| `memory_item.rs` | MemoryItem | 355 | ~10 | Scope validation; canon immutability |
| `pairing.rs` | Pairing | ~150 | ~5 | Active-only pairing; unique constraint |
| `fork_branch.rs` | ForkBranch | ~200 | ~5 | Parent must exist; no cycles |
| `world_membership.rs` | WorldMembership | ~200 | ~5 | Role-based permissions |
| `reference_source.rs` | ReferenceSource | ~180 | ~5 | URI format validation |
| `source_anchor.rs` | SourceAnchor | ~100 | ~5 | Manifest visibility check |
| `story_manifest.rs` | StoryManifest | ~200 | ~5 | Status transitions |
| `consistency.rs` | Cross-aggregate rules | 318 | 14 | G1-G6 global invariants |
| `errors.rs` | DomainError | 267 | 18 | Structured error variants |
| `contract_assertions.rs` | Compile-time checks | 361 | ~8 | Domain ↔ Contract type alignment |

### 2.2 Design Quality

**Strengths:**
- **Invariant enforcement is rigorous**: KeyBlock confirm has 5 gates (permission, version, fields, source anchor traceability, conflicts). TimelineEvent promote enforces sequence monotonicity and causality.
- **Bidirectional conversion**: Every aggregate has `From<Contract>` and `From<Domain>` implementations, verified by `contract_assertions.rs` compile-time tests.
- **Consistent patterns**: All aggregates follow the same structure — Rust enum for status, `schema_version: 1`, `created_at`/`updated_at` timestamps, `#[serde(skip_serializing_if)]` for optional fields.
- **Proper error types**: `DomainError` has 18 variants with structured context (not just strings), making programmatic error handling possible.

**Concerns:**

**Finding DM-1** [HIGH]: `MembershipPermissionCheck` is duplicated in both `key_block.rs` and `timeline_event.rs` with identical structure. Should be extracted to a shared type.

```
// key_block.rs:71-75 and timeline_event.rs:53-57 — identical definitions
pub struct MembershipPermissionCheck {
    pub can_confirm_canon: bool,
    pub can_sync_kb: bool,
}
```

**Finding DM-2** [MEDIUM]: Enum-as-string pattern is verbose. Domain enums like `CreatorStatus`, `WorldStatus`, `KeyBlockStatus` define `as_str()` methods, but the aggregates store status as `String` not the enum type. This means invalid states can be constructed:

```rust
// This compiles but creates an invalid state:
let mut creator = Creator::register(...);
creator.status = "invalid_status".to_string(); // No compile-time protection
```

A newtype wrapper or storing the enum directly would be safer.

**Finding DM-3** [LOW]: `Creator.style_profile` uses `serde_json::Value` in the contract type and a typed `StyleProfile` struct in the domain. The `From` impl does `serde_json::from_value().unwrap_or(...)` which silently drops malformed data. This is intentional resilience but should at minimum log a warning.

---

## 3. Sync Mechanism Assessment

### 3.1 Architecture

```
User Action → SyncCommand → BundleBuilder → DeltaBundle → Outbox → SyncClient → Platform
                                                      ↓                    ↓
                                               Precheck (local)    ConflictResponse
                                                      ↓                    ↓
                                              PartialApplyResult ← PushResponse
```

### 3.2 Module Assessment

| Module | Lines | Tests | Assessment |
|--------|-------|-------|------------|
| `command.rs` | 264 | 3 | Clean bidirectional conversion; `CommandOrigin` orphaned |
| `delta_bundle.rs` | 467 | 12 | Good builder pattern; unused `command` parameter |
| `outbox.rs` | 634 | 12 | Solid state machine; missing index in test DDL |
| `conflict.rs` | 527 | 15 | Good resolution strategy; manual JSON parsing |
| `sync_client.rs` | 436 | 9 | Clean HTTP client; double JSON parsing; no response size limit |
| `partial_apply.rs` | 400 | 14 | Good retry logic; no persistence for retry state |
| `precheck.rs` | 668 | 18 | Excellent local validation; `field_path` always `None` |
| `errors.rs` | 124 | — | Well-structured 20 variants; loses error source chain |

### 3.3 Key Findings

**Finding SYNC-ARCH-1** [HIGH]: `SyncError::SyncConflict` discards the full `ConflictResponse`. Only `conflict_type: String` is preserved. Callers lose `server_world_revision`, `conflicts[]` details, `retry_after`, etc. This is a significant information loss that will cause issues when implementing retry logic.

```rust
// errors.rs:42 — only preserves the type string
SyncConflict { conflict_type: String },
// Should be: SyncConflict { response: ConflictResponse },
```

**Finding SYNC-ARCH-2** [MEDIUM]: `unchecked_transaction()` in outbox bypasses `BEGIN IMMEDIATE`. For a single-process CLI this is acceptable but should be documented as a concurrency assumption. If the daemon ever serves multiple concurrent requests, this could cause `SQLITE_BUSY`.

**Finding SYNC-ARCH-3** [MEDIUM]: `From<serde_json::Error>` and `From<rusqlite::Error>` implementations wrap errors into `String`, losing the `.source()` chain. This makes debugging harder since `std::error::Error::source()` returns `None`.

**Finding SYNC-ARCH-4** [LOW]: `BundleBuilder::command()` takes a `&SyncCommandVariant` but ignores it entirely — only generates a random `command_id`. The API is misleading.

**Finding SYNC-ARCH-5** [LOW]: Test constructor `new_in_memory()` in outbox is missing the `idx_outbox_bundle_id` index that the production `new()` creates. This means tests and production have divergent DDL.

### 3.4 Extensibility Assessment

The sync mechanism is well-designed for extension:
- `DeltaType` and `DeltaOperation` enums can be extended with new variants
- `SyncCommandVariant` is cleanly separated from the generated `SyncCommand`
- `PartialApplyResult` supports both full-success and partial-failure scenarios
- `ConflictResolver` strategy pattern allows adding new resolution strategies

---

## 4. CLI/Daemon Architecture

### 4.1 CLI (`nexus42`)

**Structure:**
```
src/
├── main.rs          (147 lines — clap dispatch)
├── lib.rs           (16 lines — re-exports)
├── commands/        (8 subcommand modules)
├── acp/             (6 files, ~2,411 lines — ACP integration)
├── api/             (daemon_client.rs — 94 lines)
├── auth/            (3 files — dual-subject auth)
├── context/         (4 files — context assembly)
├── config.rs        (107 lines)
└── errors.rs        (56 lines)
```

### 4.2 Daemon (`nexus42d`)

**Structure:**
```
src/
├── main.rs          (75 lines — axum server)
├── lib.rs           (8 lines)
├── api/             (8 handler files — ~395 lines)
├── auth/            (4 stub files — ~80 lines)
└── workspace/       (2 files — ~214 lines)
```

### 4.3 Key Findings

**Finding CLI-DAEMON-1** [CRITICAL]: `WorkspaceState::init_workspace()` does not set `self.workspace_path`. The `is_initialized()` method checks `workspace_path.is_some()`, so after initialization, `is_initialized()` always returns `false`.

```rust
// workspace/mod.rs — init_workspace() creates dirs and writes to DB
// but never sets self.workspace_path = Some(path)
// is_initialized() returns self.workspace_path.is_some() → always false
```

**Finding CLI-DAEMON-2** [CRITICAL]: The CLI `ContextClient` calls `POST /v1/local/context/assemble` on the daemon, but this route is **not registered** in `api::create_router()`. The CLI will receive a 404 at runtime.

**Finding CLI-DAEMON-3** [HIGH]: Daemon handlers never return HTTP error status codes. All responses are HTTP 200 with embedded `{ success: false, message }`. This makes HTTP-level error detection impossible.

```rust
// workspace handler — returns 200 even on failure:
Json(serde_json::json!({
    "success": false,
    "message": format!("Failed to initialize: {}", e)
}))
// Should return (StatusCode::INTERNAL_SERVER_ERROR, Json(...))
```

**Finding CLI-DAEMON-4** [HIGH]: No shared auth types between CLI and daemon. `UserAuthState` (CLI) and `UserSession` (daemon) represent the same concept but are independently defined. The daemon's auth handler reads `auth.json` as raw `serde_json::Value` instead of using CLI's `AuthStore`.

**Finding CLI-DAEMON-5** [HIGH]: `CliError` has no `From<AcpError>` implementation. ACP errors are converted via `String` → `CliError::Other(...)`, losing all structured error information.

**Finding CLI-DAEMON-6** [MEDIUM]: Workspace initialization logic is duplicated in three places:
1. `nexus42/src/commands/init.rs` — creates dirs + `.gitignore` + `workspace.json`
2. `nexus42d/src/workspace/mod.rs` — creates dirs only (no `.gitignore` or `workspace.json`)
3. `nexus42d/src/workspace/manager.rs` — creates dirs + `.gitignore` + `workspace.json`

**Finding CLI-DAEMON-7** [MEDIUM]: `NEXUS_DIR` constant (`.nexus42`) is duplicated in `config.rs` and `acp/registry.rs`.

**Finding CLI-DAEMON-8** [MEDIUM]: `DaemonClient` has no timeout configuration. A hung daemon blocks the CLI indefinitely since `reqwest::Client` uses default timeouts.

---

## 5. ACP Integration Assessment

### 5.1 Architecture

```
commands::agent
  ├→ AgentSpawner::spawn()          [WORKS — subprocess lifecycle]
  ├→ AcpSdkAdapter::initialize()    [STUB — returns placeholder]
  ├→ AcpSdkAdapter::create_session() [STUB — returns placeholder]
  ├→ AcpSdkAdapter::prompt()        [STUB — returns placeholder]
  ├→ AcpSession::shutdown()         [WORKS — graceful shutdown]
  └→ RegistryClient                 [WORKS — CDN fetch + cache]
```

### 5.2 Assessment

**Strengths:**
- **Adapter pattern is well-designed**: `NexusAcpClient` trait isolates the SDK behind an abstraction boundary, enabling mock testing and future `sacp` (Structured ACP) migration.
- **`!Send` isolation is architecturally correct**: The plan to use `tokio::task::LocalSet` + `spawn_local` is the right approach for SDK futures that are `!Send`.
- **Registry caching is solid**: Stale-while-revalidate with 24h TTL, proper cache directory, case-insensitive agent lookup.
- **Graceful shutdown protocol is robust**: cancel → 5s wait → SIGTERM → 2s wait → SIGKILL. `kill_on_drop(true)` ensures cleanup.

**Concerns:**

**Finding ACP-ARCH-1** [CRITICAL]: `AcpSdkAdapter` returns placeholder responses for ALL operations. `initialize()`, `create_session()`, `prompt()`, and `subscribe()` all log warnings and return hardcoded values. The actual `LocalSet` thread + channel bridge is entirely unimplemented.

```rust
// client.rs — all methods are stubs:
async fn initialize(&self, ...) -> AcpResult<InitializedSession> {
    tracing::warn!("ACP initialize: stub implementation");
    Ok(InitializedSession { /* placeholder */ })
}
```

This means: `nexus42 agent run <agent>` will spawn a subprocess but never actually communicate with it via ACP protocol. The `interactive_prompt_loop` reads stdin but does NOT send messages to the agent.

**Finding ACP-ARCH-2** [MEDIUM]: `RegistryClient::default()` calls `Self::new().expect(...)` which can panic if `HOME` is unavailable. This violates Rust conventions for `Default`.

**Finding ACP-ARCH-3** [MEDIUM]: `find_agent("")` (empty query) matches the first agent in the registry because `"".to_lowercase().starts_with("")` is always `true`. Empty query should return `None`.

**Finding ACP-ARCH-4** [LOW]: `subscribe()` creates an immediately-closed broadcast channel, which silently produces `RecvError::Closed` for any consumer. Defensive but misleading.

### 5.3 V1.1 Readiness

The ACP integration is **structurally ready** for V1.1 — the trait boundary, error types, and transport layer are solid. The primary work item is implementing the actual `LocalSet` bridge:

1. Spawn `tokio::task::LocalSet` thread
2. Create `ClientSideConnection` inside `LocalSet`
3. Bridge via `mpsc` channels between async and `!Send` contexts
4. Implement `SimpleClientHandler` with proper permission policy

Estimated: **M complexity** (~2-3 agent sessions).

---

## 6. Schema/Codegen Pipeline

### 6.1 Schema Organization

```
schemas/
├── meta/            (1 file — validation meta-schema)
├── common/          (3 files — types, source-anchor, version-ref)
├── domain/          (14 files — all entities)
├── cli-sync/        (2 files — bundle narrowing, conflict response)
├── acp-runtime/     (1 file — registry manifest)
└── platform/        (1 file — context assembly V1)
```

**Total**: 22 JSON Schema files, all Draft-07, all versioned with `schema_version: 1`.

### 6.2 Assessment

**Strengths:**
- **Clean layered organization**: meta → common → domain → cli-sync → platform
- **Canonical `$ref` URIs**: All domain schemas reference `common.schema.json` via canonical URI (`https://nexus42.invalid/schemas/...`)
- **Proper validation tooling**: AJV-based `schema-validator.js` catches structural issues in CI
- **Single-pass codegen**: `pnpm run codegen` generates both TypeScript and Rust from schemas

**Concerns:**

**Finding CODEGEN-1** [HIGH]: `CommonTypes` generation is **hard-coded** in both `ts-generator.ts` and `rust-generator.ts`. The `COMMON_DEFINITIONS` map is correctly populated from `common.schema.json` but is NOT used for generating the common types file. Adding a new definition to `common.schema.json` requires manually updating the generator source code.

```typescript
// ts-generator.ts:252-297 — hard-coded list of types
function generateCommonTypesFile() {
  // These are NOT driven by COMMON_DEFINITIONS map:
  // SchemaVersion, Timestamp, WorldId, CreatorId, ...
}
```

**Finding CODEGEN-2** [HIGH]: `ContextAssemblyV1` types are **silently dropped** by codegen. `platform/context-assembly-v1.schema.json` has no root-level `properties` (definitions only), so the `isDefinitionsOnly` check skips it entirely. `ContextAssembleRequestV1` and `ContextAssembleResponseV1` are absent from both TypeScript and Rust output.

**Finding CODEGEN-3** [MEDIUM]: Registry manifest nested definitions (`AgentEntry`, `Distribution`, `NpxDistribution`, etc.) are not generated as named types. In Rust they become opaque `serde_json::Value` blobs.

**Finding CODEGEN-4** [MEDIUM]: No runtime validation generated. JSON Schema constraints (`minLength`, `pattern`, `minimum`) are completely lost. Generated types only capture structural shape, not validation rules.

**Finding CODEGEN-5** [LOW]: `additionalProperties: false` on bundle envelope is not enforced in Rust. Generated structs use default serde settings which allow arbitrary extra fields. Would need `#[serde(deny_unknown_fields)]`.

**Finding CODEGEN-6** [LOW]: Two separate validators exist: AJV-based `schema-validator.js` (full validation) and codegen-internal checks (lightweight). These could diverge.

---

## 7. Cross-cutting Concerns

### 7.1 Error Handling

| Crate | Strategy | Assessment |
|-------|----------|------------|
| `nexus-domain` | `thiserror::Error` | **Good** — structured variants, `PartialEq` for testing |
| `nexus-sync` | `thiserror::Error` | **Good** — 20 well-categorized variants |
| `nexus42` (CLI) | `thiserror::Error` | **Adequate** — but `anyhow` → `Other` loses chain |
| `nexus42d` (daemon) | `anyhow::Error` | **Problematic** — untyped errors at API boundary |

**Finding CC-ERR-1** [HIGH]: Error propagation is inconsistent across crate boundaries. The daemon uses `anyhow` internally and returns `Json({success: false, message})` with HTTP 200. The CLI maps this to `CliError::Api { status: 200, message }`. Structured error codes from the domain layer (`DomainError`) are stringified and lost.

**Finding CC-ERR-2** [MEDIUM]: `From<anyhow::Error> for CliError` calls `.to_string()` which only captures the outermost error message. The full error chain is discarded.

### 7.2 Testing Patterns

| Crate | Test Count | Coverage | Quality |
|-------|-----------|----------|---------|
| `nexus-domain` | 133 | High | Excellent — invariant gates, edge cases, contract roundtrips |
| `nexus-sync` | 226 | High | Excellent — state machine, conflict resolution, precheck |
| `nexus42` | ~50 | Medium | Good integration tests with `wiremock`; unit tests sparse |
| `nexus42d` | ~10 | Low | Basic handler tests; no error path coverage |
| `nexus-contracts` | ~5 | Low | Only serde roundtrip tests |

**Total**: ~445 workspace tests (matches claimed count).

**Finding CC-TEST-1** [MEDIUM]: Daemon handler tests are minimal. Error paths (database failures, invalid inputs) are not tested. The critical `WorkspaceState` initialization bug (CLI-DAEMON-1) suggests insufficient test coverage.

**Finding CC-TEST-2** [LOW]: No property-based testing. Domain invariants (e.g., "confirm always increments revision") could benefit from `proptest` or `quickcheck`.

### 7.3 Dependency Health

**Workspace dependencies** (16 crates in `[workspace.dependencies]`):

| Dependency | Version | Necessity | Notes |
|-----------|---------|-----------|-------|
| `serde` 1.0 | Essential | Industry standard |
| `tokio` 1.35 (full) | Essential | Could reduce features to save compile time |
| `clap` 4.5 | Essential | CLI argument parsing |
| `reqwest` 0.12 | Essential | HTTP client for sync |
| `rusqlite` 0.31 (bundled) | Essential | SQLite with bundled = portable |
| `axum` 0.7 | Essential | Daemon HTTP framework |
| `agent-client-protocol` =0.10.4 | Essential (ACP) | Pinned exact version |
| `thiserror` 1.0 | Essential | Error types |
| `anyhow` 1.0 | Used by daemon | Could standardize on `thiserror` |
| `tracing` 0.1 | Essential | Observability |
| `chrono` 0.4 | Essential | Timestamps |
| `uuid` 1.0 | Essential | ID generation |
| `tower` / `tower-http` | Essential | Axum middleware |
| `dirs` 5 | Essential | Platform directories |
| `url` 2 | Minimal use | Only used in ACP registry |
| `nix` 0.28 | Platform-specific | Unix signal handling |

**Additional non-workspace deps**: `tokio-util`, `async-trait`, `async-broadcast`, `assert_cmd`, `predicates`, `tempfile`, `wiremock`, `axum-test`, `jsonschema`.

**Finding CC-DEP-1** [LOW]: `tokio` is imported with `features = ["full"]` at workspace level. Individual crates could reduce features (e.g., `nexus-domain` only needs `test-util` for tests). This would reduce compile time.

**Finding CC-DEP-2** [LOW]: `async-broadcast` is imported explicitly in `nexus42/Cargo.toml` with comment "re-exported by agent-client-protocol". Consider removing and using the re-exported version directly.

### 7.4 Build Assessment

- **5 crates** in workspace — small, manageable
- **Resolver 2** — correct for mixed binary/library workspace
- **No feature unification issues** — all crates use same workspace dep versions
- **`bundled` rusqlite** — portable but increases binary size (~2MB)
- **`nightly` rustfmt required** — correctly documented in AGENTS.md

---

## 8. Residual Impact Assessment

### 8.1 Classification: Architectural vs. Feature

| Category | Count | Examples |
|----------|-------|---------|
| **Architectural** (affect crate boundaries, error strategy, data flow) | 14 | DEBT-X1..X4, CLI-R9..R14, SYNC-R4, CODEGEN-1..2 |
| **Feature gaps** (missing capabilities for V1.1) | 17 | ACP-R3..R11, CLI-R5..R8, SYNC-R6..R13, CTX-R2..R7 |
| **Code quality** (test coverage, DRY, naming) | 7 | DM-R3, DM-R5, CLI-R15, CTX-R5, CC-TEST-1..2 |

### 8.2 Pre-Phase-2 Priorities

**Must address BEFORE V1.0-phase2** (blockers for new feature work):

1. **CLI-DAEMON-1** [CRITICAL]: Fix `WorkspaceState::init_workspace()` to set `workspace_path`. Without this, no daemon operation can detect workspace state correctly.
2. **CLI-DAEMON-2** [CRITICAL]: Register `POST /v1/local/context/assemble` route in daemon, or remove from CLI client.
3. **DEBT-X1** [HIGH]: SQLite connection pooling. The current per-request pattern will not scale for V1.0-phase2 features (sync daemon loops, concurrent context assembly).
4. **DEBT-X2** [HIGH]: Schema duplication. Three independent SQLite schema definitions will drift further during Phase 2.

**Should address EARLY in V1.0-phase2** (high impact, moderate effort):

5. **CC-ERR-1** [HIGH]: Standardize error propagation. Define a shared error strategy across CLI/daemon boundary.
6. **DEBT-X4** [HIGH]: Daemon workspace validation middleware. All handlers should check workspace state before proceeding.
7. **CODEGEN-1** [HIGH]: Drive CommonTypes generation from `common.schema.json` definitions, not hard-coded lists.
8. **CODEGEN-2** [HIGH]: Generate types for `context-assembly-v1.schema.json` definitions.

**Can defer to mid/late V1.0-phase2**:

9. **ACP-ARCH-1**: ACP SDK bridge implementation (needed when agent commands are functional)
10. **DM-1**: Extract shared `MembershipPermissionCheck` type
11. **SYNC-ARCH-1**: Preserve full `ConflictResponse` in error type
12. All remaining LOW/MEDIUM residuals

### 8.3 Cross-cutting Debt Assessment (DEBT-X1..X4)

| ID | Title | Severity | Scope Accurate? | Recommendation |
|----|-------|----------|----------------|----------------|
| DEBT-X1 | SQLite connection pooling | **HIGH** | **Yes** — accurately scopes both `nexus42d` (per-request) and `nexus-sync` (direct connections) | **Promote to V1.0-phase2 Blocker**. Use `deadpool-sqlite` or `r2d2`. |
| DEBT-X2 | Schema duplication | **HIGH** | **Yes** — 3 locations confirmed | **Promote to V1.0-phase2 Blocker**. Extract to shared crate or centralize through daemon API. |
| DEBT-X3 | Error propagation | **MEDIUM** | **Yes** — `anyhow` in daemon, `thiserror` in sync | Acceptable for V1.0 GA. Address early Phase 2. |
| DEBT-X4 | Workspace validation | **HIGH** | **Yes** — handlers skip state checks | **Promote to V1.0-phase2 Blocker**. Add Axum middleware layer. |

---

## 9. V1.0-phase2 Architecture Recommendations

### 9.1 What to Fix First (V1.0-phase2 Blockers)

These items must be resolved before starting V1.0-phase2 feature work:

| Priority | Item | Effort | Agent Sessions |
|----------|------|--------|----------------|
| P0 | Fix CLI-DAEMON-1 (workspace init bug) | XS | ~0.5 |
| P0 | Fix CLI-DAEMON-2 (missing daemon route) | XS | ~0.5 |
| P1 | DEBT-X1: SQLite connection pooling | S | ~1-2 |
| P1 | DEBT-X2: Schema deduplication | S | ~1-2 |
| P1 | DEBT-X4: Workspace validation middleware | S | ~1-2 |

**Estimated pre-Phase-2 effort**: **S** total (~3-5 agent sessions).

### 9.2 Architecture Priorities for V1.0-phase2

| Priority | Area | Rationale |
|----------|------|-----------|
| **1** | ACP SDK bridge implementation | Largest gap — adapter pattern exists but is non-functional. Enables agent commands. |
| **2** | Error strategy standardization | Inconsistent error handling will cause pain in every new feature. |
| **3** | Codegen hard-coded CommonTypes fix | Adding new schema definitions requires manual codegen source changes. |
| **4** | Context assembly V1.1 (daemon-side) | CLI types and client exist but daemon handler is missing. |
| **5** | Auth flow completion | Device code OAuth and token refresh are skeletons. |

### 9.3 Architecture Risks for V1.0-phase2

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| ACP SDK breaking changes (v0.10.4 → newer) | Medium | High | Pin exact version; adapter trait isolates changes |
| SQLite concurrency under sync daemon loops | Medium | Medium | DEBT-X1 (connection pooling) must land first |
| Context assembly types violating schema-first contract | High | Medium | CLI hand-wrote types instead of using codegen; must align |
| Platform API contract drift (CLI expects routes platform doesn't serve) | Medium | High | Formalize API contract tests before V1.0-phase2 |
| `!Send` futures blocking async runtime | Low | High | `LocalSet` bridge is designed; just needs implementation |

### 9.4 Recommended Architecture Principles for V1.0-phase2

1. **Every new type must come from schemas** — no more hand-written DTOs that bypass codegen. Fix `context/types.rs` to use generated types.
2. **Error types must be structured** — no more `anyhow` at API boundaries. Define error codes and propagate them through the full stack.
3. **Daemon owns all SQLite state** — CLI should access workspace data through daemon API, not direct SQLite connections. This resolves DEBT-X2 naturally.
4. **HTTP status codes must be meaningful** — daemon handlers should return proper 4xx/5xx status codes, not always 200.
5. **Auth tokens must have lifecycle management** — implement refresh logic and expiration checks before V1.0-phase2 ships.

---

## Appendix: File Inventory

### Files Read During This Review

**Workspace Configuration:**
- `Cargo.toml` (workspace root)

**nexus-contracts:**
- `crates/nexus-contracts/Cargo.toml`
- `crates/nexus-contracts/src/lib.rs`
- `crates/nexus-contracts/src/generated/` (directory listing, 20 files)

**nexus-domain:**
- `crates/nexus-domain/Cargo.toml`
- `crates/nexus-domain/src/lib.rs`
- `crates/nexus-domain/src/errors.rs` (267 lines)
- `crates/nexus-domain/src/consistency.rs` (318 lines)
- `crates/nexus-domain/src/creator.rs` (382 lines)
- `crates/nexus-domain/src/world.rs` (270 lines)
- `crates/nexus-domain/src/manuscript_state.rs` (264 lines)
- `crates/nexus-domain/src/key_block.rs` (504 lines)
- `crates/nexus-domain/src/timeline_event.rs` (466 lines)
- `crates/nexus-domain/src/memory_item.rs` (355 lines, first 50 lines)
- `crates/nexus-domain/src/contract_assertions.rs` (361 lines, first 30 lines)
- `crates/nexus-domain/src/` (directory listing, 16 files)

**nexus-sync:**
- `crates/nexus-sync/Cargo.toml`
- `crates/nexus-sync/src/lib.rs`
- `crates/nexus-sync/src/outbox.rs` (634 lines — via subagent)
- `crates/nexus-sync/src/delta_bundle.rs` (467 lines — via subagent)
- `crates/nexus-sync/src/conflict.rs` (527 lines — via subagent)
- `crates/nexus-sync/src/sync_client.rs` (436 lines — via subagent)
- `crates/nexus-sync/src/errors.rs` (124 lines — via subagent)
- `crates/nexus-sync/src/command.rs` (264 lines — via subagent)
- `crates/nexus-sync/src/partial_apply.rs` (400 lines — via subagent)
- `crates/nexus-sync/src/precheck.rs` (668 lines — via subagent)

**nexus42 (CLI):**
- `crates/nexus42/Cargo.toml`
- `crates/nexus42/src/main.rs` (147 lines)
- `crates/nexus42/src/lib.rs`
- `crates/nexus42/src/errors.rs` (56 lines)
- `crates/nexus42/src/config.rs` (107 lines)
- `crates/nexus42/src/commands/mod.rs`
- `crates/nexus42/src/acp/` (6 files, ~2,411 lines — via subagent)
- `crates/nexus42/src/api/daemon_client.rs` (94 lines — via subagent)
- `crates/nexus42/src/auth/` (3 files — via subagent)
- `crates/nexus42/src/context/` (4 files — via subagent)

**nexus42d (Daemon):**
- `crates/nexus42d/Cargo.toml`
- `crates/nexus42d/src/main.rs` (75 lines)
- `crates/nexus42d/src/lib.rs`
- `crates/nexus42d/src/api/` (8 files — via subagent)
- `crates/nexus42d/src/workspace/` (2 files — via subagent)
- `crates/nexus42d/src/auth/` (4 files — via subagent)

**Schemas:**
- `schemas/` (22 JSON Schema files — via subagent)

**Codegen:**
- `tooling/codegen/` (full pipeline — via subagent)

**TypeScript Package:**
- `packages/nexus-contracts/` (full structure — via subagent)

**Plans and Status:**
- `.mstar/status.json` (792 lines — full residual findings)
- `.mstar/knowledge/README.md`

### Finding Summary

| Severity | Count | IDs |
|----------|-------|-----|
| **CRITICAL** | 3 | CLI-DAEMON-1, CLI-DAEMON-2, ACP-ARCH-1 |
| **HIGH** | 7 | SYNC-ARCH-1, CLI-DAEMON-3..5, CODEGEN-1..2, CC-ERR-1 |
| **MEDIUM** | 14 | ARCH-1, DM-2, SYNC-ARCH-2..3, CLI-DAEMON-6..8, ACP-ARCH-2..3, CODEGEN-3..4, CC-ERR-2, CC-TEST-1 |
| **LOW** | 12 | ARCH-2, DM-1, DM-3, SYNC-ARCH-4..5, ACP-ARCH-4, CODEGEN-5..6, CC-DEP-1..2, CC-TEST-2 |
| **Total** | **36** | |

*Note: These 36 findings are separate from the 38 residuals tracked in `status.json`. Some findings overlap with residuals (e.g., CLI-DAEMON-3 relates to DEBT-X4, SYNC-ARCH-1 relates to SYNC residual items) but the findings provide more specific file/line references and architectural context.*

# Restructured Context Assembly Specification v1

> **Status**: Archived — Plan 11 (`2026-04-09-v1.1-context-assembly-enhancement.md`) is the current authoritative source for context assembly work.
> **Scope**: CLI-side only (this repo: `nexus`). Platform-side Context Assembly service belongs in private `nexus-platform` repo.  
> **Superseded**: The original plan at `.agents/plans/2025-04-05-context-assembly.md` which contained fundamental tech stack and responsibility split violations.

---

## 1. Overview

### 1.1 Why Restructure

The original Context Assembly implementation plan (`.agents/plans/2025-04-05-context-assembly.md`) contained five critical deviations from frozen specifications. This restructured spec resolves all five:

| # | Conflict | Severity | Resolution |
|---|----------|----------|------------|
| 1 | **Tech Stack Conflict**: Plan proposed Rust crate `crates/nexus-context/` with Neo4j/Postgres/pgvector clients. Context Assembly is frozen as TypeScript-only implementation. | P1 Critical | Remove Rust crate entirely. CLI calls platform API; Context Assembly is a platform-side TypeScript service. |
| 2 | **Storage Ownership Conflict**: Plan proposed CLI-side direct access to Neo4j/Postgres/pgvector. World/Creator metadata, Key Block structured state, Timeline/Fork graph, Explore indexes, and subscription/permission info are all platform-authoritative stores. | P1 Critical | Prohibit CLI-side direct database access. Local API endpoint is the only integration point. |
| 3 | **Responsibility Split Violation**: Plan put assembly logic in CLI/Rust. CLI generates summaries → platform receives/validates/indexes → agent consumes assembled context. | P1 Critical | Align with responsibility split (see §2 below). |
| 4 | **Over-Scoped Embedding/Reranking**: Plan included OpenAI/local model embeddings and reranking. LLM-as-judge and ColBERT/cross-encoder reranking are explicitly excluded from V1.0. | P2 | Remove embedding/reranking from CLI-side scope. Defer to platform-side future enhancements. |
| 5 | **Docker Compose Misalignment**: Plan's docker-compose included Neo4j + Postgres + pgvector + Redis for CLI dev. CLI uses SQLite for local structured state. | P3 | Remove docker-compose from CLI-side plan. Move to separate platform-dev infrastructure file (not in this repo). |

### 1.2 Revision Goals

1. **Narrow scope to CLI-side only**: Summary generation + Local API call + bundle metadata integration.
2. **Respect platform authority**: Neo4j, Postgres, pgvector are platform infrastructure — CLI never touches them directly.
3. **Align with frozen responsibility split**: CLI generates `StoryManifest.summary_text`; platform does everything else.
4. **Provide clear integration contracts**: `POST /v1/local/context/assemble` request/response shapes.
5. **Define actionable CLI-side tasks** with file paths and verification commands.

### 1.3 Design Decisions (Inlined)

All restructured decisions are based on these frozen design constraints:

- **TypeScript-only implementation**: Context Assembly service must be implemented entirely in TypeScript
- **CLI generates summaries; platform does everything else**: CLI generates `StoryManifest.summary_text`; platform receives, validates, indexes into graph/vector stores, and serves assembled context
- **Platform-authoritative stores**: World/Creator metadata, Key Block structured state, Timeline/Fork graph, Explore indexes, and subscription/permission info are all managed by the platform — CLI never accesses them directly
- **Local API is the only integration point**: `POST /v1/local/context/assemble` is the frozen Local API contract between CLI and Context Assembly
- **`nexus42 context assemble`**: reads confirmed KBs, recent canon timeline, and memory slices; outputs a read-only snapshot for local agent/template consumption
- **Responsibility split**: CLI generates story summaries and includes them in structured sync submissions; platform receives summaries, validates, checks consistency, persists to database, vectorizes, and provides Context Assembly retrieval

---

## 2. Responsibility Split

### 2.1 Architecture Diagram

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                         THIS REPO (nexus) — CLI Side                       │
│                                                                             │
│  ┌──────────────┐    ┌──────────────────────┐    ┌───────────────────────┐  │
│  │  Manuscript   │    │  Summary Generation   │    │  Bundle Metadata      │  │
│  │  Files        │───▶│  Module               │───▶│  Integration          │  │
│  │  Stories/     │    │  (extract summary     │    │  (attach summary to   │  │
│  │  <world>/     │    │   from local files)   │    │   sync bundle)        │  │
│  └──────────────┘    └──────────────────────┘    └───────┬───────────────┘  │
│                                                            │                  │
│  ┌──────────────────────────────────────────────────────────▼──────────────┐ │
│  │                     nexus42 CLI / nexus42d                             │ │
│  │                                                                        │ │
│  │  ┌─────────────────────────────────────────────────────────────────┐   │ │
│  │  │  POST /v1/local/context/assemble  (Local API → Platform proxy)  │   │ │
│  │  └─────────────────────────────────────────────────────────────────┘   │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────┬──────────────────────────────────────────┘
                                   │ HTTPS
                                   ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                   PRIVATE REPO (nexus-platform) — Platform Side              │
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │              ContextAssemblyService (TypeScript)                        │  │
│  │                                                                        │  │
│  │  ┌──────────┐  ┌──────────────┐  ┌────────────┐  ┌─────────────────┐  │  │
│  │  │  Neo4j   │  │  Postgres     │  │  pgvector   │  │  Query Façade   │  │  │
│  │  │  (KB,    │  │  (Story       │  │  (Memory    │  │  (GraphQL /    │  │  │
│  │  │  Timeline│  │   summary     │  │   embed     │  │   read-only    │  │  │
│  │  │  Fork)   │  │   metadata)   │  │   retrieval)│  │   aggregation) │  │  │
│  │  └──────────┘  └──────────────┘  └────────────┘  └─────────────────┘  │  │
│  │                                                                        │  │
│  │  ← Receives CLI-generated summaries via sync API (Phase A → B)         │  │
│  │  ← Validates, indexes into Neo4j/pgvector                              │  │
│  │  ← Assembles stable read-only context snapshots                         │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ❌ NOT in this repo: Neo4j client, pgvector client, HybridRAG logic,      │
│     query façade — all are platform-side TypeScript implementations        │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Responsibility Matrix

| Responsibility | Owner | Repo | Tech | Design Constraint |
|---|---|---|---|---|
| **Summary generation** from local manuscript files | CLI | `nexus` (this repo) | Rust | CLI generates summaries; platform does everything else |
| **Local API call** to request assembled context | CLI / daemon | `nexus` (this repo) | Rust (HTTP client) | Local API contract (frozen) |
| **Bundle metadata** (attach summary to sync bundle) | CLI | `nexus` (this repo) | Rust | `bundle.schema.json`, `story-manifest.schema.json` |
| **Context Assembly service** (HybridRAG + query façade) | Platform | `nexus-platform` (private) | TypeScript / Next.js | TypeScript-only implementation; frozen design |
| **Neo4j graph storage** (KB, Timeline, Fork) | Platform | `nexus-platform` (private) | TypeScript + Neo4j driver | Platform-authoritative graph store |
| **Postgres/pgvector** (Story summary, Memory embeddings) | Platform | `nexus-platform` (private) | TypeScript + pgvector | Platform-authoritative relational store |
| **Vector retrieval + similarity search** | Platform | `nexus-platform` (private) | TypeScript | V1.0 baseline; LLM-as-judge and cross-encoder reranking deferred to V1.1+ |
| **Embedding generation** (future: OpenAI or local model) | Platform | `nexus-platform` (private) | TypeScript | Deferred to V1.1+ |
| **Reranking** (future: LLM-as-judge, cross-encoder) | Platform | `nexus-platform` (private) | TypeScript | Explicitly excluded from V1.0 |

### 2.3 Strict Boundaries

**CLI MUST NOT:**
- Connect to Neo4j, Postgres, or pgvector directly
- Include any Neo4j/Postgres/pgvector client libraries in its dependency tree
- Perform embedding generation or similarity search
- Implement HybridRAG logic
- Include docker-compose for Neo4j/Postgres/pgvector/Redis

**CLI MUST:**
- Generate `StoryManifest.summary_text` from local manuscript files
- Call `POST /v1/local/context/assemble` (proxied to platform) to request assembled context
- Include summary in sync bundle metadata alongside KB/Timeline deltas
- Use only SQLite for local structured state

---

## 3. CLI-Side Scope

### 3.1 Summary Generation

**What**: CLI generates `StoryManifest.summary_text` from local manuscript files under `Stories/<world_ref>/`.

**How** (V1.0 baseline — basic extraction):

1. **Scan manuscript directory**: Walk `Stories/<world_ref>/` for recognized file types (`.md`, `.txt`, `.docx` export targets).
2. **Extract structured metadata**: Parse front-matter or heading structure for title, chapter boundaries, word count.
3. **Generate summary text**: V1.0 uses **basic extraction** (title + chapter list + word count + opening excerpt). No LLM call required.
   - Future V1.1+: LLM-assisted summary generation (using user's local agent via ACP, not a hardcoded model SDK).
4. **Attach to StoryManifest**: Set `StoryManifest.summary_text` on the local working copy.

**File scope**:
- Source: `Stories/<world_ref>/chapter-*.md` (and subdirectories)
- Output: `StoryManifest.summary_text` (stored in local SQLite + included in sync bundle)

**Constraints**:
- Only reads from whitelisted manuscript paths (`Stories/` tree).
- Does NOT read `References/` tree (that's research scope, not manuscript).
- Summary is a plain text field, max TBD bytes (recommend 4096 chars for V1.0).
- Full manuscript text is never included in sync — only the summary.

**Future path (V1.1+)**: Agent-assisted summary via ACP capability `nexus.manuscript.summarize` — CLI sends manuscript content to local agent, receives generated summary. This keeps CLI model-agnostic.

### 3.2 Local API Endpoint

**Endpoint**: `POST /v1/local/context/assemble`

**Source**: Local API contract (frozen)

**Behavior**:
- CLI/daemon sends request to the Local API.
- `nexus42d` proxies this request to the platform's Context Assembly endpoint (HTTPS).
- The platform performs the actual HybridRAG query (Neo4j + pgvector + Postgres).
- Response is returned to CLI as a stable read-only snapshot.

**Note**: The Local API is the **only** integration point between CLI and Context Assembly. CLI never directly accesses platform databases.

### 3.3 Request/Response Schema

The following shapes define the wire contract between CLI and the platform's Context Assembly service. These shapes are also used by the Local API proxy in `nexus42d`.

#### 3.3.1 Request: `ContextAssembleRequestV1`

Aligned with the minimal request shape for `POST /v1/local/context/assemble`, wrapped in the Local API envelope.

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/platform/context-assembly-v1.schema.json",
  "schema_version": 1,
  "title": "ContextAssembleRequestV1",
  "description": "Request shape for POST /v1/local/context/assemble. CLI sends this to request a stable read-only context snapshot from the platform.",
  "type": "object",
  "required": ["request_id", "workspace_id", "creator_id", "world_id"],
  "properties": {
    "request_id": {
      "type": "string",
      "minLength": 1,
      "description": "Caller-generated traceable ID (Local API envelope)"
    },
    "workspace_id": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorkspaceId"
    },
    "creator_id": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/CreatorId"
    },
    "world_id": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorldId"
    },
    "include_memory": {
      "type": "boolean",
      "default": true,
      "description": "Include memory items in assembled context"
    },
    "include_timeline": {
      "type": "boolean",
      "default": true,
      "description": "Include timeline events in assembled context"
    },
    "include_story_summaries": {
      "type": "boolean",
      "default": true,
      "description": "Include story summaries in assembled context"
    },
    "memory_kinds": {
      "type": "array",
      "items": {
        "type": "string",
        "enum": ["story_summary", "research_material", "review_note"]
      },
      "default": ["story_summary", "research_material", "review_note"],
      "description": "Filter memory items by kind"
    },
    "max_timeline_events": {
      "type": ["integer", "null"],
      "minimum": 1,
      "maximum": 100,
      "description": "Maximum number of recent timeline events (null = platform default)"
    },
    "max_story_summaries": {
      "type": ["integer", "null"],
      "minimum": 1,
      "maximum": 50,
      "description": "Maximum number of story summaries (null = platform default)"
    }
  },
  "additionalProperties": false
}
```

#### 3.3.2 Response: `ContextAssembleResponseV1`

Aligned with the minimal output shape for `POST /v1/local/context/assemble`, wrapped in the Local API response envelope.

```json
{
  "title": "ContextAssembleResponseV1",
  "description": "Response shape for POST /v1/local/context/assemble. Platform returns a stable read-only context snapshot.",
  "type": "object",
  "required": ["request_id", "success", "world_id", "assembled_at"],
  "properties": {
    "request_id": {
      "type": "string",
      "description": "Echo of request_id for correlation"
    },
    "success": {
      "type": "boolean",
      "description": "Whether the assembly succeeded"
    },
    "error_code": {
      "type": ["string", "null"],
      "description": "Error code if success=false (e.g., 'auth_expired', 'world_not_found', 'platform_unavailable')"
    },
    "error_message": {
      "type": ["string", "null"],
      "description": "Human-readable error message if success=false"
    },
    "world_id": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/WorldId"
    },
    "assembled_at": {
      "$ref": "https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/Timestamp"
    },
    "data_freshness_hint": {
      "type": ["string", "null"],
      "description": "Freshness indicator (e.g., 'last_indexed_bundle_id') to detect stale data"
    },
    "key_blocks": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "key_block_id": { "type": "string" },
          "block_type": { "type": "string" },
          "name": { "type": "string" },
          "summary": { "type": "string" }
        }
      },
      "description": "Confirmed KeyBlocks relevant to the world"
    },
    "timeline_events": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "event_id": { "type": "string" },
          "event_type": { "type": "string" },
          "description": { "type": "string" },
          "occurred_at": { "type": "string" }
        }
      },
      "description": "Recent canon timeline events"
    },
    "story_summaries": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "story_manifest_id": { "type": "string" },
          "title": { "type": "string" },
          "summary_text": { "type": "string" },
          "manifest_type": { "type": "string" }
        }
      },
      "description": "Story summaries from StoryManifest.summary_text"
    },
    "memory_items": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "memory_id": { "type": "string" },
          "memory_kind": { "type": "string" },
          "content": { "type": "string" }
        }
      },
      "description": "Memory slices (story_summary, research_material, review_note)"
    }
  },
  "additionalProperties": false
}
```

**Output requirements**:
- Same request under same authoritative state MUST return **stable order**.
- Returns a **snapshot**, not a mutable working set.

### 3.4 Bundle Metadata Integration

Summary generation integrates with the sync contract as follows:

**Flow**:

```text
1. CLI generates StoryManifest.summary_text locally
2. CLI creates a DeltaBundle with delta_type="story_manifest"
3. Summary is included as a payload field in the story_manifest delta
4. DeltaBundle carries optional metadata: manuscript_phase, output_manuscript
5. CLI pushes bundle via sync API
6. Platform Phase A: validates and persists to Postgres (authoritative)
7. Platform Phase B: indexes summary into pgvector (async)
```

**Key bundle metadata fields** (from `bundle.schema.json`):

| Field | Purpose | Spec Anchor |
|---|---|---|
| `manuscript_phase` | Current manuscript lifecycle phase (`brainstorm`/`draft`/`review`/`finalize`/`published`). Used for downstream gate validation. | `bundle.schema.json`, `common.schema.json` `ManuscriptPhase` |
| `output_manuscript` | Whether this execution requires manuscript output. Defaults `true` for local, `false` for platform-hosted creators. | `story-manifest.schema.json` |
| `deltas[].payload.summary_text` | The generated summary text, embedded in the story_manifest delta payload. | `story-manifest.schema.json` `summary_text` |

**Dependency on sync-contract**:
- Context Assembly (restructured) depends on `sync-contract` for bundle envelope fields: `manuscript_phase`, `output_manuscript`, `submitting_creator_id`, `last_confirmed_delta_sequence`.
- The story_manifest delta type in `bundle.schema.json` carries the summary payload.
- Platform's Phase A/B pipeline handles persistence and async indexing (Phase A: persist to Postgres; Phase B: index into Neo4j/pgvector asynchronously).

---

## 4. Platform-Side Scope (Reference Only)

> **This section is for boundary clarification only.** No implementation in this repo.

### 4.1 Ownership Clarification

The full Context Assembly service — including Neo4j graph queries, pgvector similarity search, HybridRAG orchestration, query façade, and embedding generation — **belongs in the private `nexus-platform` repository**.

This is not a future decision; it is already frozen by:
- Context Assembly is implemented entirely in TypeScript
- Platform is responsible for receiving, validating, indexing, and providing Context Assembly retrieval

### 4.2 Integration Point

Platform receives CLI-generated summaries through the standard sync pipeline:
1. CLI pushes `story_manifest` delta with `summary_text` payload
2. Platform `SyncService` validates the bundle
3. Platform Phase A: persists to Postgres
4. Platform Phase B: `GraphProjectionService` and `MemoryService` index into Neo4j/pgvector asynchronously
5. Platform `ContextAssemblyService` serves assembled context via HTTPS API (proxied through Local API)

### 4.3 Not in This Repo

The following are explicitly **NOT** implemented in this (`nexus`) repository:

| Component | Reason |
|---|---|
| Neo4j client/driver | Platform-authoritative graph store |
| Postgres client (for platform stores) | Platform-authoritative relational store |
| pgvector client | Platform-authoritative vector store |
| HybridRAG query logic | Platform TypeScript service |
| Query façade (GraphQL / aggregation layer) | Platform read model |
| Embedding generation (OpenAI / local model) | Deferred to V1.1+ platform-side |
| Reranking (LLM-as-judge, cross-encoder) | Explicitly excluded from V1.0 |
| Docker Compose (Neo4j/Postgres/pgvector/Redis) | Platform infrastructure, not CLI dev dependency |

---

## 5. Task Breakdown (CLI-Side Implementation)

The following tasks are the **narrowed CLI-side scope** for this repo. Each task is self-contained and actionable.

### Task 1: Summary Generation Module

**Goal**: Implement basic summary extraction from local manuscript files.

**Files to create**:
- `crates/nexus42/src/context/summary.rs` — Summary generation logic
- `crates/nexus42/src/context/summary_test.rs` — Unit tests
- `crates/nexus42/src/context/mod.rs` — Module root

**Implementation**:
- [ ] Define `SummaryGenerator` struct with workspace path configuration
- [ ] Implement `scan_manuscript_dir(world_ref: &str) -> Vec<ManuscriptFile>` that walks `Stories/<world_ref>/`
- [ ] Implement `generate_basic_summary(files: Vec<ManuscriptFile>) -> String` (V1.0: title + chapter list + word count + opening excerpt, max 4096 chars)
- [ ] Parse `.md` front-matter for title extraction
- [ ] Parse heading structure for chapter boundary detection
- [ ] Extract first N chars of first chapter as opening excerpt
- [ ] Unit tests: valid manuscript dir, empty dir, single file, multi-chapter, non-markdown files ignored

**Verification**:
```bash
cargo test -p nexus42 context::summary
```

**Expected result**: All tests pass. Summary text is a plain string suitable for `StoryManifest.summary_text`.

**No dependencies on**: Neo4j, Postgres, pgvector, OpenAI, any model SDK.

### Task 2: Context Assembly Client (Local API Call)

**Goal**: Implement the HTTP client that calls `POST /v1/local/context/assemble` via `nexus42d`.

**Files to create**:
- `crates/nexus42/src/context/client.rs` — Platform Context Assembly HTTP client
- `crates/nexus42/src/context/client_test.rs` — Unit/integration tests
- `crates/nexus42/src/context/types.rs` — Request/response Rust types (generated or hand-written matching schema)

**Implementation**:
- [ ] Define `ContextAssembleRequest` and `ContextAssembleResponse` Rust structs matching `§3.3.1` and `§3.3.2` shapes
- [ ] Implement `ContextClient::assemble(request: ContextAssembleRequest) -> Result<ContextAssembleResponse, ContextError>`
- [ ] Client sends request to Local API endpoint (`POST /v1/local/context/assemble` on loopback)
- [ ] Handle error responses: `auth_expired`, `world_not_found`, `platform_unavailable`
- [ ] Parse `data_freshness_hint` for staleness detection
- [ ] Integration test with mock Local API server (wiremock or similar)
- [ ] Ensure request types are serializable/deserializable (serde)

**Verification**:
```bash
cargo test -p nexus42 context::client
```

**Expected result**: All tests pass. Client correctly constructs requests and parses responses per schema shapes.

**No dependencies on**: Neo4j, Postgres, pgvector. Only `reqwest` (or equivalent HTTP client) and `serde`.

### Task 3: Bundle Metadata Integration

**Goal**: Wire summary generation into the sync bundle pipeline so summaries are included in `story_manifest` deltas.

**Files to modify**:
- `crates/nexus42/src/sync/bundle_builder.rs` — (existing or new) Bundle construction logic
- `crates/nexus42/src/sync/builder_test.rs` — Tests

**Implementation**:
- [ ] After summary generation (Task 1), create a `story_manifest` delta entry
- [ ] Set `delta_type = "story_manifest"`, `operation = "upsert"`
- [ ] Include `summary_text` in delta payload
- [ ] Set bundle-level `manuscript_phase` from current workspace state
- [ ] Set bundle-level `output_manuscript` from workspace config
- [ ] Ensure `canonical_hash` covers the summary payload
- [ ] Unit tests: bundle with story_manifest delta, bundle without summary, hash consistency

**Verification**:
```bash
cargo test -p nexus42 sync::bundle_builder
```

**Expected result**: All tests pass. Sync bundles correctly include story_manifest deltas with summary_text.

### Task 4: CLI Command `nexus42 context assemble`

**Goal**: Wire the CLI command to the context assembly client.

**Files to modify**:
- `crates/nexus42/src/cli/context.rs` — CLI command handler
- `crates/nexus42/src/cli/context_test.rs` — Tests

**Implementation**:
- [ ] Implement `nexus42 context assemble` subcommand (aligned with §1.3 design decisions)
- [ ] Command reads `workspace_id`, `world_id`, `creator_id` from current workspace config
- [ ] Sends `ContextAssembleRequest` via Local API client (Task 2)
- [ ] Outputs assembled context as formatted JSON to stdout (or `--output <file>`)
- [ ] Handles degraded mode: if platform unavailable, print clear error with action suggestion
- [ ] Supports optional flags: `--include-memory`, `--include-timeline`, `--include-stories`
- [ ] Unit tests: command parsing, error handling

**Verification**:
```bash
cargo test -p nexus42 cli::context
cargo run -p nexus42 -- context assemble --help
```

**Expected result**: Help text displays correctly. Tests pass. Command outputs valid JSON matching response schema.

### Task 5: Schema Registration

**Goal**: Create the JSON Schema file for context assembly request/response types.

**Files to create**:
- `schemas/platform/context-assembly-v1.schema.json` — Request/response schema definitions

**Implementation**:
- [ ] Define `ContextAssembleRequestV1` (per §3.3.1)
- [ ] Define `ContextAssembleResponseV1` (per §3.3.2)
- [ ] Use `$ref` to common types from `schemas/common/common.schema.json`
- [ ] Validate schema compiles: `ajv validate -s meta.schema.json -d context-assembly-v1.schema.json` (or equivalent)
- [ ] Ensure codegen pipeline can consume it (TypeScript + Rust types)

**Verification**:
```bash
# If ajv is available:
npx ajv compile -s schemas/platform/context-assembly-v1.schema.json

# Or manual JSON validity check:
python3 -m json.tool schemas/platform/context-assembly-v1.schema.json > /dev/null
```

**Expected result**: Schema is valid JSON Schema Draft-07. All `$ref` targets exist.

---

## 6. Dependency on Sync Contract

The restructured Context Assembly (CLI-side) depends on the sync contract for the following:

1. **Bundle envelope fields**: `manuscript_phase`, `output_manuscript`, `submitting_creator_id`, `last_confirmed_delta_sequence` — these are defined in `bundle.schema.json` and must be present in the sync pipeline before context assembly can attach summaries to bundles.

2. **Delta type `story_manifest`**: Already defined in `bundle.schema.json` `deltas[].delta_type` enum. The context assembly summary is carried as a `story_manifest` delta payload.

3. **StoryManifest entity**: Defined in `story-manifest.schema.json`. The `summary_text` field is where CLI-generated summaries are stored. The `summary_unit_id` field is platform-assigned after indexing.

4. **Phase A/B pipeline**: Context Assembly depends on the platform's sync pipeline to persist summaries (Phase A) and index them (Phase B). CLI does not control this — it only pushes bundles and receives assembled context via Local API.

5. **Prerequisite**: The `sync-contract` plan (`.agents/plans/2025-04-05-sync-contract`) must be at least partially complete (bundle envelope + story_manifest delta support) before Task 3 (Bundle Metadata Integration) can be implemented.

---

## 7. Removed from Original Plan

The following items from the original plan (`.agents/plans/2025-04-05-context-assembly.md`) are explicitly **removed** from CLI-side scope:

| Removed Item | Reason | New Owner |
|---|---|---|
| `crates/nexus-context/` Rust crate | Tech stack conflict (§3.1: all TypeScript) | N/A — restructured as thin CLI module |
| Neo4j client wrapper (`neo4rs`) | Storage ownership conflict (§5.2: platform-authoritative) | `nexus-platform` |
| Postgres/pgvector client wrapper (`sqlx`) | Storage ownership conflict | `nexus-platform` |
| Graph storage (Timeline, KB in Neo4j) | Responsibility split violation | `nexus-platform` |
| Vector storage (Memory embeddings in pgvector) | Responsibility split violation | `nexus-platform` |
| Embedding generation (OpenAI / local model) | Over-scoped for V1.0 (§3.4 excludes) | `nexus-platform` (V1.1+) |
| HybridRAG query logic | Responsibility split violation | `nexus-platform` |
| Query façade / reranking | Over-scoped for V1.0 | `nexus-platform` (V1.1+) |
| `docker-compose.yml` (Neo4j + Postgres + pgvector + Redis) | Docker misalignment (CLI uses SQLite only) | `nexus-platform` dev infra |
| `.env.example` (database connection strings) | No direct database access from CLI | `nexus-platform` |

---

## 8. Implementation Effort (Agent-Oriented)

- **Complexity**: **M** (medium)
- **Agent session band**: ~1–2 focused agent sessions for the restructured spec output. Actual implementation (Tasks 1–5) is **S** per task individually but **M** collectively.
- **Prerequisites**: `sync-contract` plan must deliver bundle envelope + story_manifest delta support before Task 3.

---

## 9. Acceptance Criteria Verification

| Criterion | Status | Evidence |
|---|---|---|
| CLI-side scope vs Platform-side scope clearly separated | ✅ | §2 Responsibility Matrix + Architecture Diagram |
| CLI-side scope has NO Neo4j/Postgres/pgvector direct access | ✅ | §2.3 Strict Boundaries + §7 Removed Items |
| `POST /v1/local/context/assemble` request/response shapes defined | ✅ | §3.3.1 + §3.3.2 |
| Summary generation logic integrates with sync-contract bundle metadata | ✅ | §3.4 Bundle Metadata Integration + §6 Dependency on Sync Contract |
| Task breakdown is actionable (file paths, test commands, expected results) | ✅ | §5 Tasks 1–5 with verification commands |
| Aligns with tech stack freeze (TypeScript platform-side, no Rust Context Assembly crate) | ✅ | §7 Removed Items + §1.1 Conflict Resolution table |

---

*Document generated as input for restructuring `.agents/plans/2025-04-05-context-assembly.md`. Not a direct modification of the plan file.*

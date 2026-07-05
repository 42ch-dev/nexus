# Architect Note: C0/J0 Injection Design for V1.31 Capabilities

**Agent**: architect  
**Plans**: `2026-05-30-v1.31-creator-memory-capabilities` + `2026-05-30-v1.31-judge-and-summarize-capabilities`  
**Branch**: `feature/v1.31-agentic-design-patterns`  
**Date**: 2026-05-30

## Scope

V1.31 de-stubs orchestration capabilities while preserving two locked compass invariants:

1. Runtime dependencies enter built-in capabilities through `CapabilityRegistry` factories, matching `KbExtractWork::with_pool`.
2. Capability-layer LLM calls use ACP worker IPC (`worker/acp_prompt`), not direct model/provider crates.

Mandatory source review covered the registry factory, creator/judge/summarize/acp builtins, `KbExtractWork`, task worker IPC, `nexus-creator-memory`, and relevant crate `AGENTS.md` files.

---

## C0 — Registry Factory Injection for Creator Capabilities

### Problem statement

`creator.read_memory`, `creator.write_memory`, and `creator.inject_prompt` are currently stateless stubs. They need creator-scoped memory and queue persistence without introducing process globals or making preset YAML repeat all runtime context.

### Options considered

1. **Inject `SqlitePool` directly into each creator capability** — smallest change and mirrors `KbExtractWork`, but duplicates memory/identity/error logic.
2. **Inject a small pool-backed creator capability store** — centralizes identity resolution, SQL calls, and error mapping while keeping capabilities as input/output adapters.
3. **Pass `home_dir` in every capability input and call file-backed memory APIs** — uses existing `nexus-creator-memory::memory_io`, but leaks filesystem layout into presets and does not meet the SQLite queue requirement.

### Recommended approach

Use option 2: introduce an orchestration-side adapter backed by `Arc<SqlitePool>`, then inject that adapter through the registry factory. Keep `with_builtins()` as explicit standalone/test placeholder mode. Extend `with_builtins_and_pool` for P1 and add a superset `with_runtime_deps` for P2.

```rust
pub struct CreatorCapabilityStore {
    pool: std::sync::Arc<sqlx::SqlitePool>,
}

impl CreatorCapabilityStore {
    pub fn new(pool: sqlx::SqlitePool) -> Self;
    pub fn from_arc(pool: std::sync::Arc<sqlx::SqlitePool>) -> Self;

    pub async fn resolve_creator_id(
        &self,
        input: &serde_json::Value,
    ) -> Result<String, CapabilityError>;

    pub async fn read_memory(
        &self,
        creator_id: &str,
        keyword: Option<&str>,
        limit: u32,
    ) -> Result<CreatorMemoryReadResult, CapabilityError>;

    pub async fn write_memory(
        &self,
        creator_id: &str,
        content: &str,
        keywords: &[String],
        source_session_id: Option<&str>,
    ) -> Result<String, CapabilityError>;

    pub async fn enqueue_prompt(
        &self,
        item: InjectPromptQueueItem,
    ) -> Result<(), CapabilityError>;

    pub async fn drain_prompt_queue(
        &self,
        creator_id: &str,
        session_id: &str,
        limit: u32,
    ) -> Result<Vec<InjectPromptQueueItem>, CapabilityError>;
}
```

Capability constructors should mirror `KbExtractWork::new()` / `with_pool()`:

```rust
pub struct CreatorReadMemory {
    store: Option<std::sync::Arc<CreatorCapabilityStore>>,
}

impl CreatorReadMemory {
    pub const fn new() -> Self;
    pub fn with_store(store: std::sync::Arc<CreatorCapabilityStore>) -> Self;
}

pub struct CreatorWriteMemory {
    store: Option<std::sync::Arc<CreatorCapabilityStore>>,
}

pub struct CreatorInjectPrompt {
    store: Option<std::sync::Arc<CreatorCapabilityStore>>,
}
```

Registry sketch:

```rust
pub struct CapabilityRuntimeDeps {
    pub pool: sqlx::SqlitePool,
    pub worker_handles: Option<std::sync::Arc<dyn WorkerHandleProvider>>,
}

impl CapabilityRegistry {
    pub fn with_builtins_and_pool(pool: sqlx::SqlitePool) -> Self {
        let creator_store = std::sync::Arc::new(CreatorCapabilityStore::new(pool.clone()));
        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::CreatorReadMemory::with_store(creator_store.clone())),
            Box::new(builtins::CreatorWriteMemory::with_store(creator_store.clone())),
            Box::new(builtins::CreatorInjectPrompt::with_store(creator_store)),
            Box::new(builtins::KbExtractWork::with_pool(pool)),
            // unchanged stateless built-ins
        ];
        Self { capabilities: caps }
    }

    pub fn with_runtime_deps(deps: CapabilityRuntimeDeps) -> Self;
}
```

### Creator identity resolution

Do not silently read a global creator. Resolve in this order:

1. Capability input `creator_id` once schemas/contracts add it.
2. Capability input `_creator_id`, injected by task execution from schedule/session context.
3. Input `schedule_id`: look up `creator_schedules.creator_id` in `state.db`.
4. Input `session_id`: look up `orchestration_sessions.creator_id` in `state.db`.
5. Otherwise return `CapabilityError::InputInvalid("missing creator identity")`.

Current generated DTOs for creator capabilities do not include `creator_id`; P1 should either update schemas/contracts and regenerate, or inspect the original `serde_json::Value` envelope for `_creator_id`, `schedule_id`, and `session_id` before deserializing stable DTO fields.

Recommended execution-layer behavior: `CapabilityTask`/`StateCompositeTask` should enrich `_capability_input` with `_creator_id`, `_session_id`, and `_schedule_id` when those values exist in `graph_flow::Context`. Preset authors should not have to repeat runtime identity on every enter action.

### Risks and trade-offs

- The adapter is an extra type, but prevents three separate capabilities from learning SQL and creator-context rules independently.
- `nexus-creator-memory` currently exposes substantial file-backed memory APIs. For SQLite-backed V1.31 tests, add missing persistence functions in `nexus-local-db` and keep domain construction/conversion in `nexus-creator-memory`.
- Existing `with_builtins()` tests will continue to exercise placeholder mode; production daemon boot must switch from `with_builtins()` to a pool-backed factory.

---

## C0 — `creator.inject_prompt` Queue Design

### Problem statement

`creator.inject_prompt` must enqueue text that is consumed by the next `acp.prompt` on the same creator session. Queue behavior must be restart-safe and deterministic under per-creator schedule concurrency.

### Options considered

1. **SQLite queue in `state.db`** — durable, observable, testable, and naturally scoped by creator/session.
2. **In-worker `VecDeque`** — simple and colocated with ACP sessions, but lost on worker restart and hard to inspect/recover.

### Recommended approach

Use a per-creator/per-session SQLite queue in `state.db`. Prompt injection is orchestration state, not model state.

Migration sketch:

```sql
CREATE TABLE IF NOT EXISTS creator_prompt_injections (
  injection_id   TEXT PRIMARY KEY,
  creator_id     TEXT NOT NULL,
  session_id     TEXT NOT NULL,
  prompt         TEXT NOT NULL,
  priority       INTEGER NOT NULL DEFAULT 0,
  status         TEXT NOT NULL, -- queued | claimed | consumed | expired
  created_at     INTEGER NOT NULL,
  claimed_at     INTEGER,
  consumed_at    INTEGER,
  expires_at     INTEGER,
  source_schedule_id TEXT,
  source_capability_call_id TEXT,
  metadata_json  BLOB
);

CREATE INDEX IF NOT EXISTS creator_prompt_injections_next
  ON creator_prompt_injections(creator_id, session_id, status, priority DESC, created_at ASC);

CREATE INDEX IF NOT EXISTS creator_prompt_injections_cleanup
  ON creator_prompt_injections(status, expires_at);
```

Local-db API sketch:

```rust
pub struct PromptInjectionRow {
    pub injection_id: String,
    pub creator_id: String,
    pub session_id: String,
    pub prompt: String,
    pub priority: i64,
    pub status: String,
    pub created_at: i64,
}

pub async fn enqueue_prompt_injection(
    pool: &sqlx::SqlitePool,
    new: NewPromptInjection<'_>,
) -> Result<PromptInjectionRow, sqlx::Error>;

pub async fn claim_prompt_injections(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    session_id: &str,
    limit: i64,
    now: i64,
) -> Result<Vec<PromptInjectionRow>, sqlx::Error>;

pub async fn mark_prompt_injections_consumed(
    pool: &sqlx::SqlitePool,
    injection_ids: &[String],
    now: i64,
) -> Result<u64, sqlx::Error>;
```

### Enqueue semantics

`CreatorInjectPrompt::run`:

1. Validate `prompt.trim()` is non-empty.
2. Resolve `creator_id` using the C0 identity order.
3. Resolve `session_id` from input `session_id`, then `_session_id`, then `creator_schedules.current_session_id` when `schedule_id` is present, otherwise use `"default"` only in explicit standalone/test mode.
4. Insert one `queued` row with priority, source schedule/call metadata, and an optional expiry.
5. Return `{ "queued": true }` only after durable insert succeeds.

### Consumption point

Consumption belongs immediately before `worker/acp_prompt` dispatch on the real IPC path:

- `AcpPromptTask::run` renders its normal prompt.
- It resolves `creator_id` and `session_id` from context/task construction.
- It claims queued rows ordered by `priority DESC, created_at ASC`.
- It prepends them using a fixed delimiter before sending IPC, for example:

```text
System-injected context for this creator session:
<queued prompt 1>

<queued prompt 2>

--- User/task prompt ---
<rendered task prompt>
```

- It marks claimed rows `consumed` only after `worker/acp_prompt` returns a final response. If IPC fails, leave rows `claimed` and let recovery either requeue stale claims or mark them expired.

`creator.inject_prompt` should not directly call the worker.

### Queue lifecycle and cleanup

- `queued`: inserted and eligible for next matching `creator_id` + `session_id` prompt.
- `claimed`: selected for a dispatch attempt; includes `claimed_at`.
- `consumed`: dispatch succeeded; retained for short audit/debug window.
- `expired`: TTL elapsed or stale `claimed` row exceeded retry window.

Cleanup can run during daemon maintenance or opportunistically before enqueue/drain:

- expire `queued` rows where `expires_at <= now`;
- requeue or expire `claimed` rows older than a small retry window;
- delete old `consumed`/`expired` rows after a retention window.

### Risks and trade-offs

- SQLite adds migration/query work, but prevents lost injections on daemon/worker restart.
- Marking consumed only after IPC success may duplicate prompts if the worker performed the action but the final response was lost. This is acceptable pre-1.0; later trace IDs can make consumption exactly-once from the orchestration perspective.

---

## J0 — Shared Worker-Handle Accessor for LLM Capabilities

### Problem statement

`AcpPromptTask` already has a worker IPC path, but `AcpPrompt`, `JudgeLlm`, and `ContextSummarize` capability implementations still return placeholders or heuristics when invoked through `CapabilityTask`. P2 needs capability-layer LLM calls to use the same worker IPC path when a worker handle is present.

### Options considered

1. **Pass raw `WorkerHandle` through capability input** — awkward because `Capability::run` accepts JSON `Value`, and `WorkerHandle` is not serializable.
2. **Have registry hold a shared worker-handle provider** — clean constructor injection and compatible with `Capability::run(Value)`.
3. **Only support task-level LLM IPC** — leaves `judge.llm` and `context.summarize` as stubs when called as capabilities, failing V1.31 acceptance.

### Recommended approach

Use option 2. Define a provider trait in `nexus-orchestration` and inject `Arc<dyn WorkerHandleProvider>` through `CapabilityRegistry::with_runtime_deps`.

The provider should not return a borrowed `&WorkerHandle` because async call sites need to avoid holding locks across `.await`. Prefer a closure-style API that executes the JSON-RPC call while the provider owns synchronization.

```rust
#[async_trait::async_trait]
pub trait WorkerHandleProvider: Send + Sync {
    async fn call_acp_prompt(
        &self,
        creator_id: &str,
        session_id: &str,
        prompt: String,
        tool_policy: ToolPolicy,
    ) -> Result<serde_json::Value, CapabilityError>;
}
```

For existing `AcpPromptTask` compatibility, an adapter can wrap the current shared handle type:

```rust
pub type SharedWorkerHandle =
    std::sync::Arc<std::sync::Mutex<Option<crate::worker::WorkerHandle>>>;

pub struct SingleWorkerHandleProvider {
    handle: SharedWorkerHandle,
}

#[async_trait::async_trait]
impl WorkerHandleProvider for SingleWorkerHandleProvider {
    async fn call_acp_prompt(
        &self,
        creator_id: &str,
        session_id: &str,
        prompt: String,
        tool_policy: ToolPolicy,
    ) -> Result<serde_json::Value, CapabilityError> {
        let params = serde_json::json!({
            "creator_id": creator_id,
            "session_id": session_id,
            "prompt": prompt,
            "tool_policy": tool_policy.as_str(),
        });
        // Take/reinsert handle like AcpPromptTask today, or migrate to async Mutex.
        // Call method: "worker/acp_prompt".
    }
}
```

Longer term, a provider backed by `WorkerRegistry<WorkerManagerSpawner>` can use `creator_id` to get or spawn the correct worker. For V1.31, reuse the daemon boot bridge available near `WorkspaceState::set_worker_manager` and registry construction.

Capability constructors:

```rust
pub struct JudgeLlm {
    workers: Option<std::sync::Arc<dyn WorkerHandleProvider>>,
}

impl JudgeLlm {
    pub const fn new() -> Self;
    pub fn with_worker_provider(provider: std::sync::Arc<dyn WorkerHandleProvider>) -> Self;
}

pub struct ContextSummarize {
    workers: Option<std::sync::Arc<dyn WorkerHandleProvider>>,
}

pub struct AcpPrompt {
    workers: Option<std::sync::Arc<dyn WorkerHandleProvider>>,
}
```

`JudgeLlm::run` should build the judge prompt, call `call_acp_prompt(..., ToolPolicy::DenyAll)`, parse the returned `full_text` with the existing GO/NOGO vocabulary, and return `CapabilityError::InputInvalid` for ambiguous verdicts unless the plan explicitly keeps conservative false-on-ambiguous behavior.

`ContextSummarize::run` should build a summarization prompt from `content`, `trace`, and `template`, call `call_acp_prompt(..., ToolPolicy::DenyAll)`, return `summary = full_text`, and compute `prompt_hash` from the prompt actually sent.

`AcpPrompt::run` can use the same provider in standalone capability mode when `creator_id`/`session_id` is available. If no provider is injected, it should clearly report standalone mode, not pretend production dispatch happened.

### Risks and trade-offs

- A provider trait is an abstraction, but avoids serializing `WorkerHandle` through JSON and gives tests a mockable seam.
- The current `Arc<Mutex<Option<WorkerHandle>>>` pattern works but is cumbersome. If P2 touches it, prefer a single shared alias and helper to avoid duplicating take/reinsert logic.
- Daemon boot currently constructs `CapabilityRegistry::with_builtins()` before creating `WorkerManager`; P2 must reorder or add a second registry construction point so both pool and worker provider are available before graph execution.

---

## J0 — DF-37 Stub Fallback Reduction Strategy

### Problem statement

DF-37 requires that worker-backed paths use IPC whenever a worker handle is present. Current fallback behavior exists in three places:

- `AcpPrompt::run` returns `dispatched_via_ipc: false` prepared output.
- `AcpPromptTask::run` returns `[acp_prompt stub: ...]` when `worker_handle` is `None`.
- `InnerGraphNodeTask::run` returns `inner_node:<id>:stub_output` when `worker_handle` is `None`.

### Recommended approach

Document and enforce two modes:

1. **Production/wired mode**: registry and graph tasks have a worker provider/handle. `judge.llm`, `context.summarize`, `AcpPrompt`, `AcpPromptTask`, and `InnerGraphNodeTask` must call IPC.
2. **Standalone/test mode**: no worker provider/handle was intentionally supplied. Stub outputs are allowed only in tests or direct capability smoke usage.

Concrete changes for implementers:

- Add explicit constructor names for test mode, e.g. `AcpPromptTask::new_stub_for_test(...)` or clear docs on `new(None, ...)`.
- Add a context flag only for intentional stub mode if needed, e.g. `_allow_worker_stub = true`; otherwise missing worker in a production graph should produce a task status error.
- Extend `build_wired_outer_graph` or adjacent runtime wiring so inner graph nodes receive the worker handle/provider once available. `build_inner_graphs()` currently creates `InnerGraphNodeTask::new(...).with_agent_ref(...).with_tool_policy(...).with_template(...)` with no worker handle, so a follow-up wiring pass or loader parameter is required.
- When `InnerGraphNodeTask` has a worker handle, it already delegates to `AcpPromptTask`; keep that path and make it the production default.
- When `AcpPromptTask` has a worker handle, always send `worker/acp_prompt` with `prompt`, `tool_policy`, and `session_id` and store `state.<state_id>.output` from `full_text`.

Suggested signature changes:

```rust
pub fn build_wired_outer_graph(
    loaded: &LoadedPreset,
    engine: &std::sync::Arc<dyn OrchestrationEngine>,
    caps: &std::sync::Arc<CapabilityRegistry>,
    worker_provider: Option<std::sync::Arc<dyn WorkerHandleProvider>>,
) -> graph_flow::Graph;

impl InnerGraphNodeTask {
    pub fn with_worker_provider(
        self,
        provider: std::sync::Arc<dyn WorkerHandleProvider>,
    ) -> Self;
}
```

If changing `build_wired_outer_graph` is too wide for P2, use an adjacent `build_wired_outer_graph_with_runtime(...)` and keep the old function as a documented test/standalone helper.

### Risks and trade-offs

- Tightening missing-worker behavior may break tests that implicitly relied on stubs. Rename or update those tests to opt into standalone mode.
- Full worker-registry integration may be larger than P2; a single-worker provider still satisfies the first IPC-backed acceptance path and leaves multi-creator spawning to a later hardening pass.
- Prompt-injection consumption should live in the IPC path; if `AcpPrompt::run` gains provider support, it must share the same drain/prepend helper as `AcpPromptTask` to avoid divergent behavior.

---

## Implementation Notes for P1/P2 Handoff

- P1 should add the SQLite prompt-injection migration and local-db API, then wire creator capabilities through `CreatorCapabilityStore`.
- P2 should add `WorkerHandleProvider`, inject it through `CapabilityRegistry::with_runtime_deps`, and convert `JudgeLlm`/`ContextSummarize` to call `worker/acp_prompt` when present.
- P1/P2 should coordinate on a shared helper for resolving `creator_id` and `session_id` from capability input/context to avoid two incompatible conventions.
- Tests should explicitly cover both modes: injected runtime dependencies use real persistence/IPC mocks; standalone constructors preserve deterministic smoke tests.

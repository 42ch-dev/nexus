//! Capability trait + registry.
//!
//! Design: `.mstar/knowledge/specs/orchestration-engine.md` §5.1–5.2.

pub mod builtins;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by capability execution.
#[derive(Error, Debug)]
pub enum CapabilityError {
    #[error("invalid input: {0}")]
    InputInvalid(String),
    #[error("transient external error: {0}")]
    TransientExternal(String),
    #[error("permanent external error: {0}")]
    PermanentExternal(String),
    #[error("worker unavailable")]
    WorkerUnavailable,
    #[error("ACP session lost")]
    AcpSessionLost,
    #[error("cancelled")]
    Cancelled,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("internal error: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// WorkerHandleProvider — injected LLM call seam (J0)
// ---------------------------------------------------------------------------

/// Provider trait for capability-layer LLM calls via worker IPC.
///
/// Injected into `JudgeLlm`, `ContextSummarize`, and `AcpPrompt` through
/// the registry factory. When absent, capabilities operate in standalone/
/// test mode with clear error messages.
///
/// Design: architect note C0/J0, `KbExtractWork::with_pool` pattern.
#[async_trait]
pub trait WorkerHandleProvider: Send + Sync {
    /// Call `worker/acp_prompt` via IPC.
    ///
    /// Returns the full JSON-RPC response value on success.
    async fn call_acp_prompt(
        &self,
        creator_id: &str,
        session_id: &str,
        prompt: String,
        tool_policy: &str,
    ) -> Result<Value, CapabilityError>;
}

/// Provider trait for daemon-side `nexus.*` tool dispatch (DF-47 production wiring).
///
/// Implemented by `nexus-daemon-runtime` using `HostToolExecutor::dispatch_from_worker`.
/// Injected into `HostToolCallTask` so the orchestration engine can invoke
/// `nexus.*` tools on a schedule tick without worker IPC round-trip.
///
/// Design: `agent-nexus-tool-bridge.md` §7.4, V1.42 P3.
#[async_trait]
pub trait DaemonToolDispatch: Send + Sync {
    /// Dispatch a `nexus.*` tool call through the daemon's unified registry.
    ///
    /// Returns the tool result JSON on success, or a `CapabilityError` on failure.
    async fn dispatch_tool(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        request_id: &str,
    ) -> Result<serde_json::Value, CapabilityError>;
}

/// Runtime dependencies injected through `CapabilityRegistry::with_runtime_deps`.
///
/// Groups pool and worker provider so daemon boot can construct a single
/// struct and pass it to the registry factory.
pub struct CapabilityRuntimeDeps {
    /// Pool for pool-backed capabilities (`kb.extract_work`, etc.).
    pub pool: Option<sqlx::SqlitePool>,
    /// Worker handle provider for LLM-backed capabilities (`judge.llm`,
    /// `context.summarize`, `acp.prompt`, `nexus.llm.extract`).
    pub worker_provider: Option<std::sync::Arc<dyn WorkerHandleProvider>>,
    /// Daemon-side tool dispatch for `nexus.*` tools (DF-47, V1.42 P3).
    pub daemon_tool_dispatch: Option<std::sync::Arc<dyn DaemonToolDispatch>>,
    /// CDN fetch config for `registry.refresh` (V1.57 P1 — constructor-injected).
    pub cdn_config: Option<builtins::CdnConfig>,
}

// ---------------------------------------------------------------------------
// Capability trait
// ---------------------------------------------------------------------------

/// A capability that can be invoked as a graph-flow Task node.
///
/// Per the design spec, every capability ships its own input/output JSON Schema
/// as `&'static str` constants. These are **local** types, not wire contracts.
#[async_trait]
pub trait Capability: Send + Sync {
    /// Dot-separated capability name, e.g. `"sync.pull"`.
    fn name(&self) -> &'static str;

    /// JSON Schema (draft 2020-12) describing valid inputs.
    fn input_schema(&self) -> &'static str;

    /// JSON Schema (draft 2020-12) describing the output shape.
    fn output_schema(&self) -> &'static str;

    /// Execute the capability with the given input.
    ///
    /// Returns a JSON `Value` on success or a [`CapabilityError`].
    async fn run(&self, input: Value) -> Result<Value, CapabilityError>;
}

// ---------------------------------------------------------------------------
// CapabilityRegistry
// ---------------------------------------------------------------------------

/// Registry of available capabilities. Built once at daemon startup.
///
/// Capabilities are stored in a `Vec` for ordered iteration, with a `HashMap`
/// index for O(1) lookup by name (built lazily on first `get()` call).
pub struct CapabilityRegistry {
    capabilities: Vec<Box<dyn Capability>>,
    /// Lazy index: `name` → position in `capabilities`. Built on first lookup.
    index: Option<std::collections::HashMap<&'static str, usize>>,
}

impl CapabilityRegistry {
    /// Create a registry pre-populated with all built-in capabilities.
    ///
    /// Built-ins: `sync.pull`, `sync.push`, `outbox.flush`, `outbox.compact`,
    /// `workspace.open`, `workspace.commit`, `registry.refresh`,
    /// `creator.read_memory`, `creator.write_memory`, `creator.inject_prompt`,
    /// `creator.write_brief`, `judge.rule`, `acp.prompt`, `acp.session_load`,
    /// `judge.llm`, `context.summarize`, `kb.extract_work`,
    /// `nexus.llm.extract`, `soul.experience.aggregate`,
    /// `narrative.compute`.
    ///
    /// `kb.extract_work` is created without a pool (placeholder mode).
    /// Use [`with_builtins_and_pool`] for full e2e support.
    #[must_use]
    pub fn with_builtins() -> Self {
        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(builtins::OutboxFlush::new()),
            Box::new(builtins::OutboxCompact::new()),
            Box::new(builtins::WorkspaceOpen),
            Box::new(builtins::WorkspaceCommit),
            Box::new(builtins::RegistryRefresh::new()),
            Box::new(builtins::CreatorReadMemory::new()),
            Box::new(builtins::CreatorWriteMemory::new()),
            Box::new(builtins::CreatorInjectPrompt::new()),
            Box::new(builtins::CreatorWriteBrief::new()),
            Box::new(builtins::JudgeRule),
            Box::new(builtins::AcpPrompt::new()),
            Box::new(builtins::AcpSessionLoad),
            Box::new(builtins::JudgeLlm::new()),
            Box::new(builtins::ContextSummarize::new()),
            Box::new(builtins::KbExtractWork::new()),
            Box::new(builtins::LlmExtract::new()),
            Box::new(builtins::SoulExperienceAggregate),
            // F6 (C-001): register novel.project_scaffold in the
            // pool-less registry so embedded preset validation can
            // resolve it. The pool-bound variant is registered via
            // [`with_builtins_and_pool`] for runtime use.
            Box::new(builtins::NovelProjectScaffold::new()),
            // P3 (T3): register novel.chapter_transition for chapter
            // status transitions (DB + frontmatter).
            Box::new(builtins::NovelChapterTransition::new()),
            // V1.52 T-A P2: register essay.project_scaffold for
            // embedded preset validation.
            Box::new(builtins::EssayProjectScaffold::new()),
            // V1.63 P2: register essay.draft_status.finalize for
            // essay-writing preset finalize_commit state.
            Box::new(builtins::EssayDraftStatusFinalize::new()),
            // V1.54 P1: register game_bible.project_scaffold for
            // embedded preset validation.
            Box::new(builtins::GameBibleProjectScaffold::new()),
            // V1.55 P3: register script.project_scaffold for
            // embedded preset validation.
            Box::new(builtins::ScriptProjectScaffold::new()),
            // V1.56 P-last R-V155P2-F002: game_bible.section_status.update
            Box::new(builtins::GameBibleSectionStatusUpdate::new()),
            // V1.67 P2 (R-V160P1-QC1-W001): script.section_status.update
            Box::new(builtins::ScriptSectionStatusUpdate::new()),
            // V1.58 P1: nexus.reference.refresh (pool-less; returns WorkerUnavailable)
            Box::new(builtins::ReferenceRefresh::new()),
            // V1.60 P0: DF-46 local parity — 5 orchestration-scope capabilities
            // (pool-less; return WorkerUnavailable without a pool).
            Box::new(builtins::WorldStateQuery::new()),
            Box::new(builtins::WorldDeltaPropose::new()),
            Box::new(builtins::WorldDeltaApply::new()),
            Box::new(builtins::TimelineEventAppend::new()),
            Box::new(builtins::ForkCreate::new()),
            // V1.61 P3: narrative.compute — sandboxed WASM compute for world state.
            Box::new(builtins::NarrativeCompute::new()),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create a registry with built-in capabilities and a pool.
    ///
    /// Same as [`with_builtins`] but `kb.extract_work` receives the pool
    /// for full e2e lifecycle, and all creator capabilities receive a
    /// [`builtins::CreatorCapabilityStore`] for real memory I/O
    /// and prompt injection persistence.
    #[must_use]
    pub fn with_builtins_and_pool(pool: sqlx::SqlitePool) -> Self {
        let creator_store = Arc::new(builtins::CreatorCapabilityStore::new(pool.clone()));
        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(builtins::OutboxFlush::with_pool(pool.clone())),
            Box::new(builtins::OutboxCompact::with_pool(pool.clone())),
            Box::new(builtins::WorkspaceOpen),
            Box::new(builtins::WorkspaceCommit),
            Box::new(builtins::RegistryRefresh::new()),
            Box::new(builtins::CreatorReadMemory::with_store(
                creator_store.clone(),
            )),
            Box::new(builtins::CreatorWriteMemory::with_store(
                creator_store.clone(),
            )),
            Box::new(builtins::CreatorInjectPrompt::with_store(
                creator_store.clone(),
            )),
            Box::new(builtins::CreatorWriteBrief::with_store(creator_store)),
            Box::new(builtins::JudgeRule),
            Box::new(builtins::AcpPrompt::new()),
            Box::new(builtins::AcpSessionLoad),
            Box::new(builtins::JudgeLlm::new()),
            Box::new(builtins::ContextSummarize::new()),
            Box::new(builtins::KbExtractWork::with_pool(pool.clone())),
            Box::new(builtins::LlmExtract::new()),
            Box::new(builtins::SoulExperienceAggregate),
            Box::new(builtins::NovelProjectScaffold::with_pool(pool.clone())),
            Box::new(builtins::NovelChapterTransition::with_pool(pool.clone())),
            // V1.52 T-A P2: essay.project_scaffold with pool.
            Box::new(builtins::EssayProjectScaffold::with_pool(pool.clone())),
            // V1.63 P2: essay.draft_status.finalize (pool-less; FS-only).
            Box::new(builtins::EssayDraftStatusFinalize::new()),
            // V1.54 P1: game_bible.project_scaffold with pool.
            Box::new(builtins::GameBibleProjectScaffold::with_pool(pool.clone())),
            // V1.55 P3: script.project_scaffold with pool.
            Box::new(builtins::ScriptProjectScaffold::with_pool(pool.clone())),
            // V1.56 P-last R-V155P2-F002: game_bible.section_status.update
            Box::new(builtins::GameBibleSectionStatusUpdate::new()),
            // V1.67 P2 (R-V160P1-QC1-W001): script.section_status.update
            Box::new(builtins::ScriptSectionStatusUpdate::new()),
            // V1.58 P1: nexus.reference.refresh with pool
            Box::new(builtins::ReferenceRefresh::with_pool(pool.clone())),
            // V1.60 P0: DF-46 local parity — 5 orchestration-scope capabilities.
            Box::new(builtins::WorldStateQuery::with_pool(pool.clone())),
            Box::new(builtins::WorldDeltaPropose::with_pool(pool.clone())),
            Box::new(builtins::WorldDeltaApply::with_pool(pool.clone())),
            Box::new(builtins::TimelineEventAppend::with_pool(pool.clone())),
            Box::new(builtins::ForkCreate::with_pool(pool.clone())),
            // V1.61 P3: narrative.compute with pool.
            Box::new(builtins::NarrativeCompute::with_pool(pool)),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create a registry with runtime dependencies injected.
    ///
    /// Production daemon boot should use this constructor when both a pool
    /// and worker provider are available. Capabilities without runtime deps
    /// are constructed in their default (standalone) form.
    ///
    /// `narrative.compute` is constructed via [`builtins::NarrativeCompute::with_pool`],
    /// which builds its own `WasmEngine` + per-instance module cache. For the
    /// daemon-wide singleton engine + cache (P-last T1, closes
    /// R-V161P3-PERF-001), use [`CapabilityRegistry::with_runtime_deps_and_wasm`].
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn with_runtime_deps(deps: &CapabilityRuntimeDeps) -> Self {
        let narrative_compute = deps
            .pool
            .as_ref()
            .map_or_else(builtins::NarrativeCompute::new, |pool| {
                builtins::NarrativeCompute::with_pool(pool.clone())
            });
        Self::build_with_narrative_compute(deps, narrative_compute)
    }

    /// Create a registry with runtime dependencies **and** a daemon-wide
    /// singleton `WasmEngine` + `ModuleCache` injected into `narrative.compute`
    /// (P-last T1/T4 — closes R-V161P3-PERF-001/002).
    ///
    /// The daemon builds exactly one engine + one cache at boot (pre-warmed
    /// with embedded and user-installed modules) and passes them here so module
    /// compilation happens once process-wide and is reused by every compute
    /// invocation. When `deps.pool` is absent, `narrative.compute` falls back
    /// to its standalone (`WorkerUnavailable`) form.
    #[must_use]
    pub fn with_runtime_deps_and_wasm(
        deps: &CapabilityRuntimeDeps,
        engine: std::sync::Arc<nexus_wasm_host::WasmEngine>,
        module_cache: std::sync::Arc<nexus_wasm_host::ModuleCache>,
    ) -> Self {
        let narrative_compute =
            deps.pool
                .as_ref()
                .map_or_else(builtins::NarrativeCompute::new, |pool| {
                    builtins::NarrativeCompute::with_pool_and_engine(
                        pool.clone(),
                        engine,
                        module_cache,
                    )
                });
        Self::build_with_narrative_compute(deps, narrative_compute)
    }

    /// Shared body of [`with_runtime_deps`] / [`with_runtime_deps_and_wasm`],
    /// parameterized only by the `narrative.compute` instance to register.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    fn build_with_narrative_compute(
        deps: &CapabilityRuntimeDeps,
        narrative_compute: builtins::NarrativeCompute,
    ) -> Self {
        let kb = deps
            .pool
            .as_ref()
            .map_or_else(builtins::KbExtractWork::new, |pool| {
                builtins::KbExtractWork::with_pool(pool.clone())
            });

        let judge_llm = deps
            .worker_provider
            .as_ref()
            .map_or_else(builtins::JudgeLlm::new, |provider| {
                builtins::JudgeLlm::with_worker_provider(provider.clone())
            });

        let context_summarize = deps
            .worker_provider
            .as_ref()
            .map_or_else(builtins::ContextSummarize::new, |provider| {
                builtins::ContextSummarize::with_worker_provider(provider.clone())
            });

        // V1.51 T-A P0: nexus.llm.extract reuses the same worker pool as
        // judge.llm / context.summarize / acp.prompt (compass §0.1 #7).
        let llm_extract = deps
            .worker_provider
            .as_ref()
            .map_or_else(builtins::LlmExtract::new, |provider| {
                builtins::LlmExtract::with_worker_provider(provider.clone())
            });

        let acp_prompt = deps
            .worker_provider
            .as_ref()
            .map_or_else(builtins::AcpPrompt::new, |provider| {
                builtins::AcpPrompt::with_worker_provider(provider.clone())
            });

        // V1.57 P1: cdn_config is constructor-injected (no global state).
        let registry_refresh = deps
            .cdn_config
            .as_ref()
            .map_or_else(builtins::RegistryRefresh::new, |cdn| {
                builtins::RegistryRefresh::with_cdn(cdn.clone())
            });

        let creator_store = deps.pool.as_ref().map(|pool| {
            std::sync::Arc::new(builtins::CreatorCapabilityStore::from_arc(
                std::sync::Arc::new(pool.clone()),
            ))
        });

        let creator_read = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorReadMemory::new, |store| {
                builtins::CreatorReadMemory::with_store(store.clone())
            });
        let creator_write = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorWriteMemory::new, |store| {
                builtins::CreatorWriteMemory::with_store(store.clone())
            });
        let creator_inject = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorInjectPrompt::new, |store| {
                builtins::CreatorInjectPrompt::with_store(store.clone())
            });
        let creator_write_brief = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorWriteBrief::new, |store| {
                builtins::CreatorWriteBrief::with_store(store.clone())
            });

        let outbox_flush = deps
            .pool
            .as_ref()
            .map_or_else(builtins::OutboxFlush::new, |pool| {
                builtins::OutboxFlush::with_pool(pool.clone())
            });
        let outbox_compact = deps
            .pool
            .as_ref()
            .map_or_else(builtins::OutboxCompact::new, |pool| {
                builtins::OutboxCompact::with_pool(pool.clone())
            });

        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(outbox_flush),
            Box::new(outbox_compact),
            Box::new(builtins::WorkspaceOpen),
            Box::new(builtins::WorkspaceCommit),
            Box::new(registry_refresh),
            Box::new(creator_read),
            Box::new(creator_write),
            Box::new(creator_inject),
            Box::new(creator_write_brief),
            Box::new(builtins::JudgeRule),
            Box::new(acp_prompt),
            Box::new(builtins::AcpSessionLoad),
            Box::new(judge_llm),
            Box::new(context_summarize),
            Box::new(kb),
            Box::new(llm_extract),
            Box::new(builtins::SoulExperienceAggregate),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::NovelProjectScaffold::new, |pool| {
                        builtins::NovelProjectScaffold::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::NovelChapterTransition::new, |pool| {
                        builtins::NovelChapterTransition::with_pool(pool.clone())
                    }),
            ),
            // V1.52 T-A P2: essay.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::EssayProjectScaffold::new, |pool| {
                        builtins::EssayProjectScaffold::with_pool(pool.clone())
                    }),
            ),
            // V1.54 P1: game_bible.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::GameBibleProjectScaffold::new, |pool| {
                        builtins::GameBibleProjectScaffold::with_pool(pool.clone())
                    }),
            ),
            // V1.55 P3: script.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::ScriptProjectScaffold::new, |pool| {
                        builtins::ScriptProjectScaffold::with_pool(pool.clone())
                    }),
            ),
            // V1.56 P-last R-V155P2-F002: game_bible.section_status.update
            Box::new(builtins::GameBibleSectionStatusUpdate::new()),
            // V1.67 P2 (R-V160P1-QC1-W001): script.section_status.update
            Box::new(builtins::ScriptSectionStatusUpdate::new()),
            // V1.58 P1: nexus.reference.refresh with pool from runtime deps
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::ReferenceRefresh::new, |pool| {
                        builtins::ReferenceRefresh::with_pool(pool.clone())
                    }),
            ),
            // V1.60 P0: DF-46 local parity — 5 orchestration-scope capabilities
            // (pool-conditional; pool-less returns WorkerUnavailable).
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::WorldStateQuery::new, |pool| {
                        builtins::WorldStateQuery::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::WorldDeltaPropose::new, |pool| {
                        builtins::WorldDeltaPropose::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::WorldDeltaApply::new, |pool| {
                        builtins::WorldDeltaApply::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::TimelineEventAppend::new, |pool| {
                        builtins::TimelineEventAppend::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::ForkCreate::new, |pool| {
                        builtins::ForkCreate::with_pool(pool.clone())
                    }),
            ),
            // V1.61 P3: narrative.compute — injected by the caller
            // (`with_runtime_deps` builds a per-instance engine + cache;
            // `with_runtime_deps_and_wasm` injects the daemon-wide singleton).
            Box::new(narrative_compute),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create an empty registry (for testing).
    #[must_use]
    pub fn empty() -> Self {
        let mut reg = Self {
            capabilities: Vec::new(),
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Build the name-to-index `HashMap` for O(1) lookups.
    ///
    /// Called once during construction. Must be called after `capabilities` is populated.
    fn build_index(&mut self) {
        let mut idx = std::collections::HashMap::with_capacity(self.capabilities.len());
        for (i, cap) in self.capabilities.iter().enumerate() {
            idx.insert(cap.name(), i);
        }
        self.index = Some(idx);
    }

    /// Look up a capability by its dot-separated name.
    ///
    /// Uses the pre-built `HashMap` index for O(1) amortized lookups.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Capability> {
        let idx = self.index.as_ref()?;
        let pos = idx.get(name)?;
        Some(self.capabilities[*pos].as_ref())
    }

    /// Iterate over all registered capabilities.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Capability> {
        self.capabilities.iter().map(std::convert::AsRef::as_ref)
    }

    /// Return the number of registered capabilities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// Return whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_34_builtins() {
        // 31 V1.60 + 1 narrative.compute (V1.61 P3) + 1 essay.draft_status.finalize (V1.63 P2)
        // + 1 script.section_status.update (V1.67 P2 R-V160P1-QC1-W001) = 34.
        let reg = CapabilityRegistry::with_builtins();
        assert_eq!(reg.len(), 34);
    }

    #[test]
    fn registry_lookup_each_builtin() {
        let reg = CapabilityRegistry::with_builtins();
        for name in [
            "sync.pull",
            "sync.push",
            "outbox.flush",
            "outbox.compact",
            "workspace.open",
            "workspace.commit",
            "registry.refresh",
            "creator.read_memory",
            "creator.write_memory",
            "creator.inject_prompt",
            "creator.write_brief",
            "judge.rule",
            "acp.prompt",
            "acp.session_load",
            "judge.llm",
            "context.summarize",
            "kb.extract_work",
            "nexus.llm.extract",
            "soul.experience.aggregate",
            "novel.project_scaffold",
            "novel.chapter_transition",
            "essay.project_scaffold",
            "essay.draft_status.finalize",
            "game_bible.project_scaffold",
            "script.project_scaffold",
            "game_bible.section_status.update",
            "script.section_status.update",
            "nexus.reference.refresh",
            // V1.60 P0: DF-46 orchestration capabilities (full nexus.* names).
            "nexus.world.state.query",
            "nexus.world.delta.propose",
            "nexus.world.delta.apply",
            "nexus.timeline.event.append",
            "nexus.fork.create",
            "narrative.compute",
        ] {
            assert!(
                reg.get(name).is_some(),
                "expected builtin '{name}' to be registered"
            );
        }
    }

    #[test]
    fn registry_lookup_missing_returns_none() {
        let reg = CapabilityRegistry::with_builtins();
        assert!(reg.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn registry_iter_returns_all() {
        let reg = CapabilityRegistry::with_builtins();
        let names: Vec<&str> = reg.iter().map(super::Capability::name).collect();
        // 31 (V1.60) + 1 (V1.61 P3 narrative.compute) + 1 (V1.63 P2 essay.draft_status.finalize)
        // + 1 (V1.67 P2 script.section_status.update).
        assert_eq!(names.len(), 34);
        assert!(names.contains(&"sync.pull"));
        assert!(names.contains(&"judge.rule"));
        assert!(names.contains(&"acp.prompt"));
        assert!(names.contains(&"judge.llm"));
        assert!(names.contains(&"context.summarize"));
        assert!(names.contains(&"kb.extract_work"));
        assert!(names.contains(&"nexus.llm.extract"));
        assert!(names.contains(&"soul.experience.aggregate"));
        assert!(names.contains(&"novel.project_scaffold"));
        assert!(names.contains(&"novel.chapter_transition"));
        // V1.60 P0 DF-46 orchestration capabilities.
        assert!(names.contains(&"nexus.world.state.query"));
        assert!(names.contains(&"nexus.fork.create"));
        // V1.61 P3 narrative.compute.
        assert!(names.contains(&"narrative.compute"));
        // V1.63 P2 essay.draft_status.finalize.
        assert!(names.contains(&"essay.draft_status.finalize"));
        // V1.67 P2 script.section_status.update.
        assert!(names.contains(&"script.section_status.update"));
    }
}

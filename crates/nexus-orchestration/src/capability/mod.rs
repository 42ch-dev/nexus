//! Capability registry + admission/scan/watch; ports live in `nexus-capability`.
//!
//! Design: `docs/specs/orchestration/orchestration-engine.md` §5.1–5.2.

pub mod admission;
pub mod builtins;
pub mod scan;
#[cfg(test)]
pub(crate) mod test_support;
pub mod user_capability;
pub mod watch;

use nexus_capability::{Capability, DaemonToolDispatch, PromptExecutor, WorkspaceExecutor};
use std::path::Path;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// PromptExecutor — injected production prompt seam (A1)
// ---------------------------------------------------------------------------

/// Whether `path` names an already occupied filesystem entry, without
/// following a symlink.
///
/// `Path::exists` is `metadata().is_ok()`, and `metadata` follows the link: a
/// dangling symlink answers `false`, so an ownership check built on it decides
/// the name is free, writes through it and later rolls back a name the user
/// owns. `symlink_metadata` answers the question the bookkeeping actually asks.
pub(crate) fn path_is_occupied(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Runtime dependencies injected through `CapabilityRegistry::with_runtime_deps`.
///
/// Groups pool and prompt executor so daemon boot can construct a single
/// struct and pass it to the registry factory.
///
/// `Clone` (V1.176 P1, AR-92 #3): the hot-reload watcher retains a cloned
/// copy from boot to rebuild fresh registries on the same admission path.
#[derive(Clone)]
pub struct CapabilityRuntimeDeps {
    /// Pool for pool-backed capabilities (`kb.extract_work`, etc.).
    pub pool: Option<sqlx::SqlitePool>,
    /// Production prompt executor for LLM-backed capabilities (`judge.llm`,
    /// `context.summarize`, `acp.prompt`, `nexus.llm.extract`).
    pub prompt_executor: Option<std::sync::Arc<dyn PromptExecutor>>,
    /// Per-run coordinator cancellation tokens (A1) — the executor listens to
    /// the token for the current run concurrently with the Host stream.
    pub session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
    /// Daemon-side tool dispatch for `nexus.*` tools (DF-47, V1.42 P3).
    pub daemon_tool_dispatch: Option<std::sync::Arc<dyn DaemonToolDispatch>>,
    /// CDN fetch config for `registry.refresh` (V1.57 P1 — constructor-injected).
    pub cdn_config: Option<builtins::CdnConfig>,
    /// Production workspace executor for `workspace.open` / `workspace.commit`.
    pub workspace_executor: Option<std::sync::Arc<dyn WorkspaceExecutor>>,
}

// ---------------------------------------------------------------------------
// CapabilityRegistry
// ---------------------------------------------------------------------------

/// Registry of available capabilities.
///
/// Built at daemon startup and **rebuilt as a fresh instance** on
/// user-capability hot reload (V1.176 P1, AR-92): the live generation is
/// held by the shared [`CapabilityRegistryHolder`] and the watcher swaps in
/// a rebuilt registry on scan-dir changes. An instance is immutable after
/// construction.
///
/// Capabilities are stored in a `Vec` for ordered iteration, with a `HashMap`
/// index for O(1) lookup by name. The index is built eagerly by every
/// constructor and append path — never lazily.
pub struct CapabilityRegistry {
    capabilities: Vec<Box<dyn Capability>>,
    /// Eager index: `name` → position in `capabilities`. Built by every
    /// constructor and append path (never on-demand).
    index: Option<std::collections::HashMap<&'static str, usize>>,
}

impl CapabilityRegistry {
    /// Create a registry pre-populated with all built-in capabilities.
    ///
    /// Built-ins: `sync.pull`, `sync.push`, `outbox.flush`, `outbox.compact`,
    /// `workspace.open`, `workspace.commit`, `registry.refresh`,
    /// `creator.read_memory`, `creator.write_memory`, `creator.inject_prompt`,
    /// `creator.write_brief`, `judge.rule`, `acp.prompt`, `judge.llm`,
    /// `context.summarize`, `kb.extract_work`,
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
            Box::new(builtins::WorkspaceOpen::new()),
            Box::new(builtins::WorkspaceCommit::new()),
            Box::new(builtins::RegistryRefresh::new()),
            Box::new(builtins::CreatorReadMemory::new()),
            Box::new(builtins::CreatorWriteMemory::new()),
            Box::new(builtins::CreatorInjectPrompt::new()),
            Box::new(builtins::CreatorWriteBrief::new()),
            Box::new(builtins::JudgeRule),
            Box::new(builtins::AcpPrompt::new()),
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
            // V1.63 P2: register essay.draft_status.finalize for the
            // essay-writing preset's finalize_commit state. This registry
            // carries no workspace executor, so the instance is
            // validation-only — `with_runtime_deps` registers the executable
            // one.
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
            Box::new(builtins::WorkspaceOpen::new()),
            Box::new(builtins::WorkspaceCommit::new()),
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
            Box::new(builtins::JudgeLlm::new()),
            Box::new(builtins::ContextSummarize::new()),
            Box::new(builtins::KbExtractWork::with_pool(pool.clone())),
            Box::new(builtins::LlmExtract::new()),
            Box::new(builtins::SoulExperienceAggregate),
            Box::new(builtins::NovelProjectScaffold::with_pool(pool.clone())),
            Box::new(builtins::NovelChapterTransition::with_pool(pool.clone())),
            // V1.52 T-A P2: essay.project_scaffold with pool.
            Box::new(builtins::EssayProjectScaffold::with_pool(pool.clone())),
            // V1.63 P2: essay.draft_status.finalize (pool only, no workspace
            // executor — validation-only here; `with_runtime_deps` registers
            // the executable instance).
            Box::new(builtins::EssayDraftStatusFinalize::new()),
            // V1.54 P1: game_bible.project_scaffold with pool.
            Box::new(builtins::GameBibleProjectScaffold::with_pool(pool.clone())),
            // V1.55 P3: script.project_scaffold with pool.
            Box::new(builtins::ScriptProjectScaffold::with_pool(pool.clone())),
            // V1.56 P-last R-V155P2-F002: game_bible.section_status.update
            Box::new(builtins::GameBibleSectionStatusUpdate::new()),
            // V1.67 P2 (R-V160P1-QC1-W001): script.section_status.update
            Box::new(builtins::ScriptSectionStatusUpdate::new()),
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

    /// Create a registry with runtime dependencies, the daemon-wide singleton
    /// `WasmEngine` + `ModuleCache`, **and** user capabilities scanned from
    /// `scan_dir` (V1.172 P0, DR-10; AR-36).
    ///
    /// Builds the base via [`with_runtime_deps_and_wasm`], appends the
    /// admitted user capabilities **after** builtins, then rebuilds the eager
    /// name index. The scan is fail-safe by contract (AR-35): a missing
    /// directory or a bad descriptor never fails boot — bad entries land in
    /// `outcome.skipped` with named reasons (already `warn!`-logged).
    #[must_use]
    pub fn with_runtime_deps_and_wasm_and_user_caps(
        deps: &CapabilityRuntimeDeps,
        engine: std::sync::Arc<nexus_wasm_host::WasmEngine>,
        module_cache: std::sync::Arc<nexus_wasm_host::ModuleCache>,
        scan_dir: &std::path::Path,
    ) -> (Self, scan::ScanOutcome) {
        let engine_handle = engine.clone();
        let module_cache_handle = module_cache.clone();
        let mut reg = Self::with_runtime_deps_and_wasm(deps, engine, module_cache);
        let outcome =
            reg.append_user_caps(scan_dir, Some(&engine_handle), Some(&module_cache_handle));
        (reg, outcome)
    }

    /// Create a registry with runtime dependencies **and** user capabilities
    /// scanned from `scan_dir`, on the engine-less boot path (V1.172 P0,
    /// DR-10; AR-36/AR-44).
    ///
    /// Engine-less arm of the AR-36 pair: user capabilities still register
    /// (discoverable); their stub `run()` returns `WorkerUnavailable` until P1
    /// wires the executor.
    #[must_use]
    pub fn with_runtime_deps_and_user_caps(
        deps: &CapabilityRuntimeDeps,
        scan_dir: &std::path::Path,
    ) -> (Self, scan::ScanOutcome) {
        let mut reg = Self::with_runtime_deps(deps);
        let outcome = reg.append_user_caps(scan_dir, None, None);
        (reg, outcome)
    }

    /// Append admitted user capabilities after builtins and rebuild the eager
    /// index (AR-36). Shared by the two user-capability constructors.
    ///
    /// The engine arm passes the daemon-wide [`WasmEngine`] + [`ModuleCache`]
    /// (some/some) so each admitted capability's real executor (AR-37) can
    /// compile/run; the engine-less arm passes `None`/`None` (AR-44) so
    /// `run()` returns `WorkerUnavailable`. Handles are forwarded into the
    /// scan so `UserCapability::new` carries them from construction.
    ///
    /// Builtin collision (AR-43 gate 1): the scan admits against the
    /// registry's builtin name set (`self.capabilities` — the builtins built
    /// by the constructor), so a user capability whose name equals a builtin
    /// is **skipped inside the scan** (builtin wins — AR-36/AR-43). Each
    /// collision lands in `outcome.skipped` with the named `NameCollision`
    /// reason and is `warn!`-logged before the append.
    fn append_user_caps(
        &mut self,
        scan_dir: &std::path::Path,
        engine: Option<&std::sync::Arc<nexus_wasm_host::WasmEngine>>,
        module_cache: Option<&std::sync::Arc<nexus_wasm_host::ModuleCache>>,
    ) -> scan::ScanOutcome {
        let builtin_names: std::collections::HashSet<&str> =
            self.capabilities.iter().map(|c| c.name()).collect();
        let outcome = scan::scan_user_capabilities(scan_dir, &builtin_names, engine, module_cache);
        // V1.176 P1 (AR-92 #4): the outcome keeps its concrete admitted
        // entries (cloned into the registry) so the boot site can seed the
        // watcher's last-good mirror from them — the scan is the single
        // admission path for both boot and hot reload. The boot-site
        // aggregate log (`log_scan_outcome`) therefore reports the admitted
        // count it was documented to report.
        self.append_user_cap_entries(&outcome.admitted);
        outcome
    }

    /// Box and append `admitted` after builtins and rebuild the eager index —
    /// the single registry-append seam shared by the boot constructors
    /// ([`append_user_caps`]) and the hot-reload watcher
    /// (`rebuild_registry_with_merge`, V1.176 P1 M-3) so the boxing stays in
    /// one place.
    fn append_user_cap_entries(
        &mut self,
        admitted: &[crate::capability::user_capability::UserCapability],
    ) {
        for cap in admitted {
            self.capabilities
                .push(Box::new(cap.clone()) as Box<dyn Capability>);
        }
        self.build_index();
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

        let judge_llm =
            deps.prompt_executor
                .as_ref()
                .map_or_else(builtins::JudgeLlm::new, |executor| {
                    builtins::JudgeLlm::with_prompt_executor(executor.clone())
                        .with_session_cancels(deps.session_cancels.clone())
                });

        let context_summarize = deps.prompt_executor.as_ref().map_or_else(
            builtins::ContextSummarize::new,
            |executor| {
                builtins::ContextSummarize::with_prompt_executor(executor.clone())
                    .with_session_cancels(deps.session_cancels.clone())
            },
        );

        // V1.51 T-A P0: nexus.llm.extract reuses the same prompt executor as
        // judge.llm / context.summarize / acp.prompt (compass §0.1 #7).
        let llm_extract =
            deps.prompt_executor
                .as_ref()
                .map_or_else(builtins::LlmExtract::new, |executor| {
                    builtins::LlmExtract::with_prompt_executor(executor.clone())
                        .with_session_cancels(deps.session_cancels.clone())
                });

        let acp_prompt =
            deps.prompt_executor
                .as_ref()
                .map_or_else(builtins::AcpPrompt::new, |executor| {
                    builtins::AcpPrompt::with_prompt_executor(executor.clone())
                        .with_session_cancels(deps.session_cancels.clone())
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

        let workspace_open = deps
            .workspace_executor
            .as_ref()
            .map_or_else(builtins::WorkspaceOpen::new, |executor| {
                builtins::WorkspaceOpen::with_workspace_executor(executor.clone())
            });
        let workspace_commit = deps
            .workspace_executor
            .as_ref()
            .map_or_else(builtins::WorkspaceCommit::new, |executor| {
                builtins::WorkspaceCommit::with_workspace_executor(executor.clone())
            });

        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(outbox_flush),
            Box::new(outbox_compact),
            Box::new(workspace_open),
            Box::new(workspace_commit),
            Box::new(registry_refresh),
            Box::new(creator_read),
            Box::new(creator_write),
            Box::new(creator_inject),
            Box::new(creator_write_brief),
            Box::new(builtins::JudgeRule),
            Box::new(acp_prompt),
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
                    })
                    .with_workspace_executor(deps.workspace_executor.clone()),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::NovelChapterTransition::new, |pool| {
                        builtins::NovelChapterTransition::with_pool(pool.clone())
                    })
                    .with_workspace_executor(deps.workspace_executor.clone()),
            ),
            // V1.52 T-A P2: essay.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::EssayProjectScaffold::new, |pool| {
                        builtins::EssayProjectScaffold::with_pool(pool.clone())
                    })
                    .with_workspace_executor(deps.workspace_executor.clone()),
            ),
            // V1.63 P2: essay.draft_status.finalize — the essay preset's
            // finalize_commit writer. It writes Work bytes, so it needs the
            // runtime executor; `with_builtins` registers it without one and
            // therefore stays validation-only.
            Box::new(
                builtins::EssayDraftStatusFinalize::new()
                    .with_workspace_executor(deps.workspace_executor.clone()),
            ),
            // V1.54 P1: game_bible.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::GameBibleProjectScaffold::new, |pool| {
                        builtins::GameBibleProjectScaffold::with_pool(pool.clone())
                    })
                    .with_workspace_executor(deps.workspace_executor.clone()),
            ),
            // V1.55 P3: script.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::ScriptProjectScaffold::new, |pool| {
                        builtins::ScriptProjectScaffold::with_pool(pool.clone())
                    })
                    .with_workspace_executor(deps.workspace_executor.clone()),
            ),
            // V1.56 P-last R-V155P2-F002: game_bible.section_status.update
            Box::new(
                builtins::GameBibleSectionStatusUpdate::new()
                    .with_workspace_executor(deps.workspace_executor.clone()),
            ),
            // V1.67 P2 (R-V160P1-QC1-W001): script.section_status.update
            Box::new(
                builtins::ScriptSectionStatusUpdate::new()
                    .with_workspace_executor(deps.workspace_executor.clone()),
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
    /// Called by every constructor and by the append seam
    /// ([`append_user_cap_entries`]) after `capabilities` is populated —
    /// the index is eager, never built on demand (M-4).
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

impl nexus_preset::capability_catalog::CapabilityCatalog for CapabilityRegistry {
    fn input_schema(&self, name: &str) -> Option<&str> {
        self.get(name).map(Capability::input_schema)
    }
}

// ---------------------------------------------------------------------------
// CapabilityRegistryHolder — hot-reload swap seam (V1.176 P1, AR-92)
// ---------------------------------------------------------------------------

/// Shared holder for the live capability registry (AR-92).
///
/// A hot reload rebuilds a **fresh** [`CapabilityRegistry`] on the same
/// scan/admission path as boot and atomically swaps it into this holder —
/// never an in-place mutation of a live registry (races `iter()`/`get()`
/// borrowers) and never a second registry lane (single-spine invariant,
/// AR-92 #1/#8).
///
/// Lock/re-entrancy contract (AR-92 #7): readers clone the inner `Arc`
/// under the read lock and release immediately — no reader holds the lock
/// across an `.await`, and wasm execution never runs under the holder lock.
/// An in-flight dispatch that cloned the pre-swap `Arc` finishes against
/// last-good (no abort, no half-call — PL-9). The write lock is held only
/// for the pointer write; swaps are serialized by construction (one watcher
/// task, one tick at a time).
///
/// Boot creates exactly **one** holder and shares it with `WorkspaceState`,
/// the engine, and the watcher (AR-92 #2/#6).
#[derive(Clone, Default)]
pub struct CapabilityRegistryHolder {
    inner: std::sync::Arc<std::sync::RwLock<Option<std::sync::Arc<CapabilityRegistry>>>>,
}

impl CapabilityRegistryHolder {
    /// Create an empty holder. The boot registry is swapped in before any
    /// reader, the engine, or a graph build touches it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clone the current registry under the read lock, if one has been
    /// swapped in.
    #[must_use]
    pub fn get(&self) -> Option<std::sync::Arc<CapabilityRegistry>> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner) // poison-recovery (crate policy)
            .clone()
    }

    /// Atomically swap in a freshly rebuilt registry (write lock; held only
    /// for the pointer write — AR-92 #7). The previous generation is dropped
    /// AFTER the write lock is released so a last-reference drop (tearing
    /// down a full builtin+user registry) never stalls readers (M-1).
    pub fn swap(&self, registry: std::sync::Arc<CapabilityRegistry>) {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = (*guard).replace(registry);
        drop(guard);
        drop(previous);
    }

    /// Create a holder pre-populated with `registry` — boot convenience:
    /// the boot registry is swapped in before readers, the engine, and the
    /// watcher exist, and every later generation arrives via [`swap`](Self::swap).
    #[must_use]
    pub fn with_registry(registry: std::sync::Arc<CapabilityRegistry>) -> Self {
        let holder = Self::new();
        holder.swap(registry);
        holder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::test_support::write_capability_dir;
    use nexus_capability::{CapabilityError, CapabilityOrigin};
    use serde_json::Value;

    #[test]
    fn registry_has_32_builtins() {
        // V1.186 removed the obsolete `acp.session_load` placeholder from the
        // prior 34; v231 removed `nexus.reference.refresh` (33→32).
        let reg = CapabilityRegistry::with_builtins();
        assert_eq!(reg.len(), 32);
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

    #[test]
    fn builtin_capability_origin_defaults_to_builtin() {
        // AR-40: the default `origin()` is `Builtin` — zero edits to the ~34
        // builtin impls; a builtin capability must report `Builtin`.
        let reg = CapabilityRegistry::with_builtins();
        let cap = reg
            .get("sync.pull")
            .expect("sync.pull builtin must be registered");
        assert!(matches!(cap.origin(), CapabilityOrigin::Builtin));
    }

    #[tokio::test]
    async fn registry_iter_returns_all() {
        let reg = CapabilityRegistry::with_builtins();
        let names: Vec<&str> = reg.iter().map(super::Capability::name).collect();
        // V1.186 removed the obsolete `acp.session_load` placeholder from the
        // prior 34; v231 removed `nexus.reference.refresh` (33→32).
        assert_eq!(names.len(), 32);
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

    /// The runtime-deps registry is the supported executable path for the
    /// essay finalizer: an invocation resolved from it must reach the injected
    /// Work executor instead of refusing with `WorkerUnavailable`.
    #[tokio::test]
    async fn runtime_deps_registry_runs_the_essay_finalizer_through_its_executor() {
        use crate::capability::builtins::work_commit::test_support::RecordingExecutor;

        let tmp = tempfile::tempdir().unwrap();
        let workspace_root = tmp.path().to_path_buf();
        let drafts = workspace_root
            .join("Works")
            .join("registry-essay")
            .join("Drafts");
        std::fs::create_dir_all(&drafts).unwrap();
        std::fs::write(drafts.join("draft.md"), "---\nstatus: draft\n---\nbody").unwrap();

        let executor = RecordingExecutor::at(workspace_root);
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            daemon_tool_dispatch: None,
            cdn_config: None,
            workspace_executor: Some(executor.clone()),
        };
        let registry = CapabilityRegistry::with_runtime_deps(&deps);
        let cap = registry
            .get("essay.draft_status.finalize")
            .expect("the executable registry exposes the essay finalizer");

        cap.run(serde_json::json!({ "work_ref": "registry-essay" }))
            .await
            .expect("finalize runs through the registry-wired executor");
        assert_eq!(
            executor.commits().len(),
            1,
            "the registry's executor received the draft commit"
        );
    }

    // ── User capability constructors (T2 / AR-36) ───────────────────────
    // The trio fixture is the shared `test_support::write_capability_dir`
    // (qc1 S-1): one writer for the in-crate test modules.

    #[test]
    fn with_runtime_deps_and_user_caps_appends_after_builtins() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            daemon_tool_dispatch: None,
            cdn_config: None,
            workspace_executor: None,
        };
        // Base builtin count of the runtime-deps constructor (34 — the shared
        // `build_with_narrative_compute` vec).
        let base = CapabilityRegistry::with_runtime_deps(&deps);
        let base_len = base.len();
        let (reg, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, tmp.path());
        assert!(
            outcome.skipped.is_empty(),
            "no skips: {:?}",
            outcome.skipped
        );
        assert_eq!(reg.len(), base_len + 1, "base builtins + 1 user capability");
        // Builtins first, then the appended user capability (AR-36 order).
        let names: Vec<&str> = reg.iter().map(Capability::name).collect();
        let base_names: Vec<&str> = base.iter().map(Capability::name).collect();
        assert_eq!(
            &names[..base_len],
            &base_names[..],
            "prefix must equal the base registry's builtin order (S-005 tightened assertion)"
        );
        assert_eq!(names[base_len], "demo.pull");
        // Eager index rebuilt: lookup works for both.
        assert!(reg.get("narrative.compute").is_some());
        assert!(reg.get("demo.pull").is_some());
        // Registered through the index (not just iterated).
        let cap = reg.get("demo.pull").expect("user cap indexed");
        assert_eq!(cap.input_schema(), r#"{"type":"object"}"#);
    }

    #[test]
    fn user_cap_colliding_with_builtin_is_skipped_builtin_wins() {
        // P1 T4 (AR-43 gate 1): a user capability named like a builtin
        // (here `sync.pull`) is skipped inside the scan with a named
        // `NameCollision` reason — the builtin keeps serving `get()` and the
        // catalog lists exactly one row (builtin wins, AR-36/AR-43).
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "sync.pull");
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            daemon_tool_dispatch: None,
            cdn_config: None,
            workspace_executor: None,
        };
        let (reg, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, tmp.path());
        assert_eq!(outcome.admitted.len(), 0, "colliding user cap not admitted");
        assert_eq!(outcome.skipped.len(), 1, "one skip with named reason");
        assert_eq!(outcome.skipped[0].name, "sync.pull");
        assert!(
            outcome.skipped[0].reason.contains("NameCollision")
                || outcome.skipped[0]
                    .reason
                    .contains("collides with a builtin"),
            "named NameCollision reason, got: {:?}",
            outcome.skipped[0].reason
        );
        // Builtin still serves `get()` through the eager index.
        let cap = reg.get("sync.pull").expect("builtin still indexed");
        assert_eq!(cap.input_schema(), builtins::SyncPull.input_schema());
        // Catalog has exactly one row for the name.
        let rows = reg.iter().filter(|c| c.name() == "sync.pull").count();
        assert_eq!(rows, 1, "no duplicate catalog row");
    }

    #[tokio::test]
    async fn with_runtime_deps_and_wasm_and_user_caps_indexes_and_executor_wired() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            daemon_tool_dispatch: None,
            cdn_config: None,
            workspace_executor: None,
        };
        let engine = std::sync::Arc::new(nexus_wasm_host::WasmEngine::new().unwrap());
        let cache = std::sync::Arc::new(nexus_wasm_host::ModuleCache::new());
        let (reg, outcome) = CapabilityRegistry::with_runtime_deps_and_wasm_and_user_caps(
            &deps,
            engine,
            cache,
            tmp.path(),
        );
        assert!(
            outcome.skipped.is_empty(),
            "no skips: {:?}",
            outcome.skipped
        );
        let cap = reg.get("demo.pull").expect("user cap indexed");
        // PL-10: the admitted-with-engine path no longer returns the P0
        // stub's WorkerUnavailable — the real executor (AR-37) runs. The
        // fixture's module pair is hash-consistent (admitted) but its wasm
        // bytes are not valid wasm, so the first `run()` fails fail-closed
        // at module load as PermanentExternal (AR-37 table: InvalidModule →
        // PermanentExternal).
        let err = cap.run(serde_json::json!({})).await.unwrap_err();
        assert!(
            matches!(err, CapabilityError::PermanentExternal(_)),
            "expected PermanentExternal from the real executor, got {err:?}"
        );
        assert!(
            err.to_string().contains("module fault"),
            "named module message, got: {err}"
        );
    }

    // ─── v1.237 P2 preflight T1: per-run registry injection ────────────────
    //
    // The premise under test: a run can be driven by a registry the caller
    // owns (a fresh holder per run), so a simulation can observe every
    // dispatch without swapping the shared production holder. The registry
    // holds ONLY the probe row — a path that resolves a real builtin instead
    // would fail with "not found", so a green run is also proof that nothing
    // real executed.
    struct RecordingCap {
        hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Capability for RecordingCap {
        fn name(&self) -> &'static str {
            "record.probe"
        }

        fn input_schema(&self) -> &'static str {
            r#"{"type":"object"}"#
        }

        fn output_schema(&self) -> &'static str {
            r#"{"type":"object"}"#
        }

        async fn run(&self, _input: Value) -> Result<Value, CapabilityError> {
            self.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(serde_json::json!({ "recorded": true }))
        }
    }

    fn recording_registry(
        hits: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Arc<CapabilityRegistry> {
        let mut reg = CapabilityRegistry::empty();
        reg.capabilities
            .push(Box::new(RecordingCap { hits: hits.clone() }));
        reg.build_index();
        Arc::new(reg)
    }

    const T1_PROBE_PRESET: &str = r#"
preset:
  id: preflight-t1-probe
  version: 1
  kind: creator
  description: "preflight T1: one reachable capability dispatch"
  requires_capabilities:
    - record.probe
  initial: probe
  terminal: done
states:
  - id: probe
    enter:
      - kind: capability
        name: record.probe
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
"#;

    const T1_QUIET_PRESET: &str = r#"
preset:
  id: preflight-t1-quiet
  version: 1
  kind: creator
  description: "preflight T1: a graph that reaches no capability"
  requires_capabilities: []
  initial: quiet
  terminal: done
states:
  - id: quiet
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
"#;

    async fn run_to_completion(
        engine: &crate::engine::GraphFlowEngine,
        loaded: &nexus_preset::LoadedPreset,
    ) {
        use crate::engine::{OrchestrationEngine, StepOutcome};

        let sid = engine
            .start_session_with_preset(loaded)
            .await
            .expect("start preset run");
        for _ in 0..16 {
            match engine.run_step(&sid).await.expect("run step") {
                StepOutcome::Completed { .. } => return,
                StepOutcome::WaitingForInput { .. } | StepOutcome::Paused { .. } => {
                    engine
                        .signal(&sid, crate::engine::EngineSignal::Resume)
                        .await
                        .expect("resume");
                }
            }
        }
        panic!("run did not complete in 16 steps");
    }

    #[tokio::test]
    async fn preflight_t1_recording_registries_are_per_run_and_isolate_dispatch_between_engines() {
        use crate::engine::GraphFlowEngine;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // Positive control: the counter is live, and the ONLY row the run can
        // resolve is the probe (the real builtins are absent from this
        // registry, so any bypass fails the run instead of passing silently).
        let first_hits = Arc::new(AtomicUsize::new(0));
        let first_caps = recording_registry(&first_hits);
        assert!(
            first_caps.get("judge.rule").is_none(),
            "no builtins in the recording registry"
        );
        let probe_loaded = nexus_preset::load_preset_from_str(T1_PROBE_PRESET, first_caps.as_ref())
            .expect("probe preset loads against the recording registry");
        let probe_engine = GraphFlowEngine::new_with_storage(
            Arc::new(graph_flow::InMemorySessionStorage::new()),
            CapabilityRegistryHolder::with_registry(first_caps.clone()),
        );
        run_to_completion(&probe_engine, &probe_loaded).await;
        assert_eq!(
            first_hits.load(Ordering::SeqCst),
            1,
            "one reachable capability, one dispatch"
        );

        // Second engine, same dispatching graph, its own registry: if this
        // engine resolved the first engine's registry instead, the dispatch
        // below would land in `first_hits` and leave `second_hits` at zero.
        let second_hits = Arc::new(AtomicUsize::new(0));
        let second_caps = recording_registry(&second_hits);
        assert!(
            second_caps.get("judge.rule").is_none(),
            "no builtins in the second recording registry"
        );
        let second_loaded =
            nexus_preset::load_preset_from_str(T1_PROBE_PRESET, second_caps.as_ref())
                .expect("probe preset loads against the second registry");
        let second_engine = GraphFlowEngine::new_with_storage(
            Arc::new(graph_flow::InMemorySessionStorage::new()),
            CapabilityRegistryHolder::with_registry(second_caps.clone()),
        );
        run_to_completion(&second_engine, &second_loaded).await;
        assert_eq!(
            second_hits.load(Ordering::SeqCst),
            1,
            "the second engine dispatched through its own registry"
        );
        assert_eq!(
            first_hits.load(Ordering::SeqCst),
            1,
            "the second run did not touch the first engine's counter"
        );

        // Negative: a graph with no capability-reachable state records zero.
        let quiet_hits = Arc::new(AtomicUsize::new(0));
        let quiet_caps = recording_registry(&quiet_hits);
        let quiet_loaded = nexus_preset::load_preset_from_str(T1_QUIET_PRESET, quiet_caps.as_ref())
            .expect("quiet preset loads");
        let quiet_engine = GraphFlowEngine::new_with_storage(
            Arc::new(graph_flow::InMemorySessionStorage::new()),
            CapabilityRegistryHolder::with_registry(quiet_caps.clone()),
        );
        run_to_completion(&quiet_engine, &quiet_loaded).await;
        assert_eq!(
            quiet_hits.load(Ordering::SeqCst),
            0,
            "no dispatch without a capability"
        );

        // No two runs shared a registry, so nothing leaked between them.
        assert!(!Arc::ptr_eq(&first_caps, &second_caps));
        assert!(!Arc::ptr_eq(&first_caps, &quiet_caps));
        assert!(!Arc::ptr_eq(&second_caps, &quiet_caps));
        assert!(
            CapabilityRegistryHolder::new().get().is_none(),
            "a fresh holder is empty, never the production one"
        );
    }
}

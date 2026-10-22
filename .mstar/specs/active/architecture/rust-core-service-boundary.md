# Rust Core Service Boundary

**Status:** Accepted target — locked 2026-09-13. The boundary it locks is the shipped composition: the ordinary `nexus42` default is the direct-core + cloud + optional Connect `cli` cohort, the desktop is the Electron host, and the integrated daemon composition no longer exists. This document is normative for that boundary and is self-contained.
**Document class:** Master
**Pillar:** Cross-cutting — Harness, Canvas, and Computable consumption ends keep their product identities; this spec only locks the service-boundary target those pillars run on.
**Coordinates with:** [local-runtime-boundary.md](local-runtime-boundary.md), [cli-spec.md](../cli/cli-spec.md), [desktop-shell.md](../surfaces/desktop-shell.md), [agent-host.md](../agents/agent-host.md), [concurrency.md](../runtime/concurrency.md), [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md), [daemon-api-surface-conventions.md](../runtime/daemon-api-surface-conventions.md), [schemas-directory-layout.md](schemas-directory-layout.md)
## 0. Authority

| Layer | Authority | Honest claim |
| --- | --- | --- |
| **Shipped current policy** | Shipped Masters: `local-runtime-boundary`, `cli-spec`, `desktop-shell`, `agent-host`, `concurrency` (plus this document for the boundary itself) | The integrated `nexus42` daemon and the CLI `DaemonClient` leaves do not exist: retained families call the Rust authority directly, or through the standalone TS service for browser/Electron. The shipped desktop is the Electron host and no native sidecar shell is part of it |
| **Accepted target** | This document | Transport-neutral Rust authority for every retained family, independent Rust CLI/runtime, complete TS service composition, proof-gated Electron desktop |
| **Wire DTOs** | `schemas/` → generated Rust + `@42ch/nexus-contracts` | Unchanged by this lock |
| **Product names** | Root `AGENTS.md` | `Nexus`, `nexus42`, `nexus-runtime`, `@42ch` — the integrated daemon runtime is not a product name |

**Activation rule:** a family uses the target topology only after the extracted Rust service is the single effect owner **and** the old mixed-handler path for that family has an explicit deletion owner and proof. That gate has fired for every retained family: the old mixed-handler path no longer exists, no shipped Master describes an implementable daemon topology, and the retained SSOT is this document plus `cli-spec`, `desktop-shell` and the TS service. **Implementation is not product qualification:** the retained route families are implemented and exercised through the TS service and Rust core (§6.5); outstanding product acceptance/QA and Run Studio completeness must not be reported as missing or wholly unexercised routes. Distribution qualification and first-run qualification remain distinct obligations (§6.5.1). Route-level exercise alone does not prove the complete program.

## 1. Goal

The target is a **behavior-preserving** cutover: one Rust authority for domain and execution truth; TypeScript for transport, auth composition, and high-churn provider SDKs; independent Rust products that do not require Node, Electron, or a daemon for basic authoring.

## 2. Frozen product identities

These names and roles stay. The refactor does not invent a second CLI, a second runtime product, a second contracts package, or a new first-party app.

| Identity | Role that stays |
| --- | --- |
| **Nexus** | Product |
| **`nexus42`** | User-facing CLI executable. The ordinary default is the direct-core + cloud + optional Connect `cli` cohort; there is no operator CLI that launches or statuses the TS service and no internal `legacy-cli` mode. The shipped desktop remains the Electron host. |
| **Daemon runtime** | Not a product. The integrated local host and the public `nexus42 daemon …` names do not exist (no `nexus42d`, no `nexus42 service` alias). The standalone TS service is the HTTP host for browser/Electron; it is not a CLI destination. |
| **`nexus-runtime`** | Headless integrator executable. Connect-only profile. Must not boot a hidden full scheduler/Host/SPA. |
| **`@42ch/nexus-contracts`** | Published TypeScript contracts package |
| **`nexus-contracts` crate** | Monorepo-internal generated Rust types |

### Three consumption ends (unchanged)

| End | Surface | Target change |
| --- | --- | --- |
| Developers | `nexus42` CLI + local HTTP/API | Retained authoring/storage (including Works list/status/use) calls Rust directly with no Node/daemon; operator HTTP stays a TS composition for Electron/browser only — there is no `nexus42 daemon` group or CLI launcher. |
| Content creators | `apps/web` + desktop shell wrapping the same SPA | No visual redesign. Browser uses generated `NexusClient`. The desktop host is Electron — exactly one desktop host. |
| Third-party users | `nexus-runtime` + Connect | Keep the existing Connect-only served-op profile. No first-party player. |

### Current workspace boundary inventory

These six workspace crates extend the frozen 2026-05 inventory in [local-cloud-crate-architecture.md](../../archived/architecture/2026-09-21-local-cloud-crate-architecture.md); they support the authority graph in §4.2, not another product or host. Membership is declared in root `Cargo.toml`; the linked per-crate `AGENTS.md` files own their detailed rules.

| Workspace crate | Boundary role |
| --- | --- |
| [`nexus-core`](../../../../crates/nexus-core/AGENTS.md) | Owned transport-neutral World KB service and stored Actor/holder admission authority; no transport or SQL pool in its public domain surface. |
| [`nexus-core-node`](../../../../crates/nexus-core-node/AGENTS.md) | Thin Node-API cdylib composing core and provider ports, with environment-local state and an audited FFI boundary. |
| [`nexus-preset`](../../../../crates/nexus-preset/AGENTS.md) | Pure preset authoring/source-file domain library shared by core and orchestration; no execution, Host, WASM, HTTP, or database dependency. |
| [`nexus-provider-conformance`](../../../../crates/nexus-provider-conformance/AGENTS.md) | Provider-neutral normalized `HostEvent` stream conformance runner; CI/test-only, not wired into product binaries. |
| [`nexus-provider-ports`](../../../../crates/nexus-provider-ports/AGENTS.md) | Port-only `ProviderPort`/`ProviderResult` contracts; no Host, SDK, SQL, napi, or orchestration implementation. |
| [`nexus-storage-guard`](../../../../crates/nexus-storage-guard/AGENTS.md) | Audited SQLite FFI boundary installing connection-local writer-protocol functions; no business SQL or independent pool. |

## 3. Target topology

```text
 Basic Rust CLI ──────┐
 Connect-only host ───┼──> nexus-core ──> guarded SQLite / canonical spoke ports
                     │        │
                     │        └── one workspace engine/effect owner
                     │
 Browser / existing NexusClient
          │ HTTP / SSE
          v
 Standalone TS service / TS provider SDK adapters
          │
          v
 Thin napi adapter ───┘
          ^
          │ same native composition; never renderer loading .node
 Electron utility (app-owned) / independent service (attach-owned)
```

Rules:

1. The facade is **not** a renamed daemon or a universal `WorkspaceState`.
2. Owned schema-derived DTOs and receipts cross the facade. SQL pools, borrowed transactions, and environment-local handles do not.
3. TypeScript does not own domain SQL, stored-principal authorization, or a second workflow/recovery engine.
4. The TS service **never imports Electron**. Electron main owns windows/OS. A utility process is the preferred *app-managed* TS-service host. Work that must outlive app exit attaches to an **independent** service, not that utility.
5. Browser/renderer never loads `.node`, secrets, or native handles.
6. One engine/effect owner at a time. No dual writable engines and no silent feature removal.

### 3.1 Selected library/package boundaries

- `crates/nexus-core`: owned `CoreService`, `CoreOpenOptions`, opaque stored-authorized `Principal`, neutral `CoreError`. The commands `open`, `active_principal`, `world_kb_graph`, `patch_world_kb_entity`, `changes`, `close` are shipped; family-named methods extend the same authority in private sibling modules. A narrow `CoreHomeService` handles pre-selection registration/configuration without holding a workspace pool. SQL pools/borrowed transactions/WorkspaceState and Host/SDK implementation types are not public service contracts. The `CoreService::pool` journal escape hatch must be removed through owned journal commands, with every native caller migrated; it is not a target extension seam.
- `crates/nexus-provider-ports`: schema-derived `ProviderCall`, `ProviderReply`, `ProviderEventBatch` and Send+Sync async `call`/bounded pull `next`; no engine or SDK implementation.
- `crates/nexus-core-node` + `packages/nexus-native`: environment-local thin ABI/facade; compose existing Host/ACP and core once. `packages/nexus-provider-acp` uses the actual stable-v1 ACP TypeScript SDK behind the Rust port.
- `nexus-spoke-adapter` keeps one conversion/upsert seam; default-on `compute` owns WASM/module-cache capability. Core disables that feature rather than importing orchestration for helpers.
- `crates/nexus-storage-guard` is the narrow audited SQLite FFI connection-function boundary; all business/domain crates retain unsafe-code prohibition.
- Products stay in `apps`: standalone `apps/nexus-service`, existing `apps/nexus42`, and the desktop host `apps/desktop-electron`. Exactly one desktop host — no second desktop or competing desktop app.
- Same `nexus42` binary/parser. The ordinary default is the direct-core + cloud + optional Connect `cli` cohort; `legacy-cli` / `web-embed` do not exist, not even as an internal mode. A new helper CLI or second product name is prohibited.

### 3.2 Selective authority graph

These are the shipped dependency cohorts, kept as the regression contract: an edge listed under "forbidden" must not reappear. `daemon-runtime` names the deleted daemon crate.

| Cohort | Selected edges | Forbidden accidental edges |
| --- | --- | --- |
| Core domain/default | contracts, home-layout, guarded local-db, knowledge/narrative, Creator memory, MCA, pure preset; spoke-adapter with defaults disabled | daemon-runtime, orchestration engine, Host/ACP host, graph-flow, WASM, Axum, napi, Connect |
| Core `execution` | Existing optional orchestration, coordinator/scheduler/capability/workspace commit; explicit `RunnerDeps` ports | transport host, unconditional WASM; a ProviderPort alone is not production runner wiring |
| Core `provider-host` | optional existing Host/ACP adapters and one admitted session registry | implicit scheduler/execution startup |
| Core `compute` | explicit execution plus WASM/spoke compute, including optional orchestration compute edges | default/domain/Connect-host activation |
| Peer-control operator | optional reverse-invoke/Connect-client/MCP adapters | activation by Connect-host product |
| Ordinary CLI (`cli`, the default product selector) | Direct local commands, existing cloud clients and local ACP spawning; existing chronology/cron/ops library dependencies | daemon-runtime, Axum router, SPA, default libp2p, Node requirement, TS-service mediation or scheduler startup |
| Optional CLI Connect (`cli,connect-host`) | Explicit spoke/libp2p plus existing spoke-adapter compute | implicit Rust daemon/HTTP host |
| Connect-only runtime (`connect-host`, no defaults) | Existing stored-Actor invoke authority and spoke-connect/libp2p; explicit spoke-adapter `compute` preserves WASM/module-cache behavior | CLI-only ACP/agent-host, orchestration scheduler, SPA/HTTP/Node; `connect-host` must not imply `cli` |
| TS native service | Current selection is core `[execution]` with native-owned Host/ProviderPort. The current-host completion selects `[execution,provider-host,compute]` and composes existing Rust owners: the workflow-control, observation/recovery and Compute whole-plan releases are accepted, and the public first workflow driver/guidance stay pending plan QC/QA and iteration integration (§6.5.1). Completing it is not merely changing the feature list. | old-daemon proxy, TS domain/SQL owner, duplicate engine/Host/WASM runtime; domain/default and Connect-only cohorts must not inherit native-service startup |

The existing pure `nexus-preset` closure and default-disabled MCA/core spoke edges are retained. The app's spoke dependency also disables defaults; only `connect-host` explicitly enables its real compute feature. Current Connect invoke code already serves compute with stored holder/module scope and a shared cache; removing WASM to satisfy a graph slogan would delete supported functionality. The ordinary CLI may retain orchestration **library** edges for chronology/cron/ops without starting an engine. Core domain closure remains engine/Host/WASM-free.

**App cohorts.** The app cohorts are `default = ["cli"]`, independent `connect-host`, and `nexus42` binary `required-features = ["cli"]`. `basic-cli`, `legacy-cli`, `web-embed` and the app-only `connect-client`/`embedded-mcp` selectors do not exist; the core `connect-client`/`embedded-mcp` **library** features remain. Generic ACP MCP descriptors remain. No second CLI, renamed legacy mode or no-op compatibility feature exists.

**Accepted feature correction target:** core `connect-client` explicitly implies `execution`, because its retained peer-control methods are implemented on `ExecutionHandle` and its peer table/configuration consume execution-owned registry types. The execution feature also owns the required `async-trait` dependency. `embedded-mcp` inherits that prerequisite through `connect-client`; it is not a separate execution activation rule. This is a selected library dependency, not implicit EngineOwner/Host/scheduler startup. Default remains `[]`, provider-host/compute are not added to Connect, and the application's independent `connect-host` cohort is unchanged. Preserve the retained consumers rather than cfg-gating them away. Prove domain, execution, connect-client and connect-client+embedded-mcp with separate lib-only `cargo check -p nexus-core --lib --no-default-features --locked` invocations, each adding only its cohort's `--features` selection; tests with self dev-dependency feature unification and graph inspection alone are not compilation evidence.

`CoreService::start_execution(&self, _providers: Arc<dyn ProviderPort>, mut deps: RunnerDeps) -> Result<Arc<ExecutionHandle>, ExecutionOpenError>` requires EngineOwner and preserves typed ownership/closing errors. Production prompt/tool/workspace dependencies must be supplied; default dependencies are not execution parity. Domain open starts no scheduler or Host. CoreService remains bound to the selected Creator/workspace generation; selection changes require reopen. Direct CLI adapters obtain a stored Principal, call typed family methods, and await close on success and failure without pool escape hatches or JSON dispatch.

**Current target:** complete retained workflow admission/list/inspect/context/control, same-run bounded observation/restart, and first-party Compute discovery/run/review/history/terminal-clear through the existing execution or compute owner. One Rust-owned production composition binds the admitted Creator/canonical workspace, real Host prompt/catalog, scheduler starter, workspace-intent recovery and conditionally shared WASM engine/cache/serializer before readiness. Public workflow events use durable root ownership and the existing epoch-based run rings, not Host identity. Context append precedes resume; cancellation success requires durable confirmed settlement, not provider acknowledgement. This target does not restore retired CLI entrances or alter SPOKE/version pins.

### 3.3 Stored Actor and process-session separation

Core admission validates stored ownership, World/binding/viewpoint and Character lifecycle_epoch, returning a non-deserializable Rust AdmittedActor. An owned context projection may cross napi but grants no authority. Separate Host-free activity/transition fences from the process Host session registry. Every Character DB/file/provider/terminal effect holds a shared activity lease; lifecycle/binding transitions require the corresponding exclusive lease and re-read stored state after acquisition. Extend the existing resource-lock discipline across processes with a stable per-Character OS lock; an in-process mutex alone cannot fence authorized direct CLI transitions against a live service. Busy is observable, no heartbeat takeover or long SQL transaction across SDK awaits.

Host owns one session/operation/tombstone registry and consumes core admission/leases. No-op transitions do not retire sessions; stale epoch or removed binding denies before effects. Memory/SOUL/ToM remain bearer/revision/cache scoped; context assembly has one implementation for Host, CLI, API and capabilities. Workspace durable content commits retain the existing intent/digest/recovery protocol in optional execution, not an engine requirement for basic file authoring.

## 4. Host and lifetime matrix

| Host | Product job | Lifetime | Current state | Product obligation |
| --- | --- | --- | --- | --- |
| Standalone Node TS service | Independent daemon/API for browser and the desktop utility | Process owned by the service; no Electron import | Browser vertical shipped | Every retained HTTP/middleware/stream family and generated client uses the real Rust owner. No old-daemon proxy, no 501 on a supported migrated family, no Electron import |
| `nexus42` basic CLI | Local authoring/storage | Process owned by the CLI | Graph/patch slice shipped Node/daemon/engine-free | Remaining retained basic leaves, including Works `list\|status\|use`, call Rust with the same constraint. Default authoring entry is this cohort, not an experimental extra binary |
| `nexus-runtime` | Integrator Connect host | Connect node lifetime; Ctrl-C shutdown | Connect-only preserved | Node-free Connect-only; no hidden Host/scheduler/SPA |
| Electron utility (shipped desktop host) | App-managed TS-service + native host | Stops with the app | Shipped desktop host | Durable-after-exit work still attaches to the independent service |
| Independent service/daemon | Durable-after-app-exit attach target | Independent of the GUI | Stopping the app must not kill an unowned service | Unchanged |

## 5. Provider matrix

| Consumer | Required capability | Node dependency |
| --- | --- | --- |
| TS service | Existing supported families behind stable Rust ports: ACP, Claude native, Codex, DSH | Node hosts the TS adapters |
| Independent Rust CLI/runtime | Only the ports those products actually use | **No** accidental Node requirement |
| Cancel proof | Both actual Rust ACP through LocalSet and a real stable-v1 TS ACP SDK adapter behind the Rust port | Same deterministic no-model protocol peer; exactly one selected adapter/owner. DSH remains `cancellation:false` and is never the cancel proof. |

Preserve verified readiness, complete-message streaming, recoverable workspace commit, cancel/settlement/replay, and sealed denial. Public first-run / Quick Start / live request stay deferred (current reading in §6.5.1). No paid or live model calls in planning or Execute unless a later user authorization names them. The TS service must not drop Claude/Codex/DSH.

The selected composition reuses actual maintained adapters: TS ACP uses `packages/nexus-provider-acp`; Claude/Codex/DSH use their existing Rust SDK-backed providers through the native ProviderPortAdapter. Independent Rust ACP keeps LocalSet. No unresearched replacement TS SDKs are required. Replace the native-open all-JS-or-all-native choice with one admitted provider multiplexer: choose at launch, retain owning port per session/operation, never fallback/re-dispatch through another adapter after effect admission. DSH cancel stays a truthful unsupported capability, not a migration error or success. Provider journal read/write/orphan settlement belongs to core; native environment state has no SQL pool. After-effect journal failure must preserve retained terminal delivery and prohibit redispatch.

## 6. CLI and operator disposition

**Product keep rule:** no supported **core/TS/HTTP-family/provider capability** is removed because its CLI leaf currently talks to the daemon. A **CLI entry** may be removed when it is local-HTTP-only and no public core method already owns the operator action. Classification of capability is destination/owner, not “keep every clap leaf”. A sample binding, TS-service launcher alias, proxy to the old daemon, or fake-success fallback is not completion. The retained set is the direct-core authoring/storage, cloud and Connect surface; a leaf outside that set loses its CLI entry while its capability owner stays.

### 6.1 Independent Rust CLI (basic local authoring/storage)

**Destination:** daemon-free, Node-free, full-engine-free product cohort. The ordinary default authoring/storage entries are this cohort (`default = ["cli"]`, `nexus42` bin `required-features = ["cli"]`).

### 6.2 Operator / service lifecycle

The public `nexus42 daemon …` names, the hidden `daemon-run`, `host-call`, and Model A `mcp serve` do not exist — the whole CLI group went with the old Rust host/SPA composition, and no replacement launcher exists.

Electron owns the desktop/Web host and the TS service process. There is no `nexus42` start/stop/status/ui/web/schedule alias and no new CLI HTTP client to `apps/nexus-service`. Missing local HTTP is not a CLI concern. Hosted schedules stay in TS/Electron; the only retained scheduling surface is `creator works cron` declaration editing. `host-call` and `mcp serve` have no CLI entries; the TS tool-execution route, Connect peer commands and embedded/TS MCP remain.

### 6.3 Headless / Connect

`nexus42 connect …` (feature `connect-host`) and `nexus-runtime` remain the integrator profile. Feature-off graphs stay libp2p-free. Do not fold full Host/scheduler/SPA into this binary.

### 6.4 Deferred, stubbed, hidden, or platform-only leaves

Those dormant leaves have no CLI entries: the retired `creator workspace clone` leaf is asserted unknown (`retired_creator_workspace_clone_is_unknown`), and the former coming-soon/platform-only rows (`workspace link|unlink|status`, `explore browse|search`, `platform context assemble`, `publish`, the `system preset` forwarding alias, top-level `sync`/`preset`/`capability`) are not CLI commands. Their capability owners are unaffected; an entry deletion is not a capability deletion.

### 6.5 Family destinations

**Implementation and exercise anchors:** `apps/nexus-service/src/routes.ts` mounts the family modules named above; `crates/nexus-core/src/service.rs`, `execution/handle_ops.rs`, and `execution/compute.rs` retain the Rust authority. Existing real-native HTTP journeys in `apps/nexus-service/tests/domain-http.test.mjs`, `actor-http.test.mjs`, `workflow-control-http.test.mjs`, `workflow-observation-http.test.mjs`, and `compute-http.test.mjs` exercise persisted domain effects, stored ownership, control/observation, and Compute accept/discard/clear. These are bounded family exercises, not blanket product acceptance, a Run Studio completeness claim, or permission for another live model request.

### 6.5.1 Current-target reading

These readings are the current target state, not a claim that every family is product-complete.

- Landed World/Work, Actor/Character, ordinary CLI, and desktop-host behavior are outside the remaining completion scope except where a selected workflow or Compute operation touches their existing authorization, CAS, or durable-effect contracts.
- Public workflow control includes schedule add/signal **and** `listSchedules`, `inspectSchedule`, `listWorkflowSessions`, `getWorkflowSession`, and `editCoreContext` in `apps/nexus-service/src/execution.ts`. Same-run `GET /v1/daemon/orchestration/sessions/{run_id}/events` is mounted by `workflow-observation.ts` over core-owned subscription/replay. The real-native control/observation journeys exercise append/resume, cancellation settlement, and restart visibility; these are not missing list/inspect/observation surfaces.
- The remaining unmounted identities in this execution surface are core-context **history reads** and schedule **label/delete** mutations (`execution.ts` explicitly retains `route_not_migrated` for them). That narrow gap is distinct from product-wide acceptance/QA.
- The TS-service Compute C1–C8 discovery/detail/run/history/accept/discard/terminal-clear routes are implemented in `compute.ts` over `crates/nexus-core/src/execution/compute.rs` and exercised by `compute-http.test.mjs`. What remains unqualified here is product acceptance/QA and Run Studio completeness, not backend route presence. A native feature flag or HTTP exercise alone does not close that UI/product obligation.
- Public first-run and Quick Start are exercised as the clean-home public first workflow (`scripts/public-first-workflow.mjs` in deterministic mode) plus the Quick Start/startup guidance. Those receipts are plan artifacts pending plan QC/QA and iteration integration; the deterministic receipt is loopback-only and is **not** live qualification.
- The single user-authorized live request has been **exercised once** against the pinned official origin: one admitted request, guard evidence complete, same-run replay, committed revision, restart without repeating the commit and confirmed cleanup all observed. It is a P3 plan-scope receipt, not a product shipment — it stays pending plan QC/QA and iteration integration, and the authorization is now spent, so a further live attempt needs a new explicit user grant.
- The live gate still refuses an environment that does not name the inherited credential channel: that case stops at `credentials_unavailable` with zero admissions, and an inherited model-origin override is refused rather than silently stripped. The driver never reads, copies, prints or persists the secret — its value stays with the sealed runtime's normal credential resolver — and a transport or authentication failure **after** an admission consumes the authorization, with no retry.

## 7. Concurrent writes (user-locked)

While a TS service is active, **authorized direct Rust CLI transactions are allowed**. Policy:

1. **One engine/effect owner.** CLI and HTTP/native callers invoke the same Rust commands. No second workflow/recovery engine and no dual-writer caches that diverge.
2. **Per-resource CAS/OCC remains the conflict tool** for the M1 slice (`expected_version` on World KB entity patch).
3. **Per-Work advisory locks** (`Works/<work_ref>/.lock`, [concurrency.md](../runtime/concurrency.md)) stay per-Work. They are not workspace-host ownership and not the M1 writer protocol.
4. **Workspace host / migration fencing** is a distinct protocol: atomic conditional acquisition, serialized migrations, cache invalidation, and event/resync visibility. WAL, a per-process mutex, or the current `runtime_lock` read-then-update helper is not sufficient proof.
5. Unauthorized or stale writes deny. Owner collision is observable. Crash/reopen does not duplicate effects.
6. Two stable OS lock files separate shared-writer/exclusive-migration admission from exclusive engine ownership. Engine takeover requires actual OS lock release and SQL conditional monotonic epoch acquisition, never a heartbeat timeout or read-then-update.
7. Persistent BEFORE-DML triggers require a current connection-local protocol/registration and engine epoch where appropriate. An old pre-opened or reopened binary lacking the guarded SQLite function cannot write. All current production writers migrate through the common pool factory; local non-OCC `kb edit` remains supported but revision/invalidation-visible. No legacy-writer exemption.
8. Same-transaction row revisions and bounded durable change records make cross-process updates visible. Reads use a fresh consistent snapshot; consumers checkpoint/poll and explicitly gap/resync when history is evicted. Before an engine effect, durable version/epoch is checked; polling is not a correctness fence.

Exclusive-owner refusal (reject all CLI writes while a service holds the workspace) is **not** the selected UX.

### 7.1 Independent service launch and attach

Schema-owned discovery/stop contracts precede both CLI launcher and TS host implementations; neither consumer requires the other's implementation to define the protocol. Discovery is a closed version 1 record containing random instance ID, diagnostic PID, canonical raw user home, nullable selected Creator/workspace/engine epoch for an explicitly uninitialized shell, tagged HTTP URL or Unix-socket endpoint, nullable TLS fingerprint, readiness and protocol version. No secrets are published.

Publish `<user_home>/.nexus42/run/service.json` atomically under a stable service-start lock, with current-user-only permissions and no symlink traversal. Publication and the `NEXUS_SERVICE_READY <json>` stdout line happen only after bind/open/recovery/provider readiness, or an explicitly uninitialized shell. CLI startup has a 15s deadline; it may terminate only the child it owns on failure. Foreground mode owns/signals the child; detached start confirms readiness before release. Basic commands never launch Node. Only explicit start/restart launches the installed/built TS entry; status and ordinary operator calls attach or report a concrete missing-runtime/service error. No auto-install/build or old-daemon fallback.

Authenticated status must match instance/home/endpoint before attach. Stop requires the expected instance ID and engine epoch; mismatches conflict without stopping. PID alone grants no ownership. A service removes discovery only if it still owns that instance record; unconfirmed close retains resources and diagnostic state. Preserve current port/home/logs/cert/TLS, Unix transport, remote-bind and opt-in peer-control behavior. Ordinary service/development startup uses the TS host; stable-interface TS/UI edits do not invoke Cargo.

The CLI launcher/attach half of this contract is **not** a CLI obligation. `nexus42` neither launches nor statuses the TS service — that service and its lifecycle belong to Electron/TS. The service-side contract above stays: discovery record, readiness ordering, atomic publication, ownership-matched removal and the retained close discipline. No explicit legacy mode exists, so no retained startup path selects one.

## 8. Native embedding and interruption (user-locked)

Thin **napi / Node-API** embedding is the selected boundary. TS watch or native-process restart **may interrupt** in-flight native work.

| Required operator-visible outcome | Forbidden claim |
| --- | --- |
| Unsupported continuation reports **Interrupted** | Uninterrupted native work across TS/native restart |
| Durable committed writes remain recovered as they are today | Transparent resume of uncommitted/in-flight native tasks |
| Process-fatal abort/OOM is not a Promise rejection that looks like app-level cancel | GC as shutdown; teardown awaiting dead JS |

Current LocalSet Drop join (~5s then detach) is implementation evidence, not a shutdown guarantee. Selected close budget is 5s total: stop admissions, cancel/drain until 2s, abort owned SDK/LocalSet work, terminate/reap owned direct child by 4s, join/release by 5s. Failed-open and env-death use the same retained cleanup owner; dead JS is never awaited. An unconfirmed task/thread retains its guard and reports Interrupted, not detached-success. LocalSet task IDs/abort handles and an independent shutdown channel replace caller-only timeouts. Every bridge includes item and UTF-8 byte accounting through TSFN, SDK queues, pull batches and the actual HTTP socket; terminal truth remains inspectable with explicit gap/resync on lost delivery.

## 9. Data, schema, and retirement

- Preserve schema/storage compatibility. No destructive reset, wipe, or dual-generation rollback.
- Future rollback **stops the current owner first**, then selects a known-compatible whole release with validated backup/restore.
- If any required step needs data loss, **stop and escalate** — this spec does not authorize it.
- Each family owns deletion of its old mixed-handler path; the old mixed-handler composition no longer exists. Public first-run remains deferred. A thin current-desktop adapter may call migrated services without a duplicate business engine; that adapter is not permission to defer any family.

## 10. Platforms and support (product cohorts)

Preserve current cohorts. Selected candidate pins distinguish tooling, standalone service runtime, ABI, dependency MSRV, compiler, and Electron's embedded runtime; proof remains required before claiming compatibility.

| Product | Current preserved support | Proof |
| --- | --- | --- |
| Desktop GUI | macOS arm64 and x86_64 (Electron host) | Electron evidence covers these macOS architectures with the accepted macOS 13+ floor; dual-architecture GUI and installed-deployment rows remain **[UNVERIFIED]** ([desktop-shell.md](../surfaces/desktop-shell.md) §12). Windows/Linux GUI is not current desktop support. |
| Headless `nexus-runtime` | Windows x64 MSVC, macOS arm64, Linux x64 GNU | Keep this matrix. Runtime is not a desktop UI artifact. |
| Browser / Studio | Current ESNext / system webview; Studio daemon-free on 5174 | No new browser matrix. No visual redesign. |
| Node / native API | Root tooling remains `node >=22.22`, `pnpm >=11` | Standalone service floor Node22.22.0; proof Node22.22.0 and24.20.0 independently. Node-API8; napi3.12.4/derive3.6.5/build2.4.2, CLI3.9.1, Rust1.98.1. napi's MSRV1.88 is not a workspace support claim. |

No new ARM Linux, musl, or Windows ARM support promise. Missing signing credentials are named Execute blockers for a **signed** release, not unsigned success and not a silent support shrink. Ordinary unsigned `.app`/`.dmg` packaging must succeed with no Apple credentials present; fail-closed applies to unsupported signed/release requests, never to that unsigned path.

Native targets: Windows x64 MSVC (Windows10/Server2016 ABI floor), macOS arm64/x64 (11.0 ABI floor), Linux x64 GNU (kernel4.18/glibc2.28). Use one existing sqlx0.9.0/libsqlite3-sys0.30.1 link. Electron44.3.0 embeds Node24.20.0/Chromium152.0.7977.78; packager20.3.0; TS ACP SDK1.4.0/zod4.6.2. Exact build candidates are Xcode16.4/deployment11.0, VS2022 17.14/v143/Windows SDK10.0.26100 and a glibc2.28 GNU sysroot/GCC10.1+.

**Desktop floor:** Electron44 requires macOS 13+ (Ventura); macOS 13+ on arm64 and x86_64 is the accepted Electron desktop target floor (user decision, 2026-09-13). Signing is release-only and is not implemented. The accepted floor is not qualification evidence: GUI/x64 and installed-deployment rows remain **[UNVERIFIED]** ([desktop-shell.md](../surfaces/desktop-shell.md) §12). Missing runners, SDKs or signing evidence block qualification, not permission to reduce cohorts.

## 11. Desktop: Electron host

Electron is the **shipped** desktop host on the accepted macOS 13+ (Ventura) floor for arm64 and x86_64. Packaging is unsigned; the GUI/x64 rows below stay unverified.

- Reuse `apps/web`, Design Studio, and `packages/nexus-ui`. No visual redesign and no second desktop UI.
- The shipped host's IPC/path/credential contract is owned by [desktop-shell.md](../surfaces/desktop-shell.md). Dual-architecture GUI and installed-deployment rows remain **[UNVERIFIED]** there.

## 12. Frontend DX (hard product goal)

Stable-interface **browser / Studio / shared-UI** edits and **TS route/provider** edits with an unchanged native/schema contract MUST invoke **zero Cargo**.

This is early loop restoration, not a rewrite of desktop packaging:

- Keep direct `dev:web` / `dev:design-studio`.
- Separate explicit backend refresh from ordinary frontend start.
- Reject incompatible schema/native artifacts instead of starting against them.
- Keep dist-load desktop (`dev:desktop`) distinct from HMR. Canonicalize the advertised vs actual HMR command (documented `dev:desktop:web` vs `pnpm --filter desktop dev`) without adding a new backend builder.
- Align daemon port selection (`NEXUS42_DAEMON_PORT`) with the Vite proxy (`VITE_DAEMON_URL`) so a non-default port is one endpoint, not two.

Do not credit already Cargo-free warm HMR as a new architecture gain. Cold and warm feedback are measured separately **after** a locked protocol; no historical speedup percentage is a product target.

## 13. Agreed flow, failures, and close (behavior baseline)

This vertical is the behavior-preservation baseline. The same flow extends to the remaining families; it is not replaced by a demo or an experimental second entry.

One agreed vertical (not a demo stub):

1. **Read** World KB graph for a real world (JSON DTO or human table). Observe per-entity `version`.
2. **Write** entity patch with `expected_version` matching that read, through (a) extracted Rust service, (b) old HTTP translation, (c) daemon-free basic CLI.
3. **Conflict:** stale version → `409 world_kb_conflict`; operator refetches graph; no silent overwrite.
4. **Unauthorized / foreign world:** stored-principal/scope denial (current 403/404 classes preserved).
5. **Concurrent:** authorized CLI patch while the service/host is active; one effect owner; both see the committed version.
6. **Provider:** cancelable streaming through both actual Rust ACP LocalSet and the real TypeScript ACP SDK behind the stable Rust port, with one selected adapter/owner. Cancel is not rollback of a committed KB patch.
7. **Browser:** existing NexusClient/BrowserClient consume generated DTOs and a generated slice-method contract for the same read/write/conflict/provider events against standalone TS. The current whole class is handwritten; only scan, not Host execution controls, exists today. An opt-in mount uses the existing canvas; new generated provider client methods execute in real browser context without a new visual surface. Full Pack/actor/API migration is not claimed; default shipped UI remains intact.
8. **Restart:** TS/native restart reports Interrupted for in-flight native work; committed KB rows remain.

Current graph projection caps (500 entities / 1000 stored relationships, human text notes truncation, no wire `truncated` flag yet) are **shipped behavior to preserve** unless a later plan explicitly changes the contract. World KB pagination/truncation backlog is out of scope.

## 14. Selected engineering acceptance targets (not measured or user-promised SLAs)

| Target | Fixed candidate threshold |
| --- | --- |
| Stable-interface UI / TS route/provider edit | Exactly zero Cargo/native-build calls; UI warm p95≤1s/max≤2s; TS restart p95≤2s/max≤5s |
| Concurrent DB | 100 barrier-synchronized writer pairs, exactly one same-version winner; independent resources both succeed; every read after acknowledgment sees the commit |
| Change visibility | Poll100ms; invalidation p95≤500ms/max≤1s; last4096 records or8MiB; old cursor explicitly resyncs |
| Close/cancel | Close5s total (proof tolerance250ms); cooperative ACP cancel p95≤2s/max≤3s; unconfirmed cleanup Interrupted with guard retained |
| Native/LocalSet | Native32 active/16 pending, pending1MiB; LocalSet16 pending/1MiB and32 active; TSFN16 queued/1MiB and32 active promises |
| Provider/pull | Per operation64 messages/1MiB,256KiB/event,4MiB emitted text; pull16 events/256KiB; reserved terminal+gap slots4KiB each; aggregate pending32MiB/environment |
| Socket pressure | 64KiB high-water mark, one outstanding frame≤512KiB; write(false) pauses pulls; drain≤2s then gap/disconnect with inspectable terminal |
| Existing run history | Preserve256 records/1MiB/run,512KiB frame,64 live+64 terminal rings,16 subscribers/run,16 pending frames/1MiB+control |
| Electron size/memory | Per architecture archive≤250MiB/installed≤600MiB; Electron-native total-owned-process idle RSS≤650MiB, active p95≤750MiB; service idle≤180MiB; 100-cycle retained growth≤20MiB |
| Electron startup/maintenance | Cold process→interactive graph p95≤5s/max≤8s; warm p95≤3s/max≤5s; explicit patch rebuild≤30min per architecture on declared runner |

Measurement protocol: same candidate hardware and seeded real DB (500 entities,1000 stored relationships); 30 warm samples after3 warmups,10 cold process launches (not a cache-purge claim),10 fault/recovery cycles per case,10-minute soak and100 open/close cycles; raw monotonic timings/artifact hashes/OS/compiler/RSS/child ownership are retained. Quantiles use nearest-rank p95. Electron is the replacement host, so its resource gate is an Electron-native absolute budget rather than a native-equivalence comparison. The 650MiB idle ceiling retains 88MiB/15.7% headroom above the measured 562MiB arm64 baseline while keeping the existing 750MiB active and20MiB retained-growth guards. Developer ID signing/notarization/stapling is release-only; development platform rows run locally or through GitHub Actions.

## 15. Non-goals (this target document)

- Claiming unmigrated daemon/CLI HTTP clients are already the target architecture
- Claiming the exercised subset is complete, or a production Electron release
- Parking any retained family
- UI redesign, new provider capabilities, second engine, permanent old-server fallback
- Removing Works reads (`creator works list|status|use`) or any retained leaf. CLI entries removed by this overlay are deletions of the *entry*, not of the capability
- Uninterrupted native continuation across TS restart
- New ARM Linux / musl / Windows ARM / Windows-or-Linux GUI support
- Unauthorized paid or live model requests. The public first-run live example stays unshipped as a product feature: the one short authorized request has been exercised once as a plan-scope receipt (not a release qualification) and is spent; an environment that does not name the credential channel still stops with zero admissions, a future admitted failure would consume a new authorization, and none of this reopens general live spend.
- Destructive data reset as migration/implementation shortcut; this does not remove the shipped explicitly confirmed desktop local-state-reset function, whose scope/fencing/recovery contract is preserved in [desktop-shell.md](../surfaces/desktop-shell.md) §8
- Browser/device/installed-deployment E2E as a development acceptance gate

## 16. Conflict with shipped Masters

A family that has landed on the target:

1. Implement current behavior against **this document** plus the retained Masters named in §0 (`cli-spec`, `desktop-shell`, `agent-host`, `concurrency`).
2. Use this document for destination, host/lifetime, CLI disposition, the agreed vertical identity, and the remaining keep rules.
3. When a remaining family lands on the target, fold the superseded section or record the deletion gate in the family plan. Do not leave two contradictory implementable topologies for the same family.

The old mixed-handler topology no longer exists to conflict with. The retained route families are implemented and exercised (§6.5), while their product acceptance/QA and Run Studio completeness remain distinct from that evidence. Do not label implemented routes “unexercised”, or claim the remaining product/distribution/first-run qualification obligations are delivered without their own evidence.

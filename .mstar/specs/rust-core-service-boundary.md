# Rust Core Service Boundary

**Status:** Accepted target — locked 2026-09-13; **v1.193 overlay delivered (P2 complete 2026-09-21).** The **M1 subset is exercised** (v1.189 PR #306, merge `71e01cf9`, 2026-09-15). **v1.192** delivered the Electron desktop cutover and retired Tauri. **v1.193 P2 delivered the remaining cutover:** the whole `nexus42 daemon` group, hidden `daemon-run`, Model A `mcp serve`/`host-call`, the app-only `legacy-cli` / `basic-cli` / `web-embed` / `connect-client` / `embedded-mcp` selectors, the `nexus-daemon-runtime` crate and the DaemonClient CLI leaves are **deleted** — retained leaves call core/cloud/Connect directly, with no TS-launcher alias and no permanent legacy fallback. This document is normative for that boundary and is self-contained; the per-leaf command inventory for the iteration is process material, not a tracked contract. Do **not** describe the `Current:` records below as the shipped state — they are the pre-cutover migration record.
**Document class:** Master
**Pillar (V1.122):** Cross-cutting — Harness, Canvas, and Computable consumption ends keep their product identities; this spec only locks the service-boundary target those pillars run on.
**Coordinates with:** [local-runtime-boundary.md](local-runtime-boundary.md), [daemon-runtime.md](daemon-runtime.md) (historical host spec — its crate was deleted in v1.193 P2), [cli-spec.md](cli-spec.md), [desktop-shell.md](desktop-shell.md), [web-ui.md](web-ui.md), [agent-host.md](agent-host.md), [concurrency.md](concurrency.md), [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md), [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md), [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md), [schemas-directory-layout.md](schemas-directory-layout.md)

## 0. Authority

| Layer | Authority | Honest claim |
| --- | --- | --- |
| **Shipped current policy** | Shipped Masters: `local-runtime-boundary`, `cli-spec`, `desktop-shell`, `agent-host`, `concurrency` (plus this document for the boundary itself) | The retired integrated `nexus42` daemon and the CLI `DaemonClient` leaves are deleted (v1.193 P2): retained families call the Rust authority directly, or through the standalone TS service for browser/Electron. Since the v1.192 cutover the shipped desktop is the Electron host and no Tauri sidecar shell is part of it. `daemon-runtime` is listed in *Coordinates with* only as the historical host Master — it is not an implementable authority |
| **Exercised M1 subset** | This document + merged v1.189 on `main` (`71e01cf9`) | World KB graph/patch, candidates read-only projection, napi/provider ACP vertical, standalone TS M1 routes, Cargo-free stable-interface DX, Electron development GO. Not full API/CLI cutover |
| **Accepted target** | This document | Transport-neutral Rust authority for every retained family, independent Rust CLI/runtime, complete TS service composition, proof-gated Electron desktop |
| **Wire DTOs** | `schemas/` → generated Rust + `@42ch/nexus-contracts` | Unchanged by this lock |
| **Product names** | Root `AGENTS.md` | `Nexus`, `nexus42`, `nexus-runtime`, `@42ch` — the retired integrated daemon runtime is no longer a product name |

**Activation rule:** a family uses the target topology only after the extracted Rust service is the single effect owner **and** the old mixed-handler path for that family has an explicit deletion owner and proof. **Delivered (v1.193 P2):** that gate fired for every retained family — the old mixed-handler path is the deleted daemon/SPA composition, so no shipped Master still describes an implementable daemon topology, and the retained SSOT is this document plus `cli-spec`, `desktop-shell` and the TS service. What has *not* been exercised is the rest of the program: the RFT-05–11 destinations listed in §7.5 (complete TS API, remaining World/Work families, Actor families) remain open, apart from the retained public first-run half of RFT-11 that v1.195 P3 exercises (§7.5.1). Do not describe an unexercised destination as delivered, and do not describe the M1 subset as the complete program.

Historical 2026-09-12 research (source baseline `3bb262b`) is advisory structure only. It is not runtime proof, a measurement, or this spec's authority. The merged v1.189 record for the exercised M1 subset is `71e01cf9e1a8c64cb68062ac9a08762e2156a925` (PR #306); the baseline has since advanced — v1.192 (RFT-09 cutover + unsigned packaging) and v1.193 P2 (obsolete-host retirement) are **merged on `main`**, so the retained `cli`/`connect-host` cohorts and the Electron host are shipped current state, not target state. Inherited v1.188 reliability remains `bbaae32d422b673576d683859b474da6bd787743`.

## 1. Problem

Operators and authors already have three consumption ends. The local stack that serves them mixes:

- authorized domain reads/writes, OCC/CAS, storage, and recovery inside daemon HTTP handlers and `WorkspaceState`
- a CLI that is often a thin HTTP client rather than a direct library caller
- provider/Host/ACP lifecycle inside the same integrated process as storage
- a Tauri desktop shell supervising a `nexus42` sidecar (retired in v1.192; the desktop is now the Electron host)

The target is a **behavior-preserving** cutover: one Rust authority for domain and execution truth; TypeScript for transport, auth composition, and high-churn provider SDKs; independent Rust products that do not require Node, Electron, or a daemon for basic authoring.

## 2. Frozen product identities

These names and roles stay. The refactor does not invent a second CLI, a second runtime product, a second contracts package, or a new first-party app.

| Identity | Role that stays |
| --- | --- |
| **Nexus** | Product |
| **`nexus42`** | User-facing CLI executable. **Delivered (v1.193 P2):** the ordinary default is the direct-core + cloud + optional Connect `cli` cohort; there is no operator CLI that launches or statuses the TS service and no internal `legacy-cli` mode. The shipped desktop remains the Electron host. |
| **Daemon runtime** | **Retired in v1.193 P2.** The integrated local host and the public `nexus42 daemon …` names are deleted (no `nexus42d`, no `nexus42 service` alias). The standalone TS service remains the HTTP host for browser/Electron; it is not a CLI destination. |
| **`nexus-runtime`** | Headless integrator executable. Connect-only profile. Must not boot a hidden full scheduler/Host/SPA. |
| **`@42ch/nexus-contracts`** | Published TypeScript contracts package |
| **`nexus-contracts` crate** | Monorepo-internal generated Rust types |

### Three consumption ends (unchanged)

| End | Surface | Target change |
| --- | --- | --- |
| Developers | `nexus42` CLI + local HTTP/API | **Delivered (v1.193 P2):** retained authoring/storage (including Works list/status/use) calls Rust directly with no Node/daemon; operator HTTP stays a TS composition for Electron/browser only — there is no `nexus42 daemon` group or CLI launcher. The M1 graph/patch slice that pioneered the direct path is now part of the ordinary `cli` cohort. |
| Content creators | `apps/web` + desktop shell wrapping the same SPA | No visual redesign. Browser uses generated `NexusClient`. The desktop host cut over to Electron in v1.192 (RFT-09 accepted) and stays exactly one desktop host. |
| Third-party users | `nexus-runtime` + Connect | Keep the existing Connect-only served-op profile. No first-party player. |

## 3. Topology before the v1.193 cutover (migration record)

**Historical record.** The numbered items below describe the mixed shipped
topology *before* v1.193 P2 finished the cutover; they are kept so the
migration is auditable, not as the current state. Delivered form: the
ordinary `nexus42` default is the `cli` cohort (no embedded `apps/web`, no
`legacy-cli`/`web-embed`, no daemon launch), and every retained CLI leaf
calls the Rust authority directly.

Kept as the record of the mix at lock time; nothing below is a supported instruction any more.

1. **`nexus42`** ordinary default is the `cli` cohort: direct core/cloud/Connect, with **no** embedded `apps/web`, **no** `nexus_daemon_runtime::boot::run_daemon` and **no** hidden `daemon-run` (all deleted in v1.193 P2). The **development/runtime entry cutover** to standalone TS + daemon-free CLI (RFT-07/RFT-08) landed with that deletion, not in M3.
2. **Daemon API** remains loopback HTTP under `/v1/daemon/*` for unmigrated families, with unguarded health/status/cert routes, API-key Tier-1, and API-key + active-Creator Tier-2. The M1 World KB graph/patch/candidates-read and provider-session slice also exist on standalone `apps/nexus-service`. Completing M2 requires every retained family on that TS service with a real Rust owner — not a 501, sample, or old-daemon proxy.
3. **Creator workflow/control CLI leaves** now call Rust (core/cloud/Connect) directly; the `DaemonClient` and the app-only cohort selectors are deleted (v1.193 P2). Several world/KB/memory/SOUL/cron/chronology/directive paths already owned local SQLite/filesystem directly and keep doing so. The M1 World KB graph/patch slice that pioneered the direct path is part of the ordinary `cli` cohort (RFT-08 complete, including Works `list|status|use`).
4. **World KB graph + entity patch** now have one Rust owner (`nexus-core`) with old HTTP translation **and** daemon-free CLI. They remain distinct from local-DB `creator world kb edit` (direct SQLite, no OCC). Candidate **promotion/merge/relationship writes**, packs, forks, rules, findings, and remaining World/Work families are **not** in the M1 subset.
5. **`nexus-runtime`** parses `--listen` / `--allow-peer` / `--home`, starts Connect, and **never** calls `run_daemon`. It is built `--no-default-features --features connect-host` — Connect-only and Node-free, with the `legacy-cli` coupling gone with that cohort (v1.193 P2). Do not fold full Host/scheduler/SPA into this binary.
6. **Desktop** ships as the Electron host (`apps/desktop-electron`) wrapping `apps/web/dist` with an app-managed TS service over the Rust authority. Support floor: macOS arm64 and x86_64. v1.192 delivered the formal switch (RFT-09) and the unsigned `.app`/`.dmg` half of RFT-10, and retired the Tauri v2 composition (`apps/desktop` plus its target-suffixed `nexus42` sidecar); that delivery supersedes the M1 Electron **development** GO as the desktop's product basis. Production signing (RFT-10) and dual-architecture GUI qualification remain open.
7. **Design Studio** is a daemon-free Vite gallery on port 5174. `pnpm run dev:web` / `dev:design-studio` already exist as direct entries.
8. **Providers:** Rust ACP/Claude/Codex/DSH remain. TS currently exposes the M1 ACP vertical; M2 must keep **all four** families available in the TS service through stable ports. DSH stays `cancellation:false` and is never the cancel proof. Independent Rust consumers must not require Node.

`WorkspaceState` is the current daemon aggregate, not the target facade. `nexus-local-db` `runtime_lock` read-then-update without an expected-holder predicate is **not** the target cross-process writer protocol.

## 4. Target topology

```text
 Basic Rust CLI ──────┐
 Connect-only host ───┼──> nexus-core ──> guarded SQLite / canonical spoke ports
 Old HTTP adapter ───┤        │
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
6. One engine/effect owner at a time. Temporary old HTTP adapters may call the extracted Rust service. No permanent dual writable engines and no silent feature removal.

### 4.1 Selected library/package boundaries

- `crates/nexus-core`: owned `CoreService`, `CoreOpenOptions`, opaque stored-authorized `Principal`, neutral `CoreError`. The M1 commands `open`, `active_principal`, `world_kb_graph`, `patch_world_kb_entity`, `changes`, `close` are **shipped**. M2 extends the same authority through family-named methods in private sibling modules. A narrow `CoreHomeService` handles pre-selection registration/configuration without holding a workspace pool. SQL pools/borrowed transactions/WorkspaceState and Host/SDK implementation types are not public service contracts. The current M1 `CoreService::pool` journal escape hatch must be removed through owned journal commands, with every native caller migrated; it is not a target extension seam.
- `crates/nexus-provider-ports`: schema-derived `ProviderCall`, `ProviderReply`, `ProviderEventBatch` and Send+Sync async `call`/bounded pull `next`; no engine or SDK implementation.
- `crates/nexus-core-node` + `packages/nexus-native`: environment-local thin ABI/facade; compose existing Host/ACP and core once. `packages/nexus-provider-acp` uses the actual stable-v1 ACP TypeScript SDK behind the Rust port.
- `nexus-spoke-adapter` keeps one conversion/upsert seam; default-on `compute` owns WASM/module-cache capability. Core disables that feature rather than importing orchestration for helpers.
- `crates/nexus-storage-guard` is the narrow audited SQLite FFI connection-function boundary; all business/domain crates retain unsafe-code prohibition.
- Products stay in `apps`: standalone `apps/nexus-service`, existing `apps/nexus42`, and the desktop host `apps/desktop-electron` (`apps/desktop` was retired with the Tauri composition in v1.192). Exactly one desktop host — no second desktop or competing desktop app.
- Same `nexus42` binary/parser. **Delivered (v1.193 P2):** the ordinary default is the direct-core + cloud + optional Connect `cli` cohort; `legacy-cli` / `web-embed` are deleted, not kept as an internal mode. A new helper CLI or second product name is prohibited.

### 4.2 Selective authority graph

These are the shipped dependency cohorts (v1.193 P2 delivered them), kept as the regression contract: an edge listed under "forbidden" must not reappear. `daemon-runtime` names the crate deleted in v1.193 P2.

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
| TS native service | Current selection on `main` is core `[execution]` with native-owned Host/ProviderPort. The v1.195 current-host completion selects `[execution,provider-host,compute]` and composes existing Rust owners: the workflow-control, observation/recovery and Compute whole-plan releases are accepted, and P3 adds the exercised public first workflow. Completing it is not merely changing the feature list, and the P3 driver/guidance stay pending plan QC/QA and iteration integration (§7.5.1). | old-daemon proxy, TS domain/SQL owner, duplicate engine/Host/WASM runtime; domain/default and Connect-only cohorts must not inherit native-service startup |

The existing pure `nexus-preset` closure and default-disabled MCA/core spoke edges are retained. The app's spoke dependency also disables defaults; only `connect-host` explicitly enables its real compute feature. Current Connect invoke code already serves compute with stored holder/module scope and a shared cache; removing WASM to satisfy a graph slogan would delete supported functionality. The ordinary CLI may retain orchestration **library** edges for chronology/cron/ops without starting an engine. Core domain closure remains engine/Host/WASM-free.

**Delivered (v1.193 P2).** The app cohorts are `default = ["cli"]`, independent `connect-host`, and `nexus42` binary `required-features = ["cli"]`. `basic-cli`, `legacy-cli`, `web-embed` and the app-only `connect-client`/`embedded-mcp` selectors are deleted; the core `connect-client`/`embedded-mcp` **library** features remain. The ACP factory that prescribed `nexus42 mcp serve` is deleted; generic ACP MCP descriptors remain. No second CLI, renamed legacy mode or no-op compatibility feature was introduced.

`CoreService::start_execution(&self, _providers: Arc<dyn ProviderPort>, mut deps: RunnerDeps) -> Result<Arc<ExecutionHandle>, ExecutionOpenError>` requires EngineOwner and preserves typed ownership/closing errors. Production prompt/tool/workspace dependencies must be supplied; default dependencies are not execution parity. Domain open starts no scheduler or Host. CoreService remains bound to the selected Creator/workspace generation; selection changes require reopen. Direct CLI adapters obtain a stored Principal, call typed family methods, and await close on success and failure without pool escape hatches or JSON dispatch.

Retire CLI `creator run`, `bootstrap`, Work `intake`/`resume-chain`, `preset run`, Character `run`/`soul reflect`, reference refresh, compute run and old runtime catalog/tool entrances where the complete direct production/observation closure does not exist. Preserve underlying Work/execution/Host/capture/SOUL/reference/compute libraries. `resume_driven_sessions` is not Work auto-chain; HostHandle query does not supply the Character CLI capture stream. Existing TS schedule add/signal and provider routes are not claimed as equivalent workflow replacements; TS compute run is currently route-not-migrated and forced native SOUL reflection has no synthesizer.

**Current target (v1.195, not yet delivered):** complete retained workflow admission/list/inspect/context/control, same-run bounded observation/restart, and first-party Compute discovery/run/review/history/terminal-clear through the existing execution or compute owner. One Rust-owned production composition binds the admitted Creator/canonical workspace, real Host prompt/catalog, scheduler starter, workspace-intent recovery and conditionally shared WASM engine/cache/serializer before readiness. Public workflow events use durable root ownership and the existing epoch-based run rings, not Host identity. Context append precedes resume; cancellation success requires durable confirmed settlement, not provider acknowledgement. This target does not restore retired CLI entrances or alter SPOKE/version pins.

Neutral production `HostPromptExecutor` moves into optional core execution with provider-host selected, and `AcpSoulNarrativeSynthesizer` into optional core execution. Existing core directive/pack/capability implementations replace daemon wrappers rather than gain duplicates. PATH enrichment moves to existing agent-host discovery for the CLI and remaining discovery probe; Connect-only runtime drops provider probing and gains no Host dependency. Retained semantic tests move to core before the obsolete Rust-host crate and server fixtures are deleted.

### 4.3 Stored Actor and process-session separation

Core admission validates stored ownership, World/binding/viewpoint and Character lifecycle_epoch, returning a non-deserializable Rust AdmittedActor. An owned context projection may cross napi but grants no authority. Separate Host-free activity/transition fences from the process Host session registry. Every Character DB/file/provider/terminal effect holds a shared activity lease; lifecycle/binding transitions require the corresponding exclusive lease and re-read stored state after acquisition. Extend the existing resource-lock discipline across processes with a stable per-Character OS lock; an in-process mutex alone cannot fence authorized direct CLI transitions against a live service. Busy is observable, no heartbeat takeover or long SQL transaction across SDK awaits.

Host owns one session/operation/tombstone registry and consumes core admission/leases. No-op transitions do not retire sessions; stale epoch or removed binding denies before effects. Memory/SOUL/ToM remain bearer/revision/cache scoped; context assembly has one implementation for Host, CLI, API and capabilities. Workspace durable content commits retain the existing intent/digest/recovery protocol in optional execution, not an engine requirement for basic file authoring.

## 5. Host and lifetime matrix

| Host | Product job | Lifetime | M1 (shipped) | M2 product obligation |
| --- | --- | --- | --- | --- |
| Integrated `nexus42` daemon + Daemon API | **Retired in v1.193 P2** (was the shipped operator/HTTP host for unmigrated families) | Was owned by the deleted `daemon start` | Deleted, not translated | The CLI group and the obsolete Rust host/SPA composition are gone. It was **not** retargeted at the TS service. TS/Electron remain the HTTP host |
| Standalone Node TS service | Independent daemon/API for browser and the desktop utility | Process owned by the service; no Electron import | RFT-04 M1 vertical shipped | Every retained M2 HTTP/middleware/stream family and generated client uses the real Rust owner. No old-daemon proxy, no 501 on a supported migrated family, no Electron import |
| `nexus42` basic CLI | Local authoring/storage | Process owned by the CLI | Graph/patch slice shipped Node/daemon/engine-free | Remaining retained basic leaves, including Works `list\|status\|use`, call Rust with the same constraint. Default authoring entry is this cohort, not an experimental extra binary |
| `nexus-runtime` | Integrator Connect host | Connect node lifetime; Ctrl-C shutdown | Connect-only preserved | Node-free Connect-only; the `legacy-cli` coupling is gone (v1.193 P2); no hidden Host/scheduler/SPA |
| Electron utility (shipped desktop host) | App-managed TS-service + native host | Stops with the app | RFT-03 development GO; v1.192 cutover delivered | Shipped desktop host since the v1.192 cutover (RFT-09 accepted). Durable-after-exit work still attaches to the independent service |
| Independent service/daemon | Durable-after-app-exit attach target | Independent of the GUI | Stopping the app must not kill an unowned service | Unchanged |
| Tauri sidecar (retired in v1.192) | Historical record: desktop packaging before the cutover | Sidecar child of the retired Tauri app | Retired with the accepted RFT-09 cutover and unsigned packaging | No product obligation. Retirement was explicit in v1.192, not a silent M2 deletion, and no second desktop UI was introduced |

## 6. Provider matrix

| Consumer | Required capability | Node dependency |
| --- | --- | --- |
| TS service | Existing supported families behind stable Rust ports: ACP, Claude native, Codex, DSH | Node hosts the TS adapters |
| Independent Rust CLI/runtime | Only the ports those products actually use | **No** accidental Node requirement |
| Cancel proof in M1 | Both actual Rust ACP through LocalSet and a real stable-v1 TS ACP SDK adapter behind the Rust port | Same deterministic no-model protocol peer; exactly one selected adapter/owner. DSH remains `cancellation:false` and is never the cancel proof. |

Preserve verified v1.188 readiness, complete-message streaming, recoverable workspace commit, cancel/settlement/replay, and sealed denial. Public first-run / Quick Start / live request stay deferred for M2 (RFT-11 / retained P5; the v1.195 current reading is in §7.5.1). No paid or live model calls in M2 planning or Execute unless a later user authorization names them. M1 cancel proof already exercised both actual Rust ACP through LocalSet and a real stable-v1 TS ACP SDK adapter; M2 must not drop Claude/Codex/DSH from the TS service.

Selected M2 composition reuses actual maintained adapters: TS ACP uses `packages/nexus-provider-acp`; Claude/Codex/DSH use their existing Rust SDK-backed providers through the native ProviderPortAdapter. Independent Rust ACP keeps LocalSet. No unresearched replacement TS SDKs are required. Replace the native-open all-JS-or-all-native choice with one admitted provider multiplexer: choose at launch, retain owning port per session/operation, never fallback/re-dispatch through another adapter after effect admission. DSH cancel stays a truthful unsupported capability, not a migration error or success. Provider journal read/write/orphan settlement belongs to core; native environment state has no SQL pool. After-effect journal failure must preserve retained terminal delivery and prohibit redispatch.

## 7. CLI and operator disposition

**Product keep rule (v1.193 overlay):** no supported **core/TS/HTTP-family/provider capability** is removed because its CLI leaf currently talks to the daemon. A **CLI entry** may be removed when it is local-HTTP-only and no public core method already owns the operator action. Classification of capability is destination/owner, not “keep every clap leaf”. A sample binding, TS-service launcher alias, proxy to the old daemon, or fake-success fallback is not completion. The retained set is the direct-core authoring/storage, cloud and Connect surface; a leaf outside that set loses its CLI entry while its capability owner stays.

### 7.1 Independent Rust CLI (basic local authoring/storage)

**Destination:** daemon-free, Node-free, full-engine-free product cohort completed in **RFT-08**. **Delivered (v1.193 P2):** the ordinary default authoring/storage entries are this cohort (`default = ["cli"]`, `nexus42` bin `required-features = ["cli"]`); M1 delivered **one real slice** of it, and it is not the whole basic CLI.

**M1 first slice (RFT-01) — delivered in v1.189:**

| Operator action | Current public command | Current transport | Target |
| --- | --- | --- | --- |
| Read World KB graph (entities + per-row `version`, relationships, source anchors) | `nexus42 creator world kb graph --world-id` | `GET /v1/daemon/worlds/{world_id}/kb/graph` | Same DTO through Rust service, old HTTP translation, **and** daemon-free CLI |
| Authorized entity write with expected-version CAS | `nexus42 creator world kb entity patch --world-id --entity-id --expected-version …` | `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` | Same CAS/409 `world_kb_conflict` semantics through all three callers |

Keep the existing distinction: local `creator world kb edit` is a different command (direct SQLite, no OCC). M1 does not merge or delete it.

Patch preserves create-or-update: absent entity + expected_version 0 + valid title/block type creates revision 1 with HTTP 200; absent + positive version conflicts. Existing merged/deleted denial, body/aliases/modules omission/null semantics, provenance, and graph caps remain. The core owns one private transaction for authorization, canonical spoke lifecycle, CAS, response projection and durable change commit.

The existing canvas unconditionally requests the read-only candidates list, so that existing projection/cursor/auth path is an ancillary M1 core/native/HTTP dependency. It must return real pending rows, not treat an unported error as an empty list. Candidate promotion/merge/relationship writes stay RFT-05. Existing canvas has no create-entity button: browser-context client creation followed by canvas observation proves the create branch without inventing UI.

**Works reads stay assigned to the complete basic CLI (RFT-08), with domain work in RFT-05.** `creator works list|status|use` are currently Daemon API-owned. They are **not** the M1 slice and **must not** be reclassified as operator-only or dropped because they are daemon-mediated today. M2 default-entry cutover does **not** move them to “wait for M3”.

Do **not** claim that all basic commands already work as direct library calls. Static inventory: 252 feature-on clap terminal leaves (247 default features). Many authoring leaves still require the daemon. Local SQLite/filesystem owners that already exist stay local; they are not a new architecture gain.

### 7.2 Operator / service lifecycle

**Delivered (v1.193 P2):** those public `nexus42 daemon …` names, the hidden `daemon-run`, `host-call`, and Model A `mcp serve` are **deleted** — the whole CLI group went with the obsolete Rust host/SPA composition, and no replacement launcher was introduced.

**Delivered (v1.193 P2):** those CLI names are deleted. Electron owns the desktop/Web host and the TS service process. There is no `nexus42` start/stop/status/ui/web/schedule alias and no new CLI HTTP client to `apps/nexus-service`. Missing local HTTP is not a CLI concern. Hosted schedules stay in TS/Electron; the only retained scheduling surface is `creator works cron` declaration editing — the `creator run` runner is removed with the other incomplete execution entrances (§4.2). `host-call` and `mcp serve` lose their CLI entries; the TS tool-execution route, Connect peer commands and embedded/TS MCP remain.

### 7.3 Headless / Connect

`nexus42 connect …` (feature `connect-host`) and `nexus-runtime` remain the integrator profile. Feature-off graphs stay libp2p-free. Do not fold full Host/scheduler/SPA into this binary.

### 7.4 Deferred, stubbed, hidden, or platform-only leaves

Historical roster (pre-v1.193 overlay) of the callable-but-incomplete leaves at that time:

- hidden hard-deprecated `creator workspace clone` — now an **unknown command** (the retired leaf is asserted as `retired_creator_workspace_clone_is_unknown`)
- coming-soon `workspace link|unlink|status`
- platform-only `explore browse|search`, deferred `platform context assemble`, coming-soon `publish`
- visible deprecated `system preset` forwarding alias; hidden but callable top-level `sync`/`preset`/`capability`

**Delivered (v1.193 P2):** these dormant rows lost their CLI entries. They are not domain capabilities, and the removal was explicit in the v1.193 overlay rather than a quiet cleanup.

### 7.5 Family destinations (program keys, not extra iterations)

RFT-00–04 shipped together as milestone **RFT-M1** (v1.189). RFT-05–08 are one milestone **RFT-M2** (v1.190), including default product-entry cutover. RFT-09–11 are **RFT-M3**. Do not split M2 into extra milestone iterations, and do not hide any retained M2 family in M3.

| Destination | What moves there | Milestone status |
| --- | --- | --- |
| RFT-00 | Inventory, decision/proof matrix, Cargo-free stable-backend UI loops | Shipped (v1.189) |
| RFT-01 | Neutral Rust service + M1 World KB graph/patch slice + concurrent writer protocol | Shipped (v1.189) |
| RFT-02 | napi adapter, provider ports, ACP LocalSet lifecycle | Shipped (v1.189) |
| RFT-03 | Native npm + packaged Electron **development** feasibility go/no-go | Shipped development GO (v1.189). Production signing is RFT-10 |
| RFT-04 | Standalone TS service + browser vertical of the M1 slice | Shipped (v1.189). Complete TS API is RFT-07 |
| RFT-05 | Remaining World/Work/KB/narrative/fork families, including Works reads/writes beyond the M1 slice | M2 / not started |
| RFT-06 | Actor/Character/admission/memory/context | M2 / not started |
| RFT-07 | Execution/scheduler/Host/providers/capabilities/MCP/Connect control plane **and** complete TS API/default service entry | M2 / not started |
| RFT-08 | Complete independent Rust CLI + headless product cutover (default authoring entry; Connect-only runtime). **v1.193 target:** no CLI operator control surface for the TS service | **Delivered (v1.193 P2)** — the ordinary `cli` cohort is the default authoring entry, `nexus-runtime` is the Connect-only headless binary, and no CLI operator control surface for the TS service exists. The unexercised destinations above stay open |
| RFT-09 | Formal desktop cutover (Electron host after the M1 development GO; reuse web/Studio; no visual redesign) | M3 / **delivered in v1.192** (accepted) |
| RFT-10 | Production distribution / Developer ID signing / notarization / stapling | M3. **v1.192 delivers the unsigned half** (`.app` and `.dmg`, both macOS architectures, no Apple credentials required); that delivery is not dual-architecture GUI qualification. Signing remains the durable destination and is a Non-Goal until explicit release authorization |
| RFT-11 | Obsolete-host retirement **and** retained v1.188 P5 public first-run / Quick Start / live request | **v1.192** retired Tauri. **v1.193 P2 delivered:** the remaining daemon/SPA/`legacy-cli` composition and the dormant CLI rows listed in §7.4 were retired in that iteration. **v1.195 P3** exercises the retained public first-run as a clean-home deterministic example with its Quick Start/startup guidance, and one user-authorized live request has been exercised against the pinned official origin (all of it pending plan QC/QA and iteration integration) — see §7.5.1 |

### 7.5.1 Current-target reading (v1.195)

The milestone labels above are historical program keys. They are not a blank-slate order, and they are not a claim that every family is product-complete.

- Landed World/Work, Actor/Character, ordinary CLI, and desktop-host behavior stay out of v1.195 except where a selected workflow or Compute operation touches their existing authorization, CAS, or durable-effect contracts.
- RFT-07 is partial on the public service. Mounted schedule add and schedule signal are not list, inspect, core-context steering, cancellation settlement, or same-workflow observation.
- The TS-service Compute / Run Studio rows in the v1.195 inventory are still open. A native `compute` feature flag is not that closure.
- RFT-11 public first-run and Quick Start: v1.195 P3 exercises them as the clean-home public first workflow (`scripts/public-first-workflow.mjs` in deterministic mode) plus the Quick Start/current-startup guidance that replaced the retired daemon text. Those are P3 plan artifacts pending plan QC/QA and iteration integration; the deterministic receipt is loopback-only and is **not** live qualification.
- The single user-authorized live request has been **exercised once** against the pinned official origin: one admitted request, guard evidence complete, same-run replay, committed revision, restart without repeating the commit and confirmed cleanup all observed. It is a P3 plan-scope receipt, not a product shipment — it stays pending plan QC/QA and iteration integration, and the authorization is now spent, so a further live attempt needs a new explicit user grant.
- The live gate still refuses an environment that does not name the inherited credential channel: that case stops at `credentials_unavailable` with zero admissions, and an inherited model-origin override is refused rather than silently stripped. The driver never reads, copies, prints or persists the secret — its value stays with the sealed runtime's normal credential resolver — and a transport or authentication failure **after** an admission consumes the authorization, with no retry. Retired daemon commands stay retired.

## 8. Concurrent writes (user-locked)

While a TS service **or** the current integrated host is active, **authorized direct Rust CLI transactions are allowed**. Policy:

1. **One engine/effect owner.** CLI and HTTP/native callers invoke the same Rust commands. No second workflow/recovery engine and no dual-writer caches that diverge.
2. **Per-resource CAS/OCC remains the conflict tool** for the M1 slice (`expected_version` on World KB entity patch).
3. **Per-Work advisory locks** (`Works/<work_ref>/.lock`, [concurrency.md](concurrency.md)) stay per-Work. They are not workspace-host ownership and not the M1 writer protocol.
4. **Workspace host / migration fencing** is a distinct protocol: atomic conditional acquisition, serialized migrations, cache invalidation, and event/resync visibility. WAL, a per-process mutex, or the current `runtime_lock` read-then-update helper is not sufficient proof.
5. Unauthorized or stale writes deny. Owner collision is observable. Crash/reopen does not duplicate effects.
6. Two stable OS lock files separate shared-writer/exclusive-migration admission from exclusive engine ownership. Engine takeover requires actual OS lock release and SQL conditional monotonic epoch acquisition, never a heartbeat timeout or read-then-update.
7. Persistent BEFORE-DML triggers require a current connection-local protocol/registration and engine epoch where appropriate. An old pre-opened or reopened binary lacking the guarded SQLite function cannot write. All current production writers migrate through the common pool factory; local non-OCC `kb edit` remains supported but revision/invalidation-visible. No legacy-writer exemption.
8. Same-transaction row revisions and bounded durable change records make cross-process updates visible. Reads use a fresh consistent snapshot; consumers checkpoint/poll and explicitly gap/resync when history is evicted. Before an engine effect, durable version/epoch is checked; polling is not a correctness fence.

Exclusive-owner refusal (reject all CLI writes while a service holds the workspace) is **not** the selected UX.

### 8.1 Independent service launch and attach

Schema-owned discovery/stop contracts precede both CLI launcher and TS host implementations; neither consumer requires the other's implementation to define the protocol. Discovery is a closed version 1 record containing random instance ID, diagnostic PID, canonical raw user home, nullable selected Creator/workspace/engine epoch for an explicitly uninitialized shell, tagged HTTP URL or Unix-socket endpoint, nullable TLS fingerprint, readiness and protocol version. No secrets are published.

Publish `<user_home>/.nexus42/run/service.json` atomically under a stable service-start lock, with current-user-only permissions and no symlink traversal. Publication and the `NEXUS_SERVICE_READY <json>` stdout line happen only after bind/open/recovery/provider readiness, or an explicitly uninitialized shell. CLI startup has a 15s deadline; it may terminate only the child it owns on failure. Foreground mode owns/signals the child; detached start confirms readiness before release. Basic commands never launch Node. Only explicit start/restart launches the installed/built TS entry; status and ordinary operator calls attach or report a concrete missing-runtime/service error. No auto-install/build or old-daemon fallback.

Authenticated status must match instance/home/endpoint before attach. Stop requires the expected instance ID and engine epoch; mismatches conflict without stopping. PID alone grants no ownership. A service removes discovery only if it still owns that instance record; unconfirmed close retains resources and diagnostic state. Preserve current port/home/logs/cert/TLS, Unix transport, remote-bind and opt-in peer-control behavior. Ordinary service/development startup switches to the TS host in M2; stable-interface TS/UI edits do not invoke Cargo.

**Delivered (v1.193 P2):** the CLI launcher/attach half of this contract is **not** a CLI obligation. `nexus42` neither launches nor statuses the TS service — that service and its lifecycle belong to Electron/TS. The service-side contract above stays: discovery record, readiness ordering, atomic publication, ownership-matched removal and the retained close discipline. The explicit legacy mode is deleted, so no retained startup path selects one.

## 9. Native embedding and interruption (user-locked)

Thin **napi / Node-API** embedding is the selected boundary. TS watch or native-process restart **may interrupt** in-flight native work.

| Required operator-visible outcome | Forbidden claim |
| --- | --- |
| Unsupported continuation reports **Interrupted** | Uninterrupted native work across TS/native restart |
| Durable committed writes remain recovered as they are today | Transparent resume of uncommitted/in-flight native tasks |
| Process-fatal abort/OOM is not a Promise rejection that looks like app-level cancel | GC as shutdown; teardown awaiting dead JS |

Current LocalSet Drop join (~5s then detach) is implementation evidence, not a shutdown guarantee. Selected close budget is 5s total: stop admissions, cancel/drain until 2s, abort owned SDK/LocalSet work, terminate/reap owned direct child by 4s, join/release by 5s. Failed-open and env-death use the same retained cleanup owner; dead JS is never awaited. An unconfirmed task/thread retains its guard and reports Interrupted, not detached-success. LocalSet task IDs/abort handles and an independent shutdown channel replace caller-only timeouts. Every bridge includes item and UTF-8 byte accounting through TSFN, SDK queues, pull batches and the actual HTTP socket; terminal truth remains inspectable with explicit gap/resync on lost delivery.

## 10. Data, schema, and retirement

- Preserve schema/storage compatibility. No destructive reset, wipe, or dual-generation rollback in M1.
- Future rollback **stops the current owner first**, then selects a known-compatible whole release with validated backup/restore.
- If any required step needs data loss, **stop and escalate** — this spec does not authorize it.
- Each migrated family owns deletion of its old mixed-handler path; that work completed with the v1.193 P2 cutover (the obsolete daemon/SPA composition and its crate are deleted) and the Tauri composition was retired in v1.192. P5 first-run remains RFT-11. A thin current-desktop adapter may call migrated services without a duplicate business engine; that adapter is not permission to defer any M2 family.

## 11. Platforms and support (product cohorts)

Preserve current cohorts. Selected candidate pins distinguish tooling, standalone service runtime, ABI, dependency MSRV, compiler, and Electron's embedded runtime; proof remains required before claiming compatibility.

| Product | Current preserved support | M1 proof |
| --- | --- | --- |
| Desktop GUI | macOS arm64 and x86_64 (Electron host since the v1.192 cutover) | Electron evidence covers these macOS architectures with the accepted macOS 13+ floor; dual-architecture GUI and installed-deployment rows remain **[UNVERIFIED]** ([desktop-shell.md](desktop-shell.md) §12). Windows/Linux GUI is not current desktop support. |
| Headless `nexus-runtime` | Windows x64 MSVC, macOS arm64, Linux x64 GNU | Keep this matrix. Runtime is not a desktop UI artifact. |
| Browser / Studio | Current ESNext / system webview; Studio daemon-free on 5174 | No new browser matrix. No visual redesign. |
| Node / native API | Root tooling remains `node >=22.22`, `pnpm >=11` | Standalone service floor Node22.22.0; proof Node22.22.0 and24.20.0 independently. Node-API8; napi3.12.4/derive3.6.5/build2.4.2, CLI3.9.1, Rust1.98.1. napi's MSRV1.88 is not a workspace support claim. |

No new ARM Linux, musl, or Windows ARM support promise. Missing signing credentials are named Execute blockers for a **signed** release, not unsigned success and not a silent support shrink. **v1.192 amendment (2026-09-19):** ordinary unsigned `.app`/`.dmg` packaging must succeed with no Apple credentials present; fail-closed applies to unsupported signed/release requests, never to that unsigned path.

Native targets: Windows x64 MSVC (Windows10/Server2016 ABI floor), macOS arm64/x64 (11.0 ABI floor), Linux x64 GNU (kernel4.18/glibc2.28). Use one existing sqlx0.9.0/libsqlite3-sys0.30.1 link. Electron44.3.0 embeds Node24.20.0/Chromium152.0.7977.78; packager20.3.0; TS ACP SDK1.4.0/zod4.6.2. Exact build candidates are Xcode16.4/deployment11.0, VS2022 17.14/v143/Windows SDK10.0.26100 and a glibc2.28 GNU sysroot/GCC10.1+.

**Desktop floor (resolved 2026-09-13):** Electron44 requires macOS13+ (Ventura); the retired Tauri config did not state a `minimumSystemVersion`. The user accepted macOS 13+ on arm64 and x86_64 as the Electron desktop target floor; v1.192 delivered the RFT-09 cutover, so the floor now governs the shipped Electron host rather than a future target. The M1 development GO did not require signed dual-architecture execution (signing is release-only / RFT-10 durable). **v1.192** does not implement signing. The accepted floor is not qualification evidence: GUI/x64 and installed-deployment rows remain **[UNVERIFIED]** ([desktop-shell.md](desktop-shell.md) §12). Missing runners, SDKs or signing evidence block qualification, not permission to reduce cohorts.

## 12. Desktop: Electron shipped (delivered in v1.192), Tauri retired

Electron is the **shipped** desktop host — v1.192 delivered the cutover and unsigned packaging — on the accepted macOS 13+ (Ventura) floor for arm64 and x86_64 (user decision, 2026-09-13). The RFT-03 package / signing / security / resource go/no-go conditions still bound what is claimed: the delivery is unsigned and the historical GUI/x64 rows below stay unverified.

- Reuse `apps/web`, Design Studio, and `packages/nexus-ui`. No visual redesign and no second desktop UI.
- P3 was feasibility, not production distribution (RFT-10) and not Tauri retirement (RFT-11); v1.192 delivered the RFT-09 cutover and retired the Tauri composition.
- **No-go does not complete M1.** M1 recorded a **development GO**, authorized by the v1.189 compass and native matrix run `34881345823`, not shipped desktop migration or production signing; v1.192 then delivered the RFT-09 product switch with unsigned app+DMG only, superseding that GO as the desktop's product basis. The historical decision JSON remains blocked (13 pass / 0 fail / 22 missing-or-unobserved, including x64 GUI rows); the CI matrix does not prove those rows passed. RFT-11 retired the replaced Tauri family after host/packaging acceptance; the still-consumed daemon/SPA composition and callable dormant CLI rows were **not** retired by that desktop change and stayed until v1.193 P2 removed them (see the delivered sentence at the end of this bullet). Product does not silently choose an alternative desktop. **Delivered (v1.193 P2):** the still-retained daemon/SPA composition and the dormant CLI rows are retired in that iteration (§7.2, §7.4); the desktop half stays as delivered.
- Tauri sidecar/IPC/path-guard behavior is retired with the composition; the shipped host's IPC/path/credential contract is owned by [desktop-shell.md](desktop-shell.md). Dual-architecture GUI and installed-deployment rows remain **[UNVERIFIED]** there.

## 13. Frontend DX (hard product goal)

Stable-interface **browser / Studio / shared-UI** edits and **TS route/provider** edits with an unchanged native/schema contract MUST invoke **zero Cargo**.

This is early loop restoration, not a rewrite of desktop packaging:

- Keep direct `dev:web` / `dev:design-studio`.
- Separate explicit backend refresh from ordinary frontend start.
- Reject incompatible schema/native artifacts instead of starting against them.
- Keep dist-load desktop (`dev:desktop`) distinct from HMR. Canonicalize the advertised vs actual HMR command (documented `dev:desktop:web` vs `pnpm --filter desktop dev`) without adding a new backend builder.
- Align daemon port selection (`NEXUS42_DAEMON_PORT`) with the Vite proxy (`VITE_DAEMON_URL`) so a non-default port is one endpoint, not two.

Do not credit already Cargo-free warm HMR as a new architecture gain. Cold and warm feedback are measured separately **after** a locked protocol; no historical speedup percentage is a product target.

## 14. M1 agreed flow, failures, and close

This vertical **shipped in v1.189**. Keep it as the behavior-preservation baseline. M2 extends the same flow to remaining families; it does not replace this slice with a demo or an experimental second entry.

One agreed vertical (not a demo stub):

1. **Read** World KB graph for a real world (JSON DTO or human table). Observe per-entity `version`.
2. **Write** entity patch with `expected_version` matching that read, through (a) extracted Rust service, (b) old HTTP translation, (c) daemon-free basic CLI.
3. **Conflict:** stale version → `409 world_kb_conflict`; operator refetches graph; no silent overwrite.
4. **Unauthorized / foreign world:** stored-principal/scope denial (current 403/404 classes preserved).
5. **Concurrent:** authorized CLI patch while the service/host is active; one effect owner; both see the committed version.
6. **Provider (RFT-02, same milestone):** cancelable streaming through both actual Rust ACP LocalSet and the real TypeScript ACP SDK behind the stable Rust port, with one selected adapter/owner. Cancel is not rollback of a committed KB patch.
7. **Browser (RFT-04):** existing NexusClient/BrowserClient consume generated DTOs and a generated slice-method contract for the same read/write/conflict/provider events against standalone TS. The current whole class is handwritten; only scan, not Host execution controls, exists today. An opt-in mount uses the existing canvas; new generated provider client methods execute in real browser context without a new visual surface. Full Pack/actor/API migration is not claimed; default shipped UI remains intact.
8. **Restart:** TS/native restart reports Interrupted for in-flight native work; committed KB rows remain.

Current graph projection caps (500 entities / 1000 stored relationships, human text notes truncation, no wire `truncated` flag yet) are **shipped behavior to preserve** unless a later plan explicitly changes the contract. M1 does not take World KB pagination/truncation backlog.

## 15. Selected engineering acceptance targets (not measured or user-promised SLAs)

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

Measurement protocol: same candidate hardware and seeded real DB (500 entities,1000 stored relationships); 30 warm samples after3 warmups,10 cold process launches (not a cache-purge claim),10 fault/recovery cycles per case,10-minute soak and100 open/close cycles; raw monotonic timings/artifact hashes/OS/compiler/RSS/child ownership are retained. Quantiles use nearest-rank p95. Electron is the replacement host, so its resource gate is an Electron-native absolute budget rather than a Tauri-equivalence comparison (user direction, 2026-09-14). The 650MiB idle ceiling retains 88MiB/15.7% headroom above the measured 562MiB arm64 baseline while keeping the existing 750MiB active and20MiB retained-growth guards. Developer ID signing/notarization/stapling is release-only; development platform rows run locally or through GitHub Actions (user direction, 2026-09-14).

## 16. Non-goals (this target document)

- Claiming unmigrated daemon/CLI HTTP clients are already the target architecture
- Claiming the exercised M1 subset is complete RFT-M2 or a production Electron release (RFT-M3)
- Parking any retained RFT-05–08 family in M3
- UI redesign, new provider capabilities, second engine, permanent old-server fallback
- Removing Works reads (`creator works list|status|use`) or any retained leaf. CLI entries removed by this overlay are deletions of the *entry*, not of the capability
- Uninterrupted native continuation across TS restart
- New ARM Linux / musl / Windows ARM / Windows-or-Linux GUI support
- Unauthorized paid or live model requests. Historical P5 public first-run stays unshipped on `main`, and the v1.195 P3 example is a plan-scope receipt rather than a release qualification. v1.195 may execute only the one short request named by that iteration's Decision D5, after deterministic prerequisites and a one-request guard; that request has now been exercised once under the user's explicit authorization and is spent, an environment that does not name the credential channel still stops with zero admissions, a future admitted failure would consume a new authorization, and none of this reopens general live spend.
- Destructive data reset as migration/implementation shortcut; this does not remove the shipped explicitly confirmed desktop local-state-reset function, whose scope/fencing/recovery contract is preserved in [desktop-shell.md](desktop-shell.md) §8
- Browser/device/installed-deployment E2E as a development acceptance gate

## 17. Conflict with shipped Masters

A family that has landed on the target:

1. Implement current behavior against **this document** plus the retained Masters named in §0 (`cli-spec`, `desktop-shell`, `agent-host`, `concurrency`) — the deleted composition's Master (`daemon-runtime.md`) is history, not an implementable authority.
2. Use this document for destination, host/lifetime, CLI disposition, the delivered M1 vertical identity, and the remaining RFT-05–11 keep/cutover rules.
3. When a remaining family lands on the target, fold the superseded section or record the deletion gate in the family plan. Do not leave two contradictory implementable topologies for the same family.

**Delivered (v1.193 P2):** the M1 World KB graph/patch, napi ACP, TS M1 vertical **and** the remaining retained families all fired their gates — the old mixed-handler topology no longer exists to conflict with. The rule above now binds only the unexercised RFT-05–11 destinations (§7.5), which must not be described as delivered before their own gates fire.

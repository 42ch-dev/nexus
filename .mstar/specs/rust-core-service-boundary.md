# Rust Core Service Boundary

**Status:** Accepted target — locked 2026-09-13. The **M1 subset is exercised** (v1.189 PR #306, merge `71e01cf9`, 2026-09-15): World KB graph/patch through one Rust core, daemon-free basic-cli slice, thin napi ACP port, standalone TS M1 vertical, and Electron **development** GO. **v1.192 delivers the desktop slice of M3:** the formal cutover to the Electron host (RFT-09), the unsigned `.app`/`.dmg` half of RFT-10, and retirement of the replaced Tauri composition (RFT-11); the RFT-05–08 families and the retained daemon/SPA/dormant-CLI families are **not** fully migrated. This document does **not** replace shipped Masters for unmigrated families until those family gates fire.
**Document class:** Master
**Pillar (V1.122):** Cross-cutting — Harness, Canvas, and Computable consumption ends keep their product identities; this spec only locks the service-boundary target those pillars run on.
**Coordinates with:** [local-runtime-boundary.md](local-runtime-boundary.md), [daemon-runtime.md](daemon-runtime.md), [cli-spec.md](cli-spec.md), [desktop-shell.md](desktop-shell.md), [web-ui.md](web-ui.md), [agent-host.md](agent-host.md), [concurrency.md](concurrency.md), [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md), [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md), [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md), [schemas-directory-layout.md](schemas-directory-layout.md)

## 0. Authority

| Layer | Authority | Honest claim |
| --- | --- | --- |
| **Shipped current policy** | Shipped Masters above (`local-runtime-boundary`, `daemon-runtime`, `cli-spec`, `desktop-shell`, `agent-host`, `concurrency`) | Unmigrated families still use the integrated `nexus42` daemon, Daemon API HTTP, and CLI `DaemonClient`; since the v1.192 cutover the shipped desktop is the Electron host and no Tauri sidecar shell is part of it |
| **Exercised M1 subset** | This document + merged v1.189 on `main` (`71e01cf9`) | World KB graph/patch, candidates read-only projection, napi/provider ACP vertical, standalone TS M1 routes, Cargo-free stable-interface DX, Electron development GO. Not full API/CLI cutover |
| **Accepted target** | This document | Transport-neutral Rust authority for every retained family, independent Rust CLI/runtime, complete TS service composition, proof-gated Electron desktop |
| **Wire DTOs** | `schemas/` → generated Rust + `@42ch/nexus-contracts` | Unchanged by this lock |
| **Product names** | Root `AGENTS.md` | `Nexus`, `nexus42`, integrated daemon runtime, `nexus-runtime`, `@42ch` |

**Activation rule:** a family uses the target topology only after the extracted Rust service is the single effect owner **and** the old mixed-handler path for that family has an explicit deletion owner and proof. The M1 World KB graph/patch + native ACP + TS M1 vertical families have fired that gate. Until other families fire it, shipped Masters remain the implementable SSOT for those families. Do not describe unmigrated code as already on the target, and do not describe the M1 subset as the complete program.

Historical 2026-09-12 research (source baseline `3bb262b`) is advisory structure only. It is not runtime proof, a measurement, or this spec's authority. Current integrated baseline for planning is merged v1.189 on `main` at `71e01cf9e1a8c64cb68062ac9a08762e2156a925` (PR #306). Inherited v1.188 reliability remains `bbaae32d422b673576d683859b474da6bd787743`.

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
| **`nexus42`** | User-facing CLI executable. In M2, ordinary basic commands call Rust directly and public operator commands control the standalone TS service. An explicitly selected internal legacy mode may remain for the retained integrated-daemon consumers until RFT-11; the shipped desktop no longer needs it, because v1.192 cut the desktop over to the Electron host. |
| **Daemon runtime** | Current integrated host for unmigrated families, not the M2 default service destination. Keep public `nexus42 daemon …` names; do not introduce a separate `nexus42d` product binary. |
| **`nexus-runtime`** | Headless integrator executable. Connect-only profile. Must not boot a hidden full scheduler/Host/SPA. |
| **`@42ch/nexus-contracts`** | Published TypeScript contracts package |
| **`nexus-contracts` crate** | Monorepo-internal generated Rust types |

### Three consumption ends (unchanged)

| End | Surface | Target change |
| --- | --- | --- |
| Developers | `nexus42` CLI + local HTTP/API | M1: graph/patch basic-cli slice is daemon-free. M2: remaining basic authoring/storage (including Works list/status/use) calls Rust directly with no Node/daemon; operator HTTP remains a TS composition over the same Rust authority and becomes the default service entry. |
| Content creators | `apps/web` + desktop shell wrapping the same SPA | No visual redesign. Browser uses generated `NexusClient`. The desktop host cut over to Electron in v1.192 (RFT-09 accepted) and stays exactly one desktop host. |
| Third-party users | `nexus-runtime` + Connect | Keep the existing Connect-only served-op profile. No first-party player. |

## 3. Currently shipped topology (honest mix; not the complete target)

Do not rewrite **unmigrated** families as if they had already moved. Do not hide the M1 subset that **has** moved.

1. **`nexus42`** default graph still embeds `apps/web` and can start `nexus_daemon_runtime::boot::run_daemon` (foreground or hidden `daemon-run`). Ordinary default **development/runtime entry cutover** to standalone TS + daemon-free basic CLI is an M2 product decision (RFT-07/RFT-08), not M3.
2. **Daemon API** remains loopback HTTP under `/v1/daemon/*` for unmigrated families, with unguarded health/status/cert routes, API-key Tier-1, and API-key + active-Creator Tier-2. The M1 World KB graph/patch/candidates-read and provider-session slice also exist on standalone `apps/nexus-service`. Completing M2 requires every retained family on that TS service with a real Rust owner — not a 501, sample, or old-daemon proxy.
3. **Most creator workflow/control CLI leaves** still call `DaemonClient`. Several world/KB/memory/SOUL/cron/chronology/directive paths still own local SQLite/filesystem directly. The M1 **basic-cli** cohort already calls Rust for World KB graph/patch without Node/daemon. Remaining basic authoring/storage, including Works `list|status|use`, stays assigned to RFT-08.
4. **World KB graph + entity patch** now have one Rust owner (`nexus-core`) with old HTTP translation **and** daemon-free CLI. They remain distinct from local-DB `creator world kb edit` (direct SQLite, no OCC). Candidate **promotion/merge/relationship writes**, packs, forks, rules, findings, and remaining World/Work families are **not** in the M1 subset.
5. **`nexus-runtime`** parses `--listen` / `--allow-peer` / `--home`, starts Connect, and **never** calls `run_daemon`. M2 must drop `legacy-cli` coupling while staying Connect-only and Node-free. Do not fold full Host/scheduler/SPA into this binary.
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
- Same `nexus42` binary/parser: `basic-cli` selected build uses `--no-default-features --features basic-cli`; `legacy-cli` preserves default unported features and Connect until RFT-08 **completes the default cutover**. A new helper CLI, a second product name, or a default feature loss is prohibited.

### 4.2 Selective authority graph

These are target dependency cohorts, not claims that the current graph already satisfies them:

| Cohort | Selected edges | Forbidden accidental edges |
| --- | --- | --- |
| Core domain/default | contracts, home-layout, guarded local-db, knowledge/narrative, Creator memory, MCA, pure preset; spoke-adapter with defaults disabled | daemon-runtime, orchestration engine, Host/ACP host, graph-flow, WASM, Axum, napi, Connect |
| Core `execution` | optional existing orchestration, coordinator/scheduler/capability/workspace commit; injected ProviderPort | transport host, unconditional WASM |
| Core `provider-host` | optional existing Host/ACP adapters and one admitted session registry | implicit scheduler/execution startup |
| Core `compute` | explicit execution plus WASM/spoke compute, including optional orchestration compute edges | default/domain/Connect-host activation |
| Peer-control operator | optional reverse-invoke/Connect-client/MCP adapters | activation by Connect-host product |
| CLI `cloud-client` | existing explicit cloud HTTP/auth/sync clients, retained in the ordinary default; core separately reuses the existing guarded Outbox implementation for local operations | daemon/Host/engine/Node |
| Ordinary CLI | `basic-cli,operator-client,cloud-client`; direct local commands plus lightweight HTTP/process-launch and existing cloud clients | default legacy-cli/web-embed or Node spawn for basic commands |
| Connect-only runtime | core canonical invoke and selected spoke-connect/libp2p | legacy-cli, daemon, full Host/scheduler/WASM/SPA/Node |
| TS native service | selected execution/provider-host/compute and peer-control authority through thin napi | old-daemon proxy/dependency, Electron |

Disable spoke defaults throughout the domain closure, including MCA. Offline preset parsing/validation cannot import the current graph-flow-backed loader wholesale: extract the pure grammar, assets/source hash, expression and validation closure into a leaf `nexus-preset`; retain graph builders and AgentBinding construction in orchestration. Validators consume non-executable capability metadata shared with the existing registry, not a second executable catalog. Move the one serialized PresetSourceIdentity definition without changing durable bytes; migrate all imports and remove obsolete re-exports. Preserve the existing basic-CLI direct allowlist and prove normal/build dependency closure separately for basic-only, ordinary default and Connect-host.

`CoreService::start_execution(Arc<dyn ProviderPort>)` returns an opaque ExecutionHandle only with EngineOwner. Domain open starts no scheduler or Host. CoreService remains bound to the selected Creator/workspace generation; switching selection invalidates the old Principal and requires reopen, never in-place pool retargeting. No generic string-method/JSON domain dispatcher substitutes for family-typed commands.

### 4.3 Stored Actor and process-session separation

Core admission validates stored ownership, World/binding/viewpoint and Character lifecycle_epoch, returning a non-deserializable Rust AdmittedActor. An owned context projection may cross napi but grants no authority. Separate Host-free activity/transition fences from the process Host session registry. Every Character DB/file/provider/terminal effect holds a shared activity lease; lifecycle/binding transitions require the corresponding exclusive lease and re-read stored state after acquisition. Extend the existing resource-lock discipline across processes with a stable per-Character OS lock; an in-process mutex alone cannot fence authorized direct CLI transitions against a live service. Busy is observable, no heartbeat takeover or long SQL transaction across SDK awaits.

Host owns one session/operation/tombstone registry and consumes core admission/leases. No-op transitions do not retire sessions; stale epoch or removed binding denies before effects. Memory/SOUL/ToM remain bearer/revision/cache scoped; context assembly has one implementation for Host, CLI, API and capabilities. Workspace durable content commits retain the existing intent/digest/recovery protocol in optional execution, not an engine requirement for basic file authoring.

## 5. Host and lifetime matrix

| Host | Product job | Lifetime | M1 (shipped) | M2 product obligation |
| --- | --- | --- | --- | --- |
| Integrated `nexus42` daemon + Daemon API | Current shipped operator/HTTP host for unmigrated families | Process owned by `daemon start` / desktop-host supervision | May translate M1's extracted family to the one Rust service | Public operator **commands keep their names and meanings** and explicitly launch/attach to the standalone TS service. Old daemon composition is a temporary adapter, not a second engine and not Done. Final crate/host deletion is M3/RFT-11 |
| Standalone Node TS service | Independent daemon/API for browser and the desktop utility | Process owned by the service; no Electron import | RFT-04 M1 vertical shipped | Every retained M2 HTTP/middleware/stream family and generated client uses the real Rust owner. No old-daemon proxy, no 501 on a supported migrated family, no Electron import |
| `nexus42` basic CLI | Local authoring/storage | Process owned by the CLI | Graph/patch slice shipped Node/daemon/engine-free | Remaining retained basic leaves, including Works `list\|status\|use`, call Rust with the same constraint. Default authoring entry is this cohort, not an experimental extra binary |
| `nexus-runtime` | Integrator Connect host | Connect node lifetime; Ctrl-C shutdown | Connect-only preserved | Stay Node-free Connect-only; lose `legacy-cli` coupling; no hidden Host/scheduler/SPA |
| Electron utility (shipped desktop host) | App-managed TS-service + native host | Stops with the app | RFT-03 development GO; v1.192 cutover delivered | Shipped desktop host since the v1.192 cutover (RFT-09 accepted). Durable-after-exit work still attaches to the independent service |
| Independent service/daemon | Durable-after-app-exit attach target | Independent of the GUI | Stopping the app must not kill an unowned service | Unchanged |
| Tauri sidecar (retired in v1.192) | Historical record: desktop packaging before the cutover | Sidecar child of the retired Tauri app | Retired with the accepted RFT-09 cutover and unsigned packaging | No product obligation. Retirement was explicit in v1.192, not a silent M2 deletion, and no second desktop UI was introduced |

## 6. Provider matrix

| Consumer | Required capability | Node dependency |
| --- | --- | --- |
| TS service | Existing supported families behind stable Rust ports: ACP, Claude native, Codex, DSH | Node hosts the TS adapters |
| Independent Rust CLI/runtime | Only the ports those products actually use | **No** accidental Node requirement |
| Cancel proof in M1 | Both actual Rust ACP through LocalSet and a real stable-v1 TS ACP SDK adapter behind the Rust port | Same deterministic no-model protocol peer; exactly one selected adapter/owner. DSH remains `cancellation:false` and is never the cancel proof. |

Preserve verified v1.188 readiness, complete-message streaming, recoverable workspace commit, cancel/settlement/replay, and sealed denial. Public first-run / Quick Start / live request stay deferred (RFT-11 / retained P5). No paid or live model calls in M2 planning or Execute unless a later user authorization names them. M1 cancel proof already exercised both actual Rust ACP through LocalSet and a real stable-v1 TS ACP SDK adapter; M2 must not drop Claude/Codex/DSH from the TS service.

Selected M2 composition reuses actual maintained adapters: TS ACP uses `packages/nexus-provider-acp`; Claude/Codex/DSH use their existing Rust SDK-backed providers through the native ProviderPortAdapter. Independent Rust ACP keeps LocalSet. No unresearched replacement TS SDKs are required. Replace the native-open all-JS-or-all-native choice with one admitted provider multiplexer: choose at launch, retain owning port per session/operation, never fallback/re-dispatch through another adapter after effect admission. DSH cancel stays a truthful unsupported capability, not a migration error or success. Provider journal read/write/orphan settlement belongs to core; native environment state has no SQL pool. After-effect journal failure must preserve retained terminal delivery and prohibit redispatch.

## 7. CLI and operator disposition

**Product keep rule:** no supported command, HTTP family, or provider capability is removed because it currently talks to the daemon. Classification is **destination**, not deletion. A sample binding, an opt-in experimental entry, a proxy to the old daemon, or a fake-success fallback is not completion.

### 7.1 Independent Rust CLI (basic local authoring/storage)

**Destination:** daemon-free, Node-free, full-engine-free product cohort completed in **RFT-08 (M2)**. Ordinary default authoring/storage entry points switch in M2. M1 delivered **one real slice**, now shipped; it is not the whole basic CLI.

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

Public command names and help/error meanings stay. **M2 (RFT-07/RFT-08) switches their destination** from the integrated daemon process to **explicit control of the standalone TS service** (`apps/nexus-service`): start/stop/restart/status/logs/doctor/ui (visible alias `web`), `nexus42 daemon schedule …`, Host/ACP session supervision, provider scan, `host-call`, and feature-gated `mcp serve`. Hidden `daemon-run` is not a user-facing default.

Missing TS runtime is an **explicit error**, never a silent spawn of the old mixed daemon and never a successful no-op. M2 may keep a thin translation adapter that calls the same Rust authority; it must not keep a second writable engine. **RFT-11** owns deleting the still-retained obsolete daemon/SPA composition after replacements exist; the Tauri part was retired in v1.192 with the accepted desktop cutover. Do not park any retained M2 family in M3 by calling it “operator leftover”.

### 7.3 Headless / Connect

`nexus42 connect …` (feature `connect-host`) and `nexus-runtime` remain the integrator profile. Feature-off graphs stay libp2p-free. Do not fold full Host/scheduler/SPA into this binary.

### 7.4 Deferred, stubbed, hidden, or platform-only leaves

Callable but incomplete rows remain inventory rows until a named plan owns them:

- hidden hard-deprecated `creator workspace clone`
- coming-soon `workspace link|unlink|status`
- platform-only `explore browse|search`, deferred `platform context assemble`, coming-soon `publish`
- visible deprecated `system preset` forwarding alias; hidden but callable top-level `sync`/`preset`/`capability`

Removal only at RFT-11 with help/docs/parity proof. Never a quiet cleanup.

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
| RFT-08 | Complete independent Rust CLI + headless product cutover (default authoring entry; operator commands control TS; Connect-only runtime) | M2 / not started |
| RFT-09 | Formal desktop cutover (Electron host after the M1 development GO; reuse web/Studio; no visual redesign) | M3 / **delivered in v1.192** (accepted) |
| RFT-10 | Production distribution / Developer ID signing / notarization / stapling | M3. **v1.192 delivers the unsigned half** (`.app` and `.dmg`, both macOS architectures, no Apple credentials required); that delivery is not dual-architecture GUI qualification. Signing remains the durable destination and is a Non-Goal until explicit release authorization |
| RFT-11 | Obsolete-host retirement **and** retained v1.188 P5 public first-run / Quick Start / live request | M3. **v1.192 retires the Tauri composition, after accepted RFT-09 and unsigned packaging.** Current source inventory confirms default `legacy-cli`/`web-embed`, integrated daemon start, embedded SPA and DaemonClient consumers remain; dormant CLI rows have real handlers. Both families are kept until retained-leaf/default-switch completion, zero-consumer evidence and/or reviewed help/docs/parity proof authorize a later retirement. Retained P5 stays out |

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

Authenticated status must match instance/home/endpoint before attach. Stop requires the expected instance ID and engine epoch; mismatches conflict without stopping. PID alone grants no ownership. A service removes discovery only if it still owns that instance record; unconfirmed close retains resources and diagnostic state. Preserve current port/home/logs/cert/TLS, Unix transport, remote-bind and opt-in peer-control behavior. Ordinary service/development startup switches to the TS host in M2; stable-interface TS/UI edits do not invoke Cargo. The explicit legacy mode is not that default.

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
- Each migrated family owns deletion of its old mixed-handler path. RFT-11 owns the remaining obsolete daemon/SPA composition **after** accepted replacements; the Tauri composition was retired in v1.192. P5 first-run remains RFT-11. A thin current-desktop adapter may call migrated services in M2/M3 without a duplicate business engine; that adapter is not permission to defer M2 families.

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
- **No-go does not complete M1.** M1 recorded a **development GO**, authorized by the v1.189 compass and native matrix run `34881345823`, not shipped desktop migration or production signing; v1.192 then delivered the RFT-09 product switch with unsigned app+DMG only, superseding that GO as the desktop's product basis. The historical decision JSON remains blocked (13 pass / 0 fail / 22 missing-or-unobserved, including x64 GUI rows); the CI matrix does not prove those rows passed. RFT-11 retired the replaced Tauri family after host/packaging acceptance; verified still-consumed daemon/SPA and callable dormant CLI rows stay. Product does not silently choose an alternative desktop.
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
- Removing Works reads or any supported leaf by classification
- Uninterrupted native continuation across TS restart
- New ARM Linux / musl / Windows ARM / Windows-or-Linux GUI support
- Paid/live model requests; P5 public first-run
- Destructive data reset as migration/implementation shortcut; this does not remove the shipped explicitly confirmed desktop local-state-reset function, whose scope/fencing/recovery contract is preserved in [desktop-shell.md](desktop-shell.md) §8
- Browser/device/installed-deployment E2E as a development acceptance gate

## 17. Conflict with shipped Masters

Until a family migration is exercised:

1. Implement current behavior against the shipped Master for that domain.
2. Use this document for destination, host/lifetime, CLI disposition, the delivered M1 vertical identity, and M2/M3 keep/cutover rules.
3. When a family lands on the target, fold the shipped Master section or record the deletion gate in the family plan. Do not leave two contradictory implementable topologies for the same family.

The M1 World KB graph/patch, napi ACP, and TS M1 vertical families have landed. Remaining families still follow this conflict rule until their gates fire.

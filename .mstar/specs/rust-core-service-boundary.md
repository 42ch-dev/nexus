# Rust Core Service Boundary

**Status:** Accepted target — V1.189, locked 2026-09-13 (not shipped). Activation is gated by exercised family migration. This document does **not** replace shipped Masters for current behavior until those gates fire.
**Document class:** Master
**Pillar (V1.122):** Cross-cutting — Harness, Canvas, and Computable consumption ends keep their product identities; this spec only locks the service-boundary target those pillars run on.
**Coordinates with:** [local-runtime-boundary.md](local-runtime-boundary.md), [daemon-runtime.md](daemon-runtime.md), [cli-spec.md](cli-spec.md), [desktop-shell.md](desktop-shell.md), [web-ui.md](web-ui.md), [agent-host.md](agent-host.md), [concurrency.md](concurrency.md), [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md), [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md), [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md), [schemas-directory-layout.md](schemas-directory-layout.md)

## 0. Authority

| Layer | Authority | Honest claim |
| --- | --- | --- |
| **Shipped current policy** | Shipped Masters above (`local-runtime-boundary`, `daemon-runtime`, `cli-spec`, `desktop-shell`, `agent-host`, `concurrency`) | Integrated `nexus42` daemon, Daemon API HTTP, Tauri sidecar shell, CLI `DaemonClient` for many leaves |
| **Accepted target** | This document | Transport-neutral Rust authority, independent Rust CLI/runtime, TS service composition, proof-gated Electron |
| **Wire DTOs** | `schemas/` → generated Rust + `@42ch/nexus-contracts` | Unchanged by this lock |
| **Product names** | Root `AGENTS.md` | `Nexus`, `nexus42`, integrated daemon runtime, `nexus-runtime`, `@42ch` |

**Activation rule:** a family uses the target topology only after the extracted Rust service is the single effect owner **and** the old mixed-handler path for that family has an explicit deletion owner and proof. Until then, shipped Masters remain the implementable SSOT. Do not describe current code as already migrated.

Historical 2026-09-12 research (source baseline `3bb262b`) is advisory structure only. It is not runtime proof, a measurement, or this spec's authority. Current integrated baseline for planning is merged v1.188 on `main` at `bbaae32d422b673576d683859b474da6bd787743`.

## 1. Problem

Operators and authors already have three consumption ends. The local stack that serves them mixes:

- authorized domain reads/writes, OCC/CAS, storage, and recovery inside daemon HTTP handlers and `WorkspaceState`
- a CLI that is often a thin HTTP client rather than a direct library caller
- provider/Host/ACP lifecycle inside the same integrated process as storage
- a Tauri desktop shell that supervises a `nexus42` sidecar

The target is a **behavior-preserving** cutover: one Rust authority for domain and execution truth; TypeScript for transport, auth composition, and high-churn provider SDKs; independent Rust products that do not require Node, Electron, or a daemon for basic authoring.

## 2. Frozen product identities

These names and roles stay. The refactor does not invent a second CLI, a second runtime product, a second contracts package, or a new first-party app.

| Identity | Role that stays |
| --- | --- |
| **Nexus** | Product |
| **`nexus42`** | User-facing CLI executable. Daemon remains an *internal process mode* of this binary until RFT-11 retires obsolete composition. |
| **Daemon runtime** | Integrated into `nexus42` (`nexus42 daemon start` → `nexus-daemon-runtime`). No separate `nexus42d` product binary. |
| **`nexus-runtime`** | Headless integrator executable. Connect-only profile. Must not boot a hidden full scheduler/Host/SPA. |
| **`@42ch/nexus-contracts`** | Published TypeScript contracts package |
| **`nexus-contracts` crate** | Monorepo-internal generated Rust types |

### Three consumption ends (unchanged)

| End | Surface | Target change |
| --- | --- | --- |
| Developers | `nexus42` CLI + local HTTP/API | Basic authoring/storage CLI calls Rust directly. Operator HTTP remains a TS composition over the same Rust authority. |
| Content creators | `apps/web` + desktop shell wrapping the same SPA | No visual redesign. Browser uses generated `NexusClient`. Desktop host may change only after an accepted P3 go. |
| Third-party users | `nexus-runtime` + Connect | Keep the existing Connect-only served-op profile. No first-party player. |

## 3. Currently shipped topology (not the target)

Do not rewrite these as if they had already moved.

1. **`nexus42`** default graph embeds `apps/web` and starts `nexus_daemon_runtime::boot::run_daemon` (foreground or hidden `daemon-run`).
2. **Daemon API** is loopback HTTP under `/v1/daemon/*`, with unguarded health/status/cert routes, API-key Tier-1, and API-key + active-Creator Tier-2.
3. **Most creator workflow/control CLI leaves** call `DaemonClient`. Several world/KB/memory/SOUL/cron/chronology/directive paths still own local SQLite/filesystem directly.
4. **World KB graph + entity patch** are daemon HTTP today: `GET /v1/daemon/worlds/{world_id}/kb/graph` and `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` with `--expected-version` CAS. They are distinct from local-DB `creator world kb edit` (direct SQLite, no OCC).
5. **`nexus-runtime`** parses `--listen` / `--allow-peer` / `--home`, starts Connect, and **never** calls `run_daemon`.
6. **Desktop** is a shipped Tauri v2 shell (`apps/desktop`) wrapping `apps/web/dist` plus a target-suffixed `nexus42` sidecar. Support floor: macOS arm64 and x86_64. There is **no** Electron product lane in current source/CI.
7. **Design Studio** is a daemon-free Vite gallery on port 5174. `pnpm run dev:web` / `dev:design-studio` already exist as direct entries.
8. **Providers** live in `nexus-agent-host`: ACP (`streaming=true`, `cancellation=true`), Claude native (both true), Codex (turn interrupt), DSH (`streaming=true`, `cancellation=false`). ACP LocalSet is a dedicated OS-thread bridge with bounded Drop join; it is not a transparent restart fabric.

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
 Electron utility proof (app-owned) / independent service (attach-owned)
```

Rules:

1. The facade is **not** a renamed daemon or a universal `WorkspaceState`.
2. Owned schema-derived DTOs and receipts cross the facade. SQL pools, borrowed transactions, and environment-local handles do not.
3. TypeScript does not own domain SQL, stored-principal authorization, or a second workflow/recovery engine.
4. The TS service **never imports Electron**. Electron main owns windows/OS. A utility process is the preferred *app-managed* TS-service host. Work that must outlive app exit attaches to an **independent** service, not that utility.
5. Browser/renderer never loads `.node`, secrets, or native handles.
6. One engine/effect owner at a time. Temporary old HTTP adapters may call the extracted Rust service. No permanent dual writable engines and no silent feature removal.

### 4.1 Selected library/package boundaries

- `crates/nexus-core`: owned `CoreService`, `CoreOpenOptions`, opaque stored-authorized `Principal`, neutral `CoreError`; `open`, `active_principal`, `world_kb_graph`, `patch_world_kb_entity`, `changes`, `close`. No daemon, Host, orchestration, Axum, napi or SQL pool/borrow in the public API.
- `crates/nexus-provider-ports`: schema-derived `ProviderCall`, `ProviderReply`, `ProviderEventBatch` and Send+Sync async `call`/bounded pull `next`; no engine or SDK implementation.
- `crates/nexus-core-node` + `packages/nexus-native`: environment-local thin ABI/facade; compose existing Host/ACP and core once. `packages/nexus-provider-acp` uses the actual stable-v1 ACP TypeScript SDK behind the Rust port.
- `nexus-spoke-adapter` keeps one conversion/upsert seam; default-on `compute` owns WASM/module-cache capability. Core disables that feature rather than importing orchestration for helpers.
- `crates/nexus-storage-guard` is the narrow audited SQLite FFI connection-function boundary; all business/domain crates retain unsafe-code prohibition.
- Products stay in `apps`: standalone `apps/nexus-service`, existing `apps/nexus42`, current `apps/desktop`, and private non-published `apps/desktop-electron` feasibility host. The proof host is not a permanent second desktop.
- Same `nexus42` binary/parser: `basic-cli` selected build uses `--no-default-features --features basic-cli`; `legacy-cli` preserves default unported features and Connect until RFT-08. A new helper CLI or a default feature loss is prohibited.

## 5. Host and lifetime matrix

| Host | Product job | Lifetime | M1 obligation |
| --- | --- | --- | --- |
| Integrated `nexus42` daemon + Daemon API | Current shipped operator/HTTP host | Process owned by `daemon start` / sidecar | Remains the public operator surface until RFT-07/RFT-11 replacement. May translate M1's extracted family to the one Rust service. |
| Standalone Node TS service | Independent daemon/API for browser and (later) desktop utility | Process owned by the service; no Electron import | RFT-04. Must load host config before runtimes, expose truthful readiness, and close with bounded teardown. |
| `nexus-runtime` | Integrator Connect host | Connect node lifetime; Ctrl-C shutdown | Preserve Connect-only profile. Shared DB uses the same guarded writer/migration protocol and one workspace effect owner; WAL alone is insufficient and does not authorize a hidden Host/scheduler. |
| Electron utility (preferred candidate) | App-managed TS-service + native host | Stops with the app | RFT-03 proof only. Not a sandbox, not task recovery, not an independent daemon. |
| Independent service/daemon | Durable-after-app-exit attach target | Independent of the GUI | Stopping the app must not kill an unowned service. |
| Tauri sidecar (shipped) | Current desktop packaging | Sidecar child of the Tauri app | Stay until RFT-09/RFT-11 after an accepted P3 decision. P3 no-go does not silently delete Tauri. |

## 6. Provider matrix

| Consumer | Required capability | Node dependency |
| --- | --- | --- |
| TS service | Existing supported families behind stable Rust ports: ACP, Claude native, Codex, DSH | Node hosts the TS adapters |
| Independent Rust CLI/runtime | Only the ports those products actually use | **No** accidental Node requirement |
| Cancel proof in M1 | Both actual Rust ACP through LocalSet and a real stable-v1 TS ACP SDK adapter behind the Rust port | Same deterministic no-model protocol peer; exactly one selected adapter/owner. DSH remains `cancellation:false` and is never the cancel proof. |

Preserve verified v1.188 readiness, complete-message streaming, recoverable workspace commit, cancel/settlement/replay, and sealed denial. Public first-run / Quick Start / live request stay deferred (RFT-11 / retained P5). No paid or live model calls in M1 planning or Execute unless a later user authorization names them.

## 7. CLI and operator disposition

**Product rule:** no supported command is removed because it currently talks to the daemon. Classification is destination, not deletion.

### 7.1 Independent Rust CLI (basic local authoring/storage)

**Destination:** daemon-free, Node-free, full-engine-free product cohort completed in RFT-08. M1 delivers **one real slice**, not the whole basic CLI.

**M1 first slice (RFT-01) — locked product choice, technical fit confirmed from canonical handler/store/spoke paths:**

| Operator action | Current public command | Current transport | Target |
| --- | --- | --- | --- |
| Read World KB graph (entities + per-row `version`, relationships, source anchors) | `nexus42 creator world kb graph --world-id` | `GET /v1/daemon/worlds/{world_id}/kb/graph` | Same DTO through Rust service, old HTTP translation, **and** daemon-free CLI |
| Authorized entity write with expected-version CAS | `nexus42 creator world kb entity patch --world-id --entity-id --expected-version …` | `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` | Same CAS/409 `world_kb_conflict` semantics through all three callers |

Keep the existing distinction: local `creator world kb edit` is a different command (direct SQLite, no OCC). M1 does not merge or delete it.

Patch preserves create-or-update: absent entity + expected_version 0 + valid title/block type creates revision 1 with HTTP 200; absent + positive version conflicts. Existing merged/deleted denial, body/aliases/modules omission/null semantics, provenance, and graph caps remain. The core owns one private transaction for authorization, canonical spoke lifecycle, CAS, response projection and durable change commit.

The existing canvas unconditionally requests the read-only candidates list, so that existing projection/cursor/auth path is an ancillary M1 core/native/HTTP dependency. It must return real pending rows, not treat an unported error as an empty list. Candidate promotion/merge/relationship writes stay RFT-05. Existing canvas has no create-entity button: browser-context client creation followed by canvas observation proves the create branch without inventing UI.

**Works reads stay assigned to the final basic CLI.** `creator works list|status|use` are currently Daemon API-owned. They are **not** the M1 slice and **must not** be reclassified as operator-only or dropped because they are daemon-mediated today. Destination: RFT-08 (with domain family work in RFT-05). M1 inventory must keep them as retained basic reads.

Do **not** claim that all basic commands already work as direct library calls. Static inventory: 252 feature-on clap terminal leaves (247 default features). Many authoring leaves still require the daemon. Local SQLite/filesystem owners that already exist stay local; they are not a new architecture gain.

### 7.2 Operator / service lifecycle

These stay on the **current public surface** until an explicit public replacement in RFT-07 / RFT-11:

- `nexus42 daemon start|stop|restart|status|logs|doctor|ui` (visible alias `web`)
- `nexus42 daemon schedule …` (13 leaves)
- hidden `daemon-run`
- Host/ACP session supervision, provider scan, `host-call`
- feature-gated `mcp serve`

Moving them requires a named replacement product, not a silent CLI-only rewrite.

### 7.3 Headless / Connect

`nexus42 connect …` (feature `connect-host`) and `nexus-runtime` remain the integrator profile. Feature-off graphs stay libp2p-free. Do not fold full Host/scheduler/SPA into this binary.

### 7.4 Deferred, stubbed, hidden, or platform-only leaves

Callable but incomplete rows remain inventory rows until a named plan owns them:

- hidden hard-deprecated `creator workspace clone`
- coming-soon `workspace link|unlink|status`
- platform-only `explore browse|search`, deferred `platform context assemble`, coming-soon `publish`
- hidden compatibility aliases (`system preset`, top-level `sync`/`preset`)

Removal only at RFT-11 with help/docs/parity proof. Never a quiet cleanup.

### 7.5 Family destinations (program keys, not extra iterations)

| Destination | What moves there |
| --- | --- |
| RFT-00 | Inventory, decision/proof matrix, Cargo-free stable-backend UI loops |
| RFT-01 | Neutral Rust service + M1 World KB graph/patch slice + concurrent writer protocol |
| RFT-02 | napi adapter, provider ports, ACP LocalSet lifecycle |
| RFT-03 | Native npm + packaged Electron feasibility go/no-go |
| RFT-04 | Standalone TS service + browser vertical of the M1 slice |
| RFT-05 | Remaining World/Work/KB/narrative/fork families, including Works reads/writes beyond the M1 slice |
| RFT-06 | Actor/Character/admission/memory/context |
| RFT-07 | Execution/scheduler/Host/providers/capabilities/MCP/Connect control plane |
| RFT-08 | Complete independent Rust CLI + headless product cutover |
| RFT-09 | Desktop cutover (only after P3 go, or after an explicit alternative decision) |
| RFT-10 | Production distribution |
| RFT-11 | Obsolete-host retirement **and** retained v1.188 P5 public first-run / Quick Start / live request |

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
- Each migrated family owns deletion of its old mixed-handler path. RFT-11 owns obsolete daemon/SPA/Tauri composition **after** accepted replacements. P5 first-run remains RFT-11.

## 11. Platforms and support (product cohorts)

Preserve current cohorts. Selected candidate pins distinguish tooling, standalone service runtime, ABI, dependency MSRV, compiler, and Electron's embedded runtime; proof remains required before claiming compatibility.

| Product | Current preserved support | M1 proof |
| --- | --- | --- |
| Desktop GUI | macOS arm64 and x86_64 (Tauri shipped) | Electron proof covers these macOS architectures with the accepted macOS 13+ target floor if P3 proceeds. Windows/Linux GUI is not current desktop support. |
| Headless `nexus-runtime` | Windows x64 MSVC, macOS arm64, Linux x64 GNU | Keep this matrix. Runtime is not a desktop UI artifact. |
| Browser / Studio | Current ESNext / system webview; Studio daemon-free on 5174 | No new browser matrix. No visual redesign. |
| Node / native API | Root tooling remains `node >=22.22`, `pnpm >=11` | Standalone service floor Node22.22.0; proof Node22.22.0 and24.20.0 independently. Node-API8; napi3.12.4/derive3.6.5/build2.4.2, CLI3.9.1, Rust1.98.1. napi's MSRV1.88 is not a workspace support claim. |

No new ARM Linux, musl, or Windows ARM support promise. Missing signing credentials are named Execute blockers, not unsigned success and not a silent support shrink.

Native targets: Windows x64 MSVC (Windows10/Server2016 ABI floor), macOS arm64/x64 (11.0 ABI floor), Linux x64 GNU (kernel4.18/glibc2.28). Use one existing sqlx0.9.0/libsqlite3-sys0.30.1 link. Electron44.3.0 embeds Node24.20.0/Chromium152.0.7977.78; packager20.3.0; TS ACP SDK1.4.0/zod4.6.2. Exact build candidates are Xcode16.4/deployment11.0, VS2022 17.14/v143/Windows SDK10.0.26100 and a glibc2.28 GNU sysroot/GCC10.1+.

**Desktop floor (resolved 2026-09-13):** Electron44 requires macOS13+ (Ventura); the current Tauri config does not state a `minimumSystemVersion`. The user accepted macOS 13+ on arm64 and x86_64 as the future Electron desktop target floor. That authorizes the target only: shipped Tauri support and sidecar behavior stay unchanged until a formal RFT-09 cutover, and a P3 go still requires actual signed dual-architecture execution plus the fixed resource/security gates. The accepted floor is not evidence and is not a shipped migration. Missing runners, SDKs or signing evidence block qualification, not permission to reduce cohorts.

## 12. Desktop: Electron preferred, Tauri shipped

Electron is the **preferred** future desktop host **subject to a real package / signing / security / resource go/no-go** (RFT-03), on the accepted macOS 13+ (Ventura) target floor for arm64 and x86_64 (user decision, 2026-09-13).

- Reuse `apps/web`, Design Studio, and `packages/nexus-ui`. No visual redesign and no second desktop UI.
- P3 is feasibility, not production distribution (RFT-10) and not Tauri retirement (RFT-11).
- **No-go does not complete M1.** It blocks RFT-M2 consumers that assumed Electron until an explicit alternative (for example Tauri + packaged Node, or typed IPC) is accepted. Product does not silently pick the alternative.
- Current Tauri sidecar/IPC/path-guard behavior stays until that later decision.

## 13. Frontend DX (hard product goal)

Stable-interface **browser / Studio / shared-UI** edits and **TS route/provider** edits with an unchanged native/schema contract MUST invoke **zero Cargo**.

This is early loop restoration, not a rewrite of Tauri/desktop packaging:

- Keep direct `dev:web` / `dev:design-studio`.
- Separate explicit backend refresh from ordinary frontend start.
- Reject incompatible schema/native artifacts instead of starting against them.
- Keep dist-load desktop (`dev:desktop`) distinct from HMR. Canonicalize the advertised vs actual HMR command (documented `dev:desktop:web` vs `pnpm --filter desktop dev`) without adding a new backend builder.
- Align daemon port selection (`NEXUS42_DAEMON_PORT`) with the Vite proxy (`VITE_DAEMON_URL`) so a non-default port is one endpoint, not two.

Do not credit already Cargo-free warm HMR as a new architecture gain. Cold and warm feedback are measured separately **after** a locked protocol; no historical speedup percentage is a product target.

## 14. M1 agreed flow, failures, and close

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
| Electron size/memory | Per architecture archive≤250MiB/installed≤600MiB; total-owned-process idle RSS≤500MiB, active p95≤750MiB; service idle≤180MiB; 100-cycle retained growth≤20MiB |
| Electron startup/maintenance | Cold process→interactive graph p95≤5s/max≤8s; warm p95≤3s/max≤5s; explicit patch rebuild≤30min per architecture on declared runner |

Measurement protocol: same baseline/candidate hardware and seeded real DB (500 entities,1000 stored relationships); 30 warm samples after3 warmups,10 cold process launches (not a cache-purge claim),10 fault/recovery cycles per case,10-minute soak and100 open/close cycles; raw monotonic timings/artifact hashes/OS/compiler/RSS/child ownership are retained. Quantiles use nearest-rank p95. RFT-00 calibration is Execute evidence, not deferred design. Failed target needs a recorded decision before a new run, never retroactive threshold relaxation. Package/signing/runtime proof has not yet run.

## 16. Non-goals (this target document)

- Claiming current daemon/Tauri/CLI HTTP clients are already the target architecture
- Full public API migration (RFT-M2) or production Electron release (RFT-M3)
- UI redesign, new provider capabilities, second engine, permanent old-server fallback
- Removing Works reads or any supported leaf by classification
- Uninterrupted native continuation across TS restart
- New ARM Linux / musl / Windows ARM / Windows-or-Linux GUI support
- Paid/live model requests; P5 public first-run
- Destructive data reset

## 17. Conflict with shipped Masters

Until a family migration is exercised:

1. Implement current behavior against the shipped Master for that domain.
2. Use this document for destination, host/lifetime, CLI disposition, and M1 vertical identity.
3. When a family lands on the target, fold the shipped Master section or record the deletion gate in the family plan. Do not leave two contradictory implementable topologies for the same family.

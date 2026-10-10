# Specs

Functional and normative specifications for the Nexus OSS repo.

**Rules (invariants):** [AGENTS.md](AGENTS.md)
**Not here:** schema-boundary policy → [knowledge root](../knowledge/README.md)

---

## Global narrative (first principles)

Nexus OSS specs describe a **local-first creative runtime** with optional cloud mount:

```text
Identity & scope          →  who owns data (Creator, User, World, …)
Architecture & contracts  →  which crate owns which concern; wire vs local types
Runtime topology          →  CLI (direct-core) → Rust core authority → ACP workers; standalone TS service for browser/Electron  (the `nexus42` daemon and embedded-SPA topology is retired history — v1.193 P2 delivered the cutover for every retained family; `nexus-runtime` is the independent Connect host and `/v1/daemon/*` the retained wire family — see rust-core-service-boundary.md §0)
Persistence               →  state.db, reference store, workspace layout
Orchestration             →  presets, capabilities, schedules, sessions
Product surface (CLI)     →  command IA, entry paths, per-flag behavior
Product lines             →  shipped journeys (Work, FL-E, agent tools, …)
Exploration               →  future engine/product lines without implement authority
```

**Why domain directories:** specs are grouped by the domain that owns them, so each domain dir holds a few long-lived **Master** documents agents can cite by stable basename; retired and historical records live under [archived/](archived/) away from live authority. Iteration velocity is handled by **Draft overlays**, not by renaming or sharding files.

**Why not one mega-spec:** CLI command detail, orchestration grammar, and ACP hosting evolve on different cadences; Feature line specs record shipped product contracts without bloating Masters.

**Discovery:** this README is the only maintained index. After adding or retiring a spec, update the tables below — do not duplicate the list in AGENTS.md.

**Three pillars (V1.122 canonized):** Nexus OSS specs describe a product built on three pillars — **Harness** (control strategy / orchestration / agent host / capability registry / presets; UI still reads "Strategy/Preset"), **Canvas** (spatial steering surface, with **Timeline-centric World building** as the hero World-entry surface), and **Computable** (the WASM layer that makes worlds react). Pillar definitions live in repo-root [`STRATEGY.md`](../../STRATEGY.md) + [`CONCEPTS.md`](../../CONCEPTS.md). Specs carry a `Pillar (V1.122)` header cross-reference where applicable (e.g. `orchestration-engine.md` → Harness; `compute-module-abi.md` + `wasm-host.md` → Computable; `canvas-strategy-surface.md` + `web-ui.md` → Ca…

---

## Document classes

| Class | Implement authority | Typical header `Status` |
| --- | --- | --- |
| Master | When normative / active | Normative, Active, Accepted |
| Draft overlay | While Status is Draft | Draft (Vx.xx), or Shipped α/β (Vx.xx) for overlay bodies carrying shipped slices |
| Feature line | Yes | Shipped (Vx.xx) |
| Exploration | No | Exploration |
| Companion | OSS scope only | Normative (companion) |
| Legacy scope | Cited subdomain only | Active (legacy scope) |

See [AGENTS.md](AGENTS.md) for create/extend/merge rules.

---

## Layout

Specs are organized into **domain subdirectories** (reorganized 2026-09-29 from the former flat layout — maintainer-authorized bulk `git mv` with a full link migration). The `specs/` root keeps only this README and [AGENTS.md](AGENTS.md).

| Directory | Scope |
| --- | --- |
| [architecture/](architecture/) | Entity scope, runtime/crate boundaries, spoke adapters, actor/holder governance, schemas layout |
| [runtime/](runtime/) | DB schema, concurrency, outbox, reference knowledge + store, embeddings, chapter-content and Daemon API conventions |
| [compute/](compute/) | Compute module ABI, WASM host |
| [cli/](cli/) | CLI product surface (`cli-spec`) |
| [orchestration/](orchestration/) | Orchestration engine, schedules/core context, preset routing, LLM extract |
| [agents/](agents/) | ACP client + capabilities, agent host, tool bridge, capability registry, registry integration |
| [creator/](creator/) | Creator product lines: work model, workflow, profiles, challenge solver, memory/SOUL lifecycle |
| [surfaces/](surfaces/) | Web UI, canvas strategy, design studio, desktop shell |
| [contracts/](contracts/) | Cross-cutting contracts: canonical hash, world delta, findings lifecycle |
| [novel-writing/](novel-writing/) | `work_profile: novel` subtree (unchanged; index: [novel-writing/README.md](novel-writing/README.md)) |
| [archived/](archived/) | Retired/deprecated/historical records only — cited for history, never implement authority |

| Index / rules | Purpose |
| --- | --- |
| [README.md](README.md) | Root domain index and authority matrix |
| [AGENTS.md](AGENTS.md) | Spec classes, lifecycle, and maintenance rules |

---

## Master index (by domain)

*Statuses reflect document headers as of last README maintenance; authoritative per-file header wins on conflict. `— (not declared)` means the header declares no document class; this index does not assign one.*

### Architecture

| Document | Class | Status |
| --- | --- | --- |
| [entity-scope-model.md](architecture/entity-scope-model.md) | Master | Normative — entity scope hierarchy, uniqueness, and domain ownership; V1.40 narrative taxonomy, V1.50 World KB promotion, V1.51 LLM pathway, and V1.62 computable validation. §1.4 owns the normative three-layer projection and V1.156 matrix completion; V1.159 era taxonomy and §6.6 fork lineage remain additive. Current KB validation owner: `nexus-knowledge`; service-family authority: `nexus-core`; former daemon ownership/branch-input wiring is historical. |
| [local-runtime-boundary.md](architecture/local-runtime-boundary.md) | Master | Normative for retained wire/ACP boundaries; daemon host-process sections are historical (retired in v1.193 P2) |
| [rust-core-service-boundary.md](architecture/rust-core-service-boundary.md) | Master | Accepted target — locked 2026-09-13; **M1 subset exercised** (v1.189 PR #306, merge `71e01cf9`); **v1.193 overlay delivered (P2 complete 2026-09-21)** — retained CLI leaves call core/cloud/Connect directly; browser/Electron use the standalone TS service, and the integrated daemon/embedded SPA are deleted. **RFT-05–07 route families implemented and exercised through the TS service + core (§7.5)**; product acceptance/QA and Run Studio completeness remain outstanding. v1.192 delivered Electron and unsigned packaging; GUI, Gatekeeper, installed-deployment, and first-run qualification remain separate, unverified obligations. §2 inventories `nexus-core`, `nexus-core-node`, `nexus-preset`, `nexus-provider-conformance`, `nexus-provider-ports`, and `nexus-storage-guard`. |
| [spoke-adapter-architecture.md](architecture/spoke-adapter-architecture.md) | Master | **Normative (v0.19 — V1.155 P1 capability-token production + tenant isolation: `nexus42 connect token issue` CLI (issuer.key Ed25519 create-once 0600, `claims.iss` MUST equal issuer-derived peer id), operator config `~/.nexus42/connect/config.json` (`trusted_issuers` / `require_capability_token` / `capability_token_provider{enabled, issuer_key_path}`, deny-unknown-fields, absent ⇒ pre-V1.155 defaults, malformed ⇒ fail-closed boot error, require-without-issuers ⇒ boot error); enforcement spoke-side fail-closed (`evaluate_invoke_token_gate` ⇒ `auth_failed` before the nexus handler, zero side effects) + nexus `PeerScope` intersection — token can never widen allowlist scope; all opt-in, … **v1.191 P1 holder amendment shipped** — [holder-governance.md](architecture/holder-governance.md) owns the holder/wire cutover; the pins are the pinned upstream lockstep release (version SSOT: the manifests; D17); **v1.198 Rule vocabulary implemented on the iteration branch (PR pending):** `archived` is terminal/read-only; `RuleQueryPort` retains raw reference resolution and the active-only World-scoped check. |
| [actor-product-model.md](architecture/actor-product-model.md) | Draft overlay | **Draft (2026-09-04 product lock; honesty amended 2026-09-06)** — **v1.184 shipped** Character bearer, bindings, three KE owner scopes, KnowledgeView, one-host execution, SOUL/Memory, ToM L1/L2 (PR #240); **v1.185 shipped** (§11 developer maintenance: identity edit, reversible archive/restore, WorldSheet binding maintenance, KE content maintenance, run-connected `--remember`; PR #241). §11 is the shipped contract, not a planning-only proposal; Schemas remain executable wire SSOT; **v1.191 holder amendment shipped on the `feat/v1.191-p1-spoke-0-13-adoption` plan branch (integration/PR delivery PM-owned)** — [holder-governance.md](architecture/holder-governance.md) |
| [holder-governance.md](architecture/holder-governance.md) | Draft overlay | **Shipped (v1.191 P1)** — implemented and exercised on the v1.191 plan branch (`feat/v1.191-p1-spoke-0-13-adoption`) and **merged to `main`** in the v1.191 iteration PR (`ee23ec47c`, #325); sole technical contract for Creator/Character holders, native disclosure, management vs ActorView admission, offline creator_only cutover, session invalidation, import identity safety, production extraction and Connect grants. The registry (`crates/nexus-local-db/src/holders.rs`, `knowledge_holders`) is shipped current state. The SPOKE pins are the pinned upstream lockstep release (version SSOT: the manifests; D17 records the adoption decision) |
| [schemas-directory-layout.md](architecture/schemas-directory-layout.md) | Master | Normative — current Daemon API contracts live under `schemas/daemon-api/`; generated authorities: Rust `generated::daemon_api` + TypeScript `generated/daemon-api` (reconciled through V1.183). V1.139 architect §5.2: `domain/key-block.schema.json` deleted (spoke `knowledge-entry.schema.json` is the KB type source) |
| [schemas-external-consumer-boundary.md](architecture/schemas-external-consumer-boundary.md) | Companion | Active — current external daemon contracts use the Daemon API namespace; V1.64 originally established the bundled Web UI as an external API consumer (moved from knowledge root 2026-08-17) |
| [world-kb-runtime-architecture.md](architecture/world-kb-runtime-architecture.md) | Master | Normative — World KB implementation SSOT: `nexus-core` service-family authorization/orchestration, `nexus-knowledge` KB domain/validation, `nexus-narrative` narrative aggregates, and `nexus-local-db` persistence. Former daemon HTTP adapters and transitional pack bridge are historical; current pack authority is `CoreService::import_world_pack`. |
| [connect-event-delivery-and-operation-receipts.md](architecture/connect-event-delivery-and-operation-receipts.md) | Master | **Shipped (v1.207, PR #367)** — Connect replay/negotiation wire + durable operation-receipt surface: WS-lane event delivery, gap/reconcile, and the receipt-first recover handshake. |

### Runtime

| Document | Class | Status |
| --- | --- | --- |
| [local-db-schema.md](runtime/local-db-schema.md) | Master | Normative — V1.40 Shipped §4.1.2 (KB validation + narrative_worlds + kb_extract_jobs artifact locator); **v1.191 P1 holder amendment shipped** (holder registry, native governance columns, knowledge revisions, schema 23 → 24) — [holder-governance.md](architecture/holder-governance.md) §§2–6 |
| [concurrency.md](runtime/concurrency.md) | Master | Normative — V1.51 advisory lock/heartbeat/OCC; V1.56 workspace sessions; V1.188 recoverable target-content commit (§9); **V1.201 P2** §6.2 restated as an implementable successful-acquire stale-holder detection decision table (diagnostics-only; semantics unchanged) |
| [outbox-consolidation.md](runtime/outbox-consolidation.md) | Master | Normative — V1.59 P-last promote (single-writer contract + schema ownership); **V1.177 revision** (daemon `outbox` table dropped at V1.163 — §2.3/§6 closed history) |
| [reference-knowledge.md](runtime/reference-knowledge.md) | Master | Normative — V1.58 P-last promote (reference body refreshable scan pipeline); current scheduler: `nexus-core` execution schedules; capability: `nexus-orchestration` builtins, with core registration/context wiring. V1.58 daemon boot composition is historical. |
| [reference-store-layout.md](runtime/reference-store-layout.md) | Master | Active — normative V1.26 design for local reference registry + body storage |
| [embedding-readiness.md](runtime/embedding-readiness.md) | Master | Normative — V1.181 P0 (RN-OGA-3 readiness-contract form): platform-provided embeddings, OSS ships no execution; `EmbeddingIdentity` tuple + fail-closed derived-index protocol + explicit lexical fallback; governs `crates/nexus-embedding/` |
| [chapter-content-local-api.md](runtime/chapter-content-local-api.md) | Feature line | Shipped — V1.65 chapter surface (`/v1/daemon/works/{work_id}/chapters/*`); V1.75 retired whole-document outline PUT in favor of the canvas patch route. Implementation owners: `nexus-core` content/outline services + `nexus-core-node`; route family served by `apps/nexus-service`. |
| [daemon-api-surface-conventions.md](runtime/daemon-api-surface-conventions.md) | Master | Normative — V1.77 amendment (§11 findings PATCH as non-OCC resource PATCH); retained Daemon API response/query conventions for `schemas/daemon-api/`, now served by `apps/nexus-service` over core (host boundary: [rust-core-service-boundary.md](architecture/rust-core-service-boundary.md)); `nexus-daemon-runtime` handler references are historical. **v1.198 §13 implemented on the iteration branch (PR pending):** archived is terminal; default omission precedes the 500-row cap; `include_archived` opts in. |

### Compute

| Document | Class | Status |
| --- | --- | --- |
| [compute-module-abi.md](compute/compute-module-abi.md) | Master | **Normative — V1.62 Shipped (P2)** — V1 envelope ABI: exports, host imports, marshalling, manifest.json contract |
| [wasm-host.md](compute/wasm-host.md) | Master | **Normative — V1.62 Shipped (P2)** — nexus-wasm-host crate: engine, sandbox, limits, watchdog, module loading, error taxonomy |

### CLI

| Document | Class | Status |
| --- | --- | --- |
| [cli-spec.md](cli/cli-spec.md) | Master | **Normative — V1.51 Shipped** — V1.40 §6.2G world binding + **V1.51** `kb adopt`/`rescan`/`pending --missing-only` (T-A P0/P1/P2); legacy V1.46 overlay fully merged; V1.52 §6.2G.1/§6.2G.2 overlays promoted (V1.158); **V1.175 P1** §6.2G.3–§6.2G.6 leaves **retargeted in v1.193 P2 to direct-core calls** (the thin daemon-HTTP transport is history); **V1.182 P1** §6.3B hidden `nexus42 ops inspect` (BL-04); **V1.193 P2 delivered (2026-09-21)** — the daemon group, `web-embed`, the `DaemonClient` leaves, `creator run`/`creator bootstrap`/runner entries, Model A `mcp serve`/`host-call` and the hidden top-level `sync` alias are deleted; the ordinary default is the direct-core `cli` cohort and `platform sync` is the canonical sync surface. Every daemon/runner instruction in the document is a historical record, not a setup step; **v1.198 implemented on the iteration branch (PR pending):** `creator world rule archive` and `rule list --include-archived`. |

**Read order:** CLI Master (§6–§7, including the delivered v1.193 P2 overlay) → historical V1.35 IA and entry-model supplements for rationale only.

### Orchestration

| Document | Class | Status |
| --- | --- | --- |
| [orchestration-engine.md](orchestration/orchestration-engine.md) | Master | Shipped (V1.4–V1.188) — preset loader, Host-mediated prompts, capability registry, recoverable workspace execution, and settle-once cancellation; V1.62 compute, V1.179 P2 bounded joins, and V1.186–V1.188 §15 execution completeness/reliability. Retained owners: `nexus-orchestration` + `nexus-preset`; daemon hosting and CLI runner entrances are historical after v1.193 P2. |
| [creator-schedule-and-core-context.md](orchestration/creator-schedule-and-core-context.md) | Master | Shipped (V1.4 WS7 → V1.34 agent-host + schedule wiring); canonical SSOT for ongoing schedule work |
| [preset-conditional-routing.md](orchestration/preset-conditional-routing.md) | Feature line | **Shipped (V1.42 P2)** — DF-56 `llm_judge` GO/NOGO minimal slice; V1.52/V1.56 overlays promoted (V1.158); **V1.179 P2** DR-06 bounded joins (§3.3.3, Normative) |
| [llm-extract.md](orchestration/llm-extract.md) | Master | **Normative — V1.51 Shipped (T-A P0)** — `nexus.llm.extract` capability, `LlmExtractTask`, `kb_extract_jobs` payload extension, and review-time extraction shipped; production preset-kind `llm_extract` routing remains unshipped/deferred |

### Agents

| Document | Class | Status |
| --- | --- | --- |
| [acp-client-tech-spec.md](agents/acp-client-tech-spec.md) | Master | **Shipped** — official `agent-client-protocol = "=2.2.0"` stable-v1 behind Nexus-owned DTOs; core-owned `HostManager` + admitted provider catalog, with native-core or TS ACP provider composition. Daemon route-facing manager/per-creator worker composition is historical. |
| [acp-capability-set.md](agents/acp-capability-set.md) | Master | Normative — logical capability catalog; current local HTTP host is `apps/nexus-service` under Electron lifecycle. Core `host_tool_registry()` owns host-tool dispatch; orchestration `CapabilityRegistry` is separate. |
| [agent-host.md](agents/agent-host.md) | Master | Normative — shipped through V1.188: Host-mediated orchestration, configured ACP/native registration, dsh SDK runtime/streaming cutover, and verified provider readiness |
| [agent-nexus-tool-bridge.md](agents/agent-nexus-tool-bridge.md) | Master | Normative — retained core tool-execution contract via `nexus-core` `host_tool_registry()`; **30 static IDs (28 `nexus.*` + 2 `fs/*`)**. Daemon `HostToolExecutor`, worker HTTP topology, and the six-tool roster are historical. |
| [capability-registry.md](agents/capability-registry.md) | Master | Normative / Shipped — promoted V1.57 P-last; retained runtime dispatch authority is `nexus-core` `host_tool_registry()` with **30 static IDs (28 `nexus.*` + 2 `fs/*`)**, plus admitted peer/user capabilities. Daemon wrappers and earlier rosters are historical. |
| [registry-integration.md](agents/registry-integration.md) | Master | Normative for identified shipped behavior — `nexus-acp-host` registry/client/cache, `HostManager` provider/session admission, and `packages/nexus-provider-acp` TS callbacks/subprocesses; Electron owns service lifecycle. Daemon supervision is historical; ACP discovery is distinct from `nexus.*` dispatch. |

### Creator

| Document | Class | Status |
| --- | --- | --- |
| [work-experience-model.md](creator/work-experience-model.md) | Feature line | Shipped (V1.33) — retained Work container; persistence: `nexus-local-db`; operations: `nexus-core` + CLI `creator works`. Historical `creator run` journeys are not current dispatch entrances. |
| [creator-workflow.md](creator/creator-workflow.md) | Feature line | Shipped (V1.34; V1.39 auto-chain; V1.40 extract binding; V1.79 SOUL visualization) — stage/checkpoint fields and the SOUL read contract remain; automatic full-stage/chapter progression and restart resume were retired with the incomplete runner in v1.193 P2-T1. Stored fields do not promise live dispatch. |
| **[novel-writing/](novel-writing/README.md)** | — (subtree index) | **`work_profile: novel`** — per-file index for the workflow profile, quality loop, author experience, audit, multi-work lifecycle, pools, and sync companion. Folded overlays are historical; cross-profile findings authority is [findings-lifecycle.md](contracts/findings-lifecycle.md), and the sync companion is a shipped chapter-discovery/bundle library, not cloud upload integration. |
| [essay-profile.md](creator/essay-profile.md) | Feature line | Shipped (V1.63) — `work_profile: essay` production-ready scaffold, seven-state preset, quality rubric, completion detection, and optional KB extraction |
| [game-bible-profile.md](creator/game-bible-profile.md) | Feature line | Master (V1.56 P-last) — section-status auto-transition closure; V1.55 P2 shipped design-writing preset, quality rubric, section completion, and KB extraction |
| [script-profile.md](creator/script-profile.md) | Feature line | Master (V1.60 P-last promotion from Draft V1.60 P1) — `work_profile: script` artifact layout, stage/preset chain, quality rubric, KB taxonomy, and completion semantics |
| [creator-challenge-solver.md](creator/creator-challenge-solver.md) | Master | Frozen (normative for shipped CLI registration path) — solver orchestration: `apps/nexus42/src/challenge/`; registration caller: CLI Creator commands; `nexus-creator` owns aggregate/local identity, not the solver |
| [creator-memory-soul-lifecycle.md](creator/creator-memory-soul-lifecycle.md) | Draft overlay | Draft (V1.82 amendment) — per-(creator, world) narrative lifecycle |

### Surfaces

| Document | Class | Status |
| --- | --- | --- |
| [web-ui.md](surfaces/web-ui.md) | Feature line | **Shipped (V1.65)** — standalone `apps/web` React/Vite SPA + `apps/nexus-service` TS service under Electron lifecycle ownership (v1.193 P2 cutover; Tauri retired in v1.192). Historical stages: Control Room + Setup (V1.64) + Content-Authoring UI stage (V1.65 §13) + Desktop Shell stage (V1.66 §14, Shipped) + Surface Convergence & De-risk stage (V1.67 §15, Shipped) + V1.69 Design System Maturation & Canvas Draft + **V1.70 Canvas Strategy Implement (α) stage (V1.70 §16, Shipped)** + CI/desktop-build optimization (parallel ops track); stages through V1.78; **V1.94/V1.98/V1.118/V1.125/V1.122 Draft amendments** (§29–§30 IA, Design Studio, creation peer groups, Three-pillar pivot + Timeline-first Canvas IA); **V1.147** Computable Run Studio; **V1.156 PD-4** §29.4 Harness pillar-entry rename; **V1.157** React 19; **V1.170 P1** Entrance-first setup (AR-17). |
| [web-ui-design-requirements.md](surfaces/web-ui-design-requirements.md) | Companion | Input brief (V1.64/V1.65 Prepare Phase 2b) for repo-root `DESIGN.md` — product/design intent, not token authority; the root DESIGN pair is the sole SSOT since `apps/web/DESIGN*.md` retired (V1.98) |
| [canvas-strategy-surface.md](surfaces/canvas-strategy-surface.md) | Draft overlay | **Shipped β (V1.74)** — Strategy α (V1.70) + Strategy write-boundary (V1.71) + Outline+Timeline β (V1.72) + World KB β (V1.73) + World KB relationships β (V1.74) shipped; **V1.122/V1.123 Draft overlays** (Timeline peer surface = default World entry; three-layer Brief/Narrative/Moment + Work Timeline) + **V1.156** 3×2 matrix completion + **V1.159** era taxonomy + **V1.162** fork authoring chrome, and **V1.163** event-level cross-surface binding — each additive and frontend-only (`wire_contracts_changed: false`); see the promotion blockquote chain in the doc |
| [design-studio.md](surfaces/design-studio.md) | Feature line | Normative target contract (v1.187) — read-only contributor gallery and proving ground; frozen IA; filterable index + pair view; DESIGN v0.5 token values locked; implementation and visual acceptance are separate; not implement-GO; not author-facing product UI |
| [desktop-shell.md](surfaces/desktop-shell.md) | Master | **Electron is the shipped desktop host (v1.192)** — cutover accepted, unsigned app+DMG delivered for both macOS architectures, Tauri composition retired; in-place product/host contract preserves setup/no-profile boot, guarded native actions, local-state recovery, HTTP client and three-option quit and defines secure preload/utility ownership. Dual-architecture GUI qualification remains unverified. |

### Contracts

| Document | Class | Status |
| --- | --- | --- |
| [canonical-hash.md](contracts/canonical-hash.md) | Companion | Normative (OSS notes; platform ADR-006 authoritative) |
| [world-delta-propose-apply.md](contracts/world-delta-propose-apply.md) | Feature line (promoted from Draft overlay V1.60 P0) | Master (V1.60 P-last promotion) — world-delta propose/apply local parity |
| [findings-lifecycle.md](contracts/findings-lifecycle.md) | Master | Normative — V1.77 Phase 2b promotion (cross-profile 6-state findings lifecycle + `target_executor` routing + UI remediation surface); produce side owned by quality-loop §2 |

*Novel-writing sync module contract: [novel-writing/sync-contract.md](novel-writing/sync-contract.md).*

### Archived records (specs/archived/)

| Document | Class | Status |
| --- | --- | --- |
| [daemon-runtime.md](archived/daemon-runtime.md) | Master | **Retired host record (v1.193 P2)** — the integrated daemon host is deleted: the `nexus-daemon-runtime` crate, the whole `nexus42 daemon` group, hidden `daemon-run` and the `web-embed` embedded-SPA bytes are gone, with no replacement service launcher. Current owners of the retained contracts: the standalone TS service serves `/v1/daemon/*` for browser/Electron, Electron owns the desktop and its service lifecycle, and `nexus-runtime` is the independent Connect host (§4.6). Sections are the historical record unless they name a retained wire, Connect or checkpoint contract. Historical amendments: V1.65 Prepare (bundled local Web UI serving + chapter-content route family); **V1.66** §12 Tauri sidecar mode; **V1.86** §13 Daemon API trust boundary; **V1.90** §14 remote bind gate + surface rename Local API → **Daemon API** with `/v1/daemon/` prefix; **V1.92** §15–16 TLS transport + remote client; **V1.118** §17 no-Profile boot + lazy `state.db` open; **V1.153** §4.6 headless `nexus-runtime` profile; **V1.180–V1.182** §19 checkpoint inspection + boot re-drive semantics (reconciled through V1.183); **V1.186 Shipped** §20 truthful terminals/waits + bounded run recovery (§20.1–§20.2 V1.188 readiness/replay); **v1.192** §12 Tauri desktop host retired (Electron) |
| [local-cloud-crate-architecture.md](archived/local-cloud-crate-architecture.md) | Master | **Historical crate-graph record (v1.193 P2)** — the integrated daemon host and `nexus-daemon-runtime` crate were deleted; current topology and the six newer crate boundaries are owned by [rust-core-service-boundary.md](architecture/rust-core-service-boundary.md) |
| [creator-run-preset-entry.md](archived/creator-run-preset-entry.md) | Master (historical V1.45 CLI IA; no current dispatch authority) | **Retired runner record (v1.193 P2-T1)** — originally Shipped V1.45; no replacement CLI preset-dispatch entrance. Current atomic Work operations are not a generic runner. |
| [cli-command-ia.md](archived/cli-command-ia.md) | Master (historical V1.35 lock; retained rationale supplement) | Shipped (V1.35) — historical IA rationale; current command authority is `cli-spec` §6.0B + its delivered v1.193 P2 overlay |
| [creator-centric-entry-model.md](archived/creator-centric-entry-model.md) | Master (historical V1.35 lock; retained entry-model supplement) | Shipped (V1.35) — historical onboarding/entry rationale; current entry authority is `cli-spec` §6.0B + its delivered v1.193 P2 overlay |
| [reading-chrome-profile-checklist.md](archived/reading-chrome-profile-checklist.md) | Legacy scope | Historical shipped acceptance record (V1.91) — behavioral bar stands; named visual values are superseded by the active DESIGN pair `components.reading-chrome-*` tokens |
| [local-api-surface-conventions.md](archived/local-api-surface-conventions.md) | Redirect stub | **V1.90 redirect stub** — renamed to [daemon-api-surface-conventions.md](runtime/daemon-api-surface-conventions.md); retained for historical links from iteration compasses/plans |

---

## Normative hierarchy (conflict resolution)

When specs disagree, higher row wins:

1. Repo root **AGENTS.md**
2. Current architecture authorities ([rust-core-service-boundary.md](architecture/rust-core-service-boundary.md), [entity-scope-model.md](architecture/entity-scope-model.md)); historical crate/host records do not override retained owners
3. **Draft overlay** over a conflicting legacy Master section until merge
4. Domain **Master**
5. Shipped supplement / retained overlay for rationale and acceptance details after Master merge
6. **Feature line** spec
7. **Exploration** (non-binding)

---

## Authority matrix (overlapping topics)

| Topic | Primary SSOT | Secondary |
| --- | --- | --- |
| Top-level CLI groups | cli-spec §6.0B + delivered v1.193 P2 overlay | cli-command-ia (historical V1.35 rationale only) |
| First-run / local vs platform | cli-spec §6.0B + delivered v1.193 P2 overlay, §7 | creator-centric-entry-model (historical V1.35 entry rationale only) |
| Work / retained atomic CLI operations | [work-experience-model.md](creator/work-experience-model.md), cli-spec §6.2H | creator-workflow §0; [creator-run-preset-entry.md](archived/creator-run-preset-entry.md) is the retired runner record, not dispatch authority |
| Novel profile / `Works/<work_ref>/` layout | [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) | work-experience-model, [novel-writing/sync-contract.md](novel-writing/sync-contract.md), cli-spec §12.1 |
| Creator workflow stage/checkpoint fields | creator-workflow §0 | work-experience-model; automatic full-stage progression/restart continuation is retired, while orchestration-engine §15 owns retained run execution semantics |
| Preset YAML / loader / validator | orchestration-engine | creator-schedule § YAML additions |
| Schedule / core_context | creator-schedule-and-core-context | orchestration-engine sessions |
| On-demand chapter audit (DF-69) | [novel-writing/manuscript-audit.md](novel-writing/manuscript-audit.md) | novel-writing/quality-loop §3, cli-spec §6.2 |
| Agent `nexus.*` tools | [capability-registry.md](agents/capability-registry.md), agent-nexus-tool-bridge §0 | acp-capability-set (logical catalog), agent-host; dispatch owner is core `host_tool_registry()`, not ACP discovery |
| ACP provider/session lifecycle | acp-client-tech-spec shipped boundary, registry-integration | agent-host, rust-core-service-boundary, local-runtime-boundary (retained ACP boundaries only); daemon worker supervision is historical |
| KB naming (KCA-003) | entity-scope-model §5.4 + cli-spec §6.2E–F | cli-command-ia §3.2 (historical V1.35 rationale) |
| LLM extraction capability | [llm-extract.md](orchestration/llm-extract.md) | entity-scope-model §5.5.6, world-kb-runtime-architecture §5.5, cli-spec §6.2G |
| Actor/Creator/Character identity, ActorWorldBinding, WorldSheet distinction, KnowledgeEntry owner scopes, Viewpoint | [actor-product-model.md](architecture/actor-product-model.md) | entity-scope-model (shipped KE taxonomy + scope hierarchy), world-kb-runtime-architecture, agent-host, acp-client-tech-spec |
| Compute module ABI (V1 envelope) | [compute-module-abi.md](compute/compute-module-abi.md) | wasm-host, schemas-directory-layout §3.5, orchestration-engine §8.4, entity-scope-model §5.5.9, `schemas/daemon-api/compute/` |
| WASM compute host runtime | [wasm-host.md](compute/wasm-host.md) | compute-module-abi, orchestration-engine §8.4, `crates/nexus-wasm-host/AGENTS.md` |
| Orchestration checkpoint inspection / bounded recovery | cli-spec §6.3B (`nexus42 ops inspect`), [orchestration-engine.md](orchestration/orchestration-engine.md) §15 | preset-conditional-routing §3.3.3, concurrency §9; inspection is read-only, not a CLI resume entrance, and former daemon-boot re-drive is retired |
| Rust core vs TS service vs CLI/runtime/desktop hosts | [rust-core-service-boundary.md](architecture/rust-core-service-boundary.md) (Accepted target; M1 exercised; **v1.193 overlay delivered; RFT-05–07 route families implemented/exercised**) | local-runtime-boundary (retained wire/ACP scope), cli-spec, desktop-shell, web-ui, agent-host, concurrency — current retained owners; daemon-runtime and local-cloud-crate-architecture are historical host/topology records, not implementable owners. Route exercise does not close outstanding product/GUI/distribution/first-run qualification. |

---

## Hygiene schedule (consolidation policy)

| Trigger | Required action | Status |
| --- | --- | --- |
| **Post-V1.35 CLI changes** | Update cli-spec §6–§7 first; update shipped supplements only when rationale, acceptance, or migration history changes | V1.36-V1.40 amendments folded into Master (no follow-up merge needed yet) |
| **V1.53 ACP capability registry hygiene** | Promote or retain `capability-registry.md` after P0/P1 registry semantics land; skills-export compatibility spec retired and DF-50 Cancelled | **Done 2026-06-22** — promoted to Master at V1.57 P-last (see header + this index) |
| **Novel-writing sync module removed from code** | Retire the sync companion only when its library contract is removed | Retained `nexus-orchestration::sync_module` discovery/bundle library (V1.36 Works layout); not an integrated cloud upload path |
| **V1.40 shipped (DF-63 closed)** | Mark `entity-scope-model.md` §5.1.1 + `cli-spec.md` §6.2G + `creator-workflow.md` persist + `local-db-schema.md` §4.1.2 + `novel-writing/workflow-profile.md` §3.5.1 as Shipped V1.40 in their headers | **Done 2026-06-11** (see headers + this index) |

**Retained splits (do not merge):** creator-schedule-and-core-context (schedule domain); ACP cluster (independent evolution cadence).

---

## Platform cross-repo references

Cite **`nexus-platform`** `v1-spec/` for cloud product, shared ADRs, and architecture umbrella. Wire JSON in this repo: `schemas/` → `nexus-contracts`.

| Need | Platform path |
| --- | --- |
| Architecture umbrella | `v1-spec/architecture.md` |
| ADR | `v1-spec/adr/{name}.md` |
| Shared contracts | `v1-spec/shared/...` |
| Platform HTTP / product | `v1-spec/platform/...` |

---

## Archived superseded specs

| Former spec | Superseded by |
| --- | --- |
| `daemon-api-workspace-write-architecture.md` | Stale — historical |
| `local-fs-layout-creator-workspace.md` | Retired |
| `nexus42-single-binary-daemon-runtime-architecture.md` | [daemon-runtime.md](archived/daemon-runtime.md) |
| `agent-host-architecture.md` | [agent-host.md](agents/agent-host.md) §8 |
| `fl-d-conditional-routing-exploration-v1.35-prepare.md` | [preset-conditional-routing.md](orchestration/preset-conditional-routing.md) |
| `novel-findings-maturity.md` | [novel-writing/quality-loop.md](novel-writing/quality-loop.md) §9 |
| `body-editor.md` | [canvas-strategy-surface.md](surfaces/canvas-strategy-surface.md) (2026-06-26 — body-editor direction rejected) |
| `non-novel-profiles-roadmap.md` | [game-bible-profile.md](creator/game-bible-profile.md) + [script-profile.md](creator/script-profile.md) + [essay-profile.md](creator/essay-profile.md) (all targets shipped) |
| `novel-writing/findings-lifecycle.md` (V1.49 overlay) | [novel-writing/quality-loop.md](novel-writing/quality-loop.md) §2 — retired; current cross-profile Master: [findings-lifecycle.md](contracts/findings-lifecycle.md) |
| `narrative-indexes.md` | [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) §4.6 |

**Former filename:** `local-platform-isolation-and-crate-architecture.md` → `local-cloud-crate-architecture.md` (2026-05-20).

---

## Maintaining this index

When adding, renaming, or archiving a spec:

1. Set header **`Status`**, **`Document class`**, and **`Coordinates with`** in the spec file.
2. Update the domain table in this README.
3. Update this README index when specs are added, retired, or promoted.
4. Do **not** add file lists to AGENTS.md.

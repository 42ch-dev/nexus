# Specs (`knowledge/specs/`)

Functional and normative specifications for the Nexus OSS repo.

**Rules (invariants):** [AGENTS.md](AGENTS.md)  
**Machine state:** [`.mstar/status.json`](../../status.json) → `metadata.latest_ship` / `metadata.latest_active_compass`; wave-0 spec per active compass §Normative specs; full index is this README  
**Not here:** trackers and schema-boundary policy → [knowledge root](../README.md); delivery evidence → [iterations](../../iterations/README.md)

---

## Global narrative (first principles)

Nexus OSS specs describe a **local-first creative runtime** with optional cloud mount:

```text
Identity & scope          →  who owns data (Creator, User, World, …)
Architecture & contracts  →  which crate owns which concern; wire vs local types
Runtime topology          →  CLI → daemon → Local API → ACP workers
Persistence               →  state.db, reference store, workspace layout
Orchestration             →  presets, capabilities, schedules, sessions
Product surface (CLI)     →  command IA, entry paths, per-flag behavior
Product lines             →  shipped journeys (Work, FL-E, agent tools, …)
Exploration               →  future engine/product lines without implement authority
```

**Why flat files:** each layer exposes a few long-lived **Master** documents agents can cite by stable basename. Iteration velocity is handled by **Draft overlays** and **compass appendices**, not by renaming or sharding directories.

**Why not one mega-spec:** CLI command detail, orchestration grammar, and ACP hosting evolve on different cadences; Feature line specs record shipped product contracts without bloating Masters.

**Discovery:** this README is the only maintained index. After adding or retiring a spec, update the tables below — do not duplicate the list in AGENTS.md.

---

## Document classes

| Class | Implement authority | Typical header `Status` |
| --- | --- | --- |
| Master | When normative / active | Normative, Active, Accepted |
| Draft overlay | While active compass + Draft | Draft (Vx.xx) |
| Feature line | Yes | Shipped (Vx.xx) |
| Exploration | No | Exploration |
| Companion | OSS scope only | Normative (companion) |
| Legacy scope | Cited subdomain only | Active (legacy scope) |

See [AGENTS.md](AGENTS.md) for create/extend/merge rules.

---

## Layout

Spec files live **flat** in this directory except **`novel-writing/`** — the novel `work_profile` subtree (relocated 2026-06-17). See [novel-writing/README.md](novel-writing/README.md) for the domain index.

---

## Master index (by domain)

*Statuses reflect document headers as of last README maintenance; authoritative per-file header wins on conflict.*

### Architecture and boundaries

| Document | Class | Status |
| --- | --- | --- |
| [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) | Master | Active |
| [entity-scope-model.md](entity-scope-model.md) | Master | Normative — V1.40 Shipped §5.1.1 (taxonomy + `canonical_name` grammar); **V1.51 Shipped** §5.5.6 LLM pathway |
| [local-runtime-boundary.md](local-runtime-boundary.md) | Master | Normative |
| [schemas-directory-layout.md](schemas-directory-layout.md) | Master | Normative |

Also: [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md) (knowledge root).

### Runtime and persistence

| Document | Class | Status |
| --- | --- | --- |
| [daemon-runtime.md](daemon-runtime.md) | Master | Normative |
| [local-db-schema.md](local-db-schema.md) | Master | Normative — V1.40 Shipped §4.1.2 (KB validation + narrative_worlds + kb_extract_jobs artifact locator) |
| [concurrency.md](concurrency.md) | Master | **Normative — V1.51 Shipped (T-B P0/P1)** — advisory lock + heartbeat + OCC + zombie detection |
| [reference-store-layout.md](reference-store-layout.md) | Master | Normative |

### CLI product surface

| Document | Class | Status |
| --- | --- | --- |
| [cli-spec.md](cli-spec.md) | Master | **Normative — V1.51 Shipped** — V1.40 §6.2G world binding + **V1.51** `kb adopt`/`rescan`/`pending --missing-only` (T-A P0/P1/P2); legacy V1.46 overlay fully merged |
| [cli-command-ia.md](cli-command-ia.md) | Master (Shipped V1.35) | Shipped (V1.35) |
| [creator-centric-entry-model.md](creator-centric-entry-model.md) | Master (Shipped V1.35) | Shipped (V1.35) |

**Read order:** CLI Master (§6–§7) → shipped IA supplement → shipped entry-model supplement.

### Orchestration and presets

| Document | Class | Status |
| --- | --- | --- |
| [orchestration-engine.md](orchestration-engine.md) | Master | Active |
| [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) | Legacy scope | Active (WS7 schedule/core_context) |
| [preset-conditional-routing.md](preset-conditional-routing.md) | Feature line | **Shipped (V1.42 P2)** — DF-56 `llm_judge` GO/NOGO minimal slice |
| [llm-extract.md](llm-extract.md) | Master | **Normative — V1.51 Shipped (T-A P0)** — `nexus.llm.extract` capability + `LlmExtractTask` + `kb_extract_jobs` LLM payload extension (closes R-V150KBED-01) |

### Creator product lines

| Document | Class | Status |
| --- | --- | --- |
| [work-experience-model.md](work-experience-model.md) | Feature line | Shipped (V1.33) |
| [creator-workflow.md](creator-workflow.md) | Feature line | Shipped (V1.34; V1.40 Shipped — DF-63 W5 `novel-review-master sync_world_kb` extract binding) |
| **[novel-writing/](novel-writing/README.md)** | Feature subtree | **`work_profile: novel`** — see [novel-writing/README.md](novel-writing/README.md) for per-file index (workflow-profile, quality-loop, author-experience, overlays, …) |
| [essay-profile.md](essay-profile.md) | Feature line | Draft (V1.52) — `work_profile: essay` first non-novel profile |
| [creator-run-preset-entry.md](creator-run-preset-entry.md) | Master | **Shipped (V1.45)** — `creator run <preset_id>` generic entry; wave 0 for V1.45 CLI IA (promoted P-last) |
| [creator-challenge-solver.md](creator-challenge-solver.md) | Master | Normative |

### ACP and agent integration

| Document | Class | Status |
| --- | --- | --- |
| [acp-client-tech-spec.md](acp-client-tech-spec.md) | Master | Accepted |
| [acp-capability-set.md](acp-capability-set.md) | Master | Normative |
| [agent-host.md](agent-host.md) | Master | Normative |
| [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) | Feature line | Shipped (V1.34) |
| [capability-registry.md](capability-registry.md) | Draft overlay | Draft (V1.53) — runtime SSOT framework for `nexus.*` dispatch |
| [registry-integration.md](registry-integration.md) | Master | Normative |

### Feature contracts and companions

| Document | Class | Status |
| --- | --- | --- |
| [canonical-hash.md](canonical-hash.md) | Companion | Normative (OSS notes; platform ADR-006 authoritative) |
| [non-novel-profiles-roadmap.md](non-novel-profiles-roadmap.md) | Exploration | V1.52 roadmap for game-bible + script profiles (V1.53+) |

*Novel-writing sync module contract: [novel-writing/sync-contract.md](novel-writing/sync-contract.md).*

---

## Normative hierarchy (conflict resolution)

When specs disagree, higher row wins:

1. Repo root **AGENTS.md**
2. Architecture Masters (crate graph, entity scope)
3. Active **iteration compass** (delivery batching only)
4. Domain **Master**
5. Shipped supplement / retained overlay for rationale and acceptance details after Master merge
6. **Feature line** spec
7. **Exploration** (non-binding)

---

## Authority matrix (overlapping topics)

| Topic | Primary SSOT | Secondary |
| --- | --- | --- |
| Top-level CLI groups | cli-spec §6.0B | cli-command-ia (Shipped V1.35 supplement) |
| First-run / local vs platform | cli-spec §7 | creator-centric-entry-model (Shipped V1.35 supplement), compass audit appendix |
| Work / `creator run` | [creator-run-preset-entry.md](creator-run-preset-entry.md) (V1.45 Draft) | work-experience-model, cli-spec §6.2 |
| Novel profile / `Works/<work_ref>/` layout | [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) | work-experience-model, [novel-writing/sync-contract.md](novel-writing/sync-contract.md), cli-spec §12.1 |
| Creator workflow stages / chain | creator-workflow | work-experience-model, novel-writing/workflow-profile (produce) |
| Preset YAML / loader / validator | orchestration-engine | creator-schedule § YAML additions |
| Schedule / core_context | creator-schedule-and-core-context | orchestration-engine sessions |
| On-demand chapter audit (DF-69) | [novel-writing/manuscript-audit.md](novel-writing/manuscript-audit.md) | novel-writing/quality-loop §3, cli-spec §6.2 |
| Agent `nexus.*` tools | agent-nexus-tool-bridge | acp-capability-set, agent-host |
| ACP worker process | acp-client-tech-spec | daemon-runtime, local-runtime-boundary |
| KB naming (KCA-003) | entity-scope-model §5.4 + cli-command-ia §3.2 | cli-spec §6.2E–F |
| LLM extraction capability | [llm-extract.md](llm-extract.md) | entity-scope-model §5.5.6, world-kb-runtime-architecture §5.5, cli-spec §6.2G |

---

## Hygiene schedule (consolidation policy)

| Trigger | Required action | Status |
| --- | --- | --- |
| **Post-V1.35 CLI changes** | Update cli-spec §6–§7 first; update shipped supplements only when rationale, acceptance, or migration history changes | V1.36-V1.40 amendments folded into Master (no follow-up merge needed yet) |
| **FL-D compass locks implement** | Promote preset-conditional-routing; update orchestration-engine §7.5 | Deferred (FL-D still out of scope) |
| **V1.53 ACP capability registry hygiene** | Promote or retain `capability-registry.md` after P0/P1 registry semantics land; skills-export compatibility spec retired and DF-50 Cancelled | Active V1.53 |
| **Novel-writing sync module removed from code** | Archive novel-writing-sync-contract | Module still shipped (V1.36+); sync contract retained |
| **V1.40 shipped (DF-63 closed)** | Mark `entity-scope-model.md` §5.1.1 + `cli-spec.md` §6.2G + `creator-workflow.md` persist + `local-db-schema.md` §4.1.2 + `novel-writing/workflow-profile.md` §3.5.1 as Shipped V1.40 in their headers | **Done 2026-06-11** (see headers + this index) |
| **V1.41 prep** | Decide which V1.40-tagged open residuals (`status.json.residual_findings`) to address in V1.41 hygiene; re-evaluate DF-60/61/56/47/59 targets | Pending V1.41 compass |

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
| [daemon-api-workspace-write-architecture.md](../../archived/knowledge/daemon-api-workspace-write-architecture.md) | Stale — historical |
| [local-fs-layout-creator-workspace.md](../../archived/knowledge/local-fs-layout-creator-workspace.md) | Pointer stub |
| `nexus42-single-binary-daemon-runtime-architecture.md` | [daemon-runtime.md](daemon-runtime.md) |
| `agent-host-architecture.md` | [agent-host.md](agent-host.md) §8 |
| [fl-d-conditional-routing-exploration-v1.35-prepare.md](../../archived/knowledge/fl-d-conditional-routing-exploration-v1.35-prepare.md) | [preset-conditional-routing.md](preset-conditional-routing.md) |
| [archived/knowledge/novel-findings-maturity.md](../../archived/knowledge/novel-findings-maturity.md) | [novel-writing/quality-loop.md](novel-writing/quality-loop.md) §9 |

**Former filename:** `local-platform-isolation-and-crate-architecture.md` → `local-cloud-crate-architecture.md` (2026-05-20).

---

## Maintaining this index

When adding, renaming, or archiving a spec:

1. Set header **`Status`**, **`Document class`**, and **`Coordinates with`** in the spec file.
2. Update the domain table in this README.
3. Update this README index when specs are added, retired, or promoted; on Prepare, record wave-0 in the iteration compass (not `status.json`).
4. Do **not** add file lists to AGENTS.md.

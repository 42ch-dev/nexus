# Specs (`knowledge/specs/`)

Functional and normative specifications for the Nexus OSS repo.

**Rules (invariants):** [AGENTS.md](AGENTS.md)  
**Machine state (wave-0, spec_refs):** [`.mstar/status.json`](../../status.json)  
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

All spec files live **flat** in this directory (kebab-case, no version suffix). Subdirectories are intentionally unused.

---

## Master index (by domain)

*Statuses reflect document headers as of last README maintenance; authoritative per-file header wins on conflict.*

### Architecture and boundaries

| Document | Class | Status |
| --- | --- | --- |
| [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) | Master | Active |
| [entity-scope-model.md](entity-scope-model.md) | Master | Normative — V1.40 Shipped §5.1.1 (taxonomy + canonical_name grammar) |
| [local-runtime-boundary.md](local-runtime-boundary.md) | Master | Normative |
| [schemas-directory-layout.md](schemas-directory-layout.md) | Master | Normative |

Also: [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md) (knowledge root).

### Runtime and persistence

| Document | Class | Status |
| --- | --- | --- |
| [daemon-runtime.md](daemon-runtime.md) | Master | Normative |
| [local-db-schema.md](local-db-schema.md) | Master | Normative — V1.40 Shipped §4.1.2 (KB validation + narrative_worlds + kb_extract_jobs artifact locator) |
| [reference-store-layout.md](reference-store-layout.md) | Master | Normative |

### CLI product surface

| Document | Class | Status |
| --- | --- | --- |
| [cli-spec.md](cli-spec.md) | Master | Normative — V1.40 Shipped §6.2G (creator world mandatory binding) + §creator kb queue-extract --chapter N |
| [cli-command-ia.md](cli-command-ia.md) | Master (Shipped V1.35) | Shipped (V1.35) |
| [creator-centric-entry-model.md](creator-centric-entry-model.md) | Master (Shipped V1.35) | Shipped (V1.35) |

**Read order:** CLI Master (§6–§7) → shipped IA supplement → shipped entry-model supplement.

### Orchestration and presets

| Document | Class | Status |
| --- | --- | --- |
| [orchestration-engine.md](orchestration-engine.md) | Master | Active |
| [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) | Legacy scope | Active (WS7 schedule/core_context) |
| [preset-conditional-routing.md](preset-conditional-routing.md) | Draft overlay | **Draft (V1.42 P2)** — DF-56 minimal `llm_judge` slice |

### Creator product lines

| Document | Class | Status |
| --- | --- | --- |
| [work-experience-model.md](work-experience-model.md) | Feature line | Shipped (V1.33) |
| [creator-workflow.md](creator-workflow.md) | Feature line | Shipped (V1.34; V1.40 Shipped — DF-63 W5 `novel-review-master sync_world_kb` extract binding) |
| [novel-workflow-profile.md](novel-workflow-profile.md) | Draft overlay | **Shipped (V1.36 → V1.40)** — World KB §3.5.1; V1.41 §6.4 multi-work cross-refs |
| [novel-multi-work-lifecycle.md](novel-multi-work-lifecycle.md) | Feature line | **Shipped (V1.41 → V1.42 P0)** — §4.2 runtime_lock production wiring implemented V1.42 P0 |
| [novel-work-pool.md](novel-work-pool.md) | Feature line | **Shipped (V1.41)** — `set_pool_active` authz |
| [novel-quality-loop.md](novel-quality-loop.md) | Draft overlay | **Shipped (V1.39 → V1.40)** — full quality loop + V1.40 P3 extract persistence |
| [novel-author-experience.md](novel-author-experience.md) | Draft overlay | **Draft (V1.43)** — BL-10 quickstart contract + CLI copy map + author visibility |
| [creator-challenge-solver.md](creator-challenge-solver.md) | Master | Normative |

### ACP and agent integration

| Document | Class | Status |
| --- | --- | --- |
| [acp-client-tech-spec.md](acp-client-tech-spec.md) | Master | Accepted |
| [acp-capability-set.md](acp-capability-set.md) | Master | Normative |
| [agent-host.md](agent-host.md) | Master | Normative |
| [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) | Feature line | Shipped (V1.34) |
| [registry-integration.md](registry-integration.md) | Master | Normative |
| [skills-export-compatibility.md](skills-export-compatibility.md) | Master | Normative |

### Feature contracts and companions

| Document | Class | Status |
| --- | --- | --- |
| [novel-writing-sync-contract.md](novel-writing-sync-contract.md) | Companion | Normative (module contract) |
| [canonical-hash.md](canonical-hash.md) | Companion | Normative (OSS notes; platform ADR-006 authoritative) |

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
| Work / `creator run` | work-experience-model | cli-spec §6.2, orchestration run_intents |
| Novel profile / `Works/<work_ref>/` layout | novel-workflow-profile | work-experience-model, novel-writing-sync-contract, cli-spec §13.1 |
| Creator workflow stages / chain | creator-workflow | work-experience-model, novel-workflow-profile (produce) |
| Preset YAML / loader / validator | orchestration-engine | creator-schedule § YAML additions |
| Schedule / core_context | creator-schedule-and-core-context | orchestration-engine sessions |
| Conditional routing | preset-conditional-routing | orchestration-engine §7.5 stub |
| Agent `nexus.*` tools | agent-nexus-tool-bridge | acp-capability-set, agent-host |
| ACP worker process | acp-client-tech-spec | daemon-runtime, local-runtime-boundary |
| KB naming (KCA-003) | entity-scope-model §5.4 + cli-command-ia §3.2 | cli-spec §6.2E–F |

---

## Hygiene schedule (consolidation policy)

| Trigger | Required action | Status |
| --- | --- | --- |
| **Post-V1.35 CLI changes** | Update cli-spec §6–§7 first; update shipped supplements only when rationale, acceptance, or migration history changes | V1.36-V1.40 amendments folded into Master (no follow-up merge needed yet) |
| **FL-D compass locks implement** | Promote preset-conditional-routing; update orchestration-engine §7.5 | Deferred (FL-D still out of scope) |
| **ACP spec hygiene plan** | Evaluate merging skills-export-compatibility into acp-client-tech-spec appendix | Backlog |
| **Novel-writing sync module removed from code** | Archive novel-writing-sync-contract | Module still shipped (V1.36+); sync contract retained |
| **V1.40 shipped (DF-63 closed)** | Mark `entity-scope-model.md` §5.1.1 + `cli-spec.md` §6.2G + `creator-workflow.md` persist + `local-db-schema.md` §4.1.2 + `novel-workflow-profile.md` §3.5.1 as Shipped V1.40 in their headers | **Done 2026-06-11** (see headers + this index) |
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

**Former filename:** `local-platform-isolation-and-crate-architecture.md` → `local-cloud-crate-architecture.md` (2026-05-20).

---

## Maintaining this index

When adding, renaming, or archiving a spec:

1. Set header **`Status`**, **`Document class`**, and **`Coordinates with`** in the spec file.
2. Update the domain table in this README.
3. Update `status.json` `spec_refs` / `wave_0_spec` if wave-0.
4. Do **not** add file lists to AGENTS.md.

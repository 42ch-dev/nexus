# Specs (`knowledge/specs/`)

**Functional and normative specifications** for the Nexus OSS repo: CLI, daemon runtime, ACP, orchestration, creator product lines, and feature contracts.

**Decision rules:** [AGENTS.md](AGENTS.md) — when to split/merge/archive, status lifecycle, authority on overlap.

**Not here:** cross-cutting trackers and schema boundary matrices → parent [knowledge/](../README.md). Iteration delivery evidence → [`.mstar/iterations/`](../../iterations/README.md).

---

## Layout

All spec files live **flat** in this directory (kebab-case filenames, no version suffix). Subdirectories are **intentionally not used** — see [AGENTS.md](AGENTS.md) § Layout invariant.

---

## Master index (by domain)

### Architecture and boundaries

| Document | Status | Role |
| --- | --- | --- |
| [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) | Normative | Local/cloud split, crate graph, contracts-first |
| [entity-scope-model.md](entity-scope-model.md) | Normative | Global/User/Creator/World/… hierarchy and crate ownership |
| [local-runtime-boundary.md](local-runtime-boundary.md) | Normative | CLI / daemon / Local API / ACP topology |
| [schemas-directory-layout.md](schemas-directory-layout.md) | Normative | `schemas/` tree; cloud vs local folders |

Also: [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md) (knowledge root — wire vs local types).

### Runtime and persistence

| Document | Status | Role |
| --- | --- | --- |
| [daemon-runtime.md](daemon-runtime.md) | Normative | Single-binary daemon layering and process model |
| [local-db-schema.md](local-db-schema.md) | Normative | Local `state.db` tables and migrations |
| [reference-store-layout.md](reference-store-layout.md) | Normative | Reference registry + `body.md` storage split |

### CLI product surface

| Document | Status | Role |
| --- | --- | --- |
| [cli-spec.md](cli-spec.md) | Normative (§6.0B legacy six-group until P5) | Per-command detail, flags, daemon mode, §7 first-run |
| [cli-command-ia.md](cli-command-ia.md) | **Draft (V1.35)** | Top-level five-group IA; supersedes cli-spec §6.0B until merge |
| [creator-centric-entry-model.md](creator-centric-entry-model.md) | **Draft (V1.35)** | Creator hub vs platform vs system entry rules |

**Read order (CLI):** `cli-command-ia` → `creator-centric-entry-model` → `cli-spec` for flags/subcommands.

### Orchestration and presets

| Document | Status | Role |
| --- | --- | --- |
| [orchestration-engine.md](orchestration-engine.md) | Active SSOT | Preset loader, capabilities, worker IPC, validator |
| [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) | Active (legacy scope) | Schedule lifecycle + `core_context` (WS7); engine primitives in orchestration-engine |
| [preset-conditional-routing-fl-d.md](preset-conditional-routing-fl-d.md) | **Exploration** | FL-D / DF-56 conditional `next` — not loadable until future compass |

### Creator product lines (shipped features)

| Document | Status | Role |
| --- | --- | --- |
| [work-experience-model.md](work-experience-model.md) | Shipped (V1.33) | Work container, Creative Brief Intake, `creator run`, `run_intents` |
| [creator-workflow-fl-e.md](creator-workflow-fl-e.md) | Shipped (V1.34) | FL-E stages, preset chain, `creator run stage` |
| [creator-challenge-solver.md](creator-challenge-solver.md) | Normative | Creator registration challenge solver |

### ACP and agent integration

| Document | Status | Role |
| --- | --- | --- |
| [acp-client-tech-spec.md](acp-client-tech-spec.md) | Accepted | Worker-delegated ACP hosting, session model |
| [acp-capability-set.md](acp-capability-set.md) | Normative | Logical `nexus.*` capability surface |
| [agent-host.md](agent-host.md) | Normative | Hybrid Managed-only `nexus-agent-host` |
| [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) | Shipped (V1.34) | Agent tools via daemon `HostToolExecutor` |
| [registry-integration.md](registry-integration.md) | Normative | ACP Registry integration |
| [skills-export-compatibility.md](skills-export-compatibility.md) | Normative | Skills export (CLI/local only) |

### Feature contracts and platform companions

| Document | Status | Role |
| --- | --- | --- |
| [novel-writing-sync-contract.md](novel-writing-sync-contract.md) | Normative (V1.15 module) | Novel-writing workspace sync scan rules |
| [canonical-hash.md](canonical-hash.md) | Companion | OSS implementation notes; platform ADR-006 is authority |

---

## Normative hierarchy (read order)

When specs overlap, use this order (higher wins on conflict):

1. **`AGENTS.md`** (repo root) — naming, contracts, release discipline.
2. **[local-cloud-crate-architecture.md](local-cloud-crate-architecture.md)** — local vs cloud product lines, crate graph.
3. **[entity-scope-model.md](entity-scope-model.md)** — scope hierarchy and crate ownership.
4. **[schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md)** — `schemas/` vs `nexus-contracts/src/local/`.
5. **[schemas-directory-layout.md](schemas-directory-layout.md)** — folder tree layout.
6. **Active iteration compass** — delivery batching only (does not override shipped normative specs except where explicitly marked draft overlay).
7. **Topology / runtime** — [local-runtime-boundary.md](local-runtime-boundary.md), [daemon-runtime.md](daemon-runtime.md), CLI cluster (above).
8. **Persistence** — [local-db-schema.md](local-db-schema.md), [reference-store-layout.md](reference-store-layout.md).
9. **Subsystem / feature specs** — orchestration, ACP, creator product lines, feature contracts.
10. **Exploration specs** — design input only; no implement authority until status promoted.

**Former filename:** `local-platform-isolation-and-crate-architecture.md` → renamed **2026-05-20** to `local-cloud-crate-architecture.md`.

---

## Authority matrix (overlapping topics)

| Topic | Primary SSOT | Secondary / historical |
| --- | --- | --- |
| Top-level CLI groups (5 vs 6) | **cli-command-ia** (draft) | cli-spec §6.0B until V1.35 P5 merge |
| First-run / local vs platform path | **creator-centric-entry-model** (draft) + cli-spec §7 | Compass Appendix A (audit evidence) |
| `creator run` / Work entity | **work-experience-model** | cli-spec §6.2, orchestration run_intents |
| FL-E stages / preset chain | **creator-workflow-fl-e** | work-experience-model § extensions |
| Preset YAML / loader / validator | **orchestration-engine** | creator-schedule §7 YAML additions |
| Schedule + core_context semantics | **creator-schedule-and-core-context** | orchestration-engine § sessions |
| Conditional preset routing | **preset-conditional-routing-fl-d** (exploration) | orchestration-engine §7.5 stub |
| Agent `nexus.*` tools | **agent-nexus-tool-bridge** | acp-capability-set, agent-host |
| ACP session / worker process | **acp-client-tech-spec** | daemon-runtime, local-runtime-boundary |
| KB / knowledge naming (KCA-003) | **entity-scope-model** §5.4 + cli-command-ia §3.2 | cli-spec §6.2E–F |

---

## Consolidation roadmap (long-term)

Planned hygiene — execute at iteration close or dedicated spec plan; details in [AGENTS.md](AGENTS.md).

| When | Action |
| --- | --- |
| **V1.35 P5** | Merge `cli-command-ia` + `creator-centric-entry-model` into `cli-spec`; archive draft stubs |
| **FL-D compass locks** | Promote `preset-conditional-routing-fl-d` → normative; update orchestration-engine §7.5 |
| **Optional ACP hygiene** | Fold `skills-export-compatibility` into acp-client-tech-spec appendix |
| **If novel-writing sync retired** | Archive `novel-writing-sync-contract.md` |

**Not recommended:** merge `creator-schedule-and-core-context` into orchestration-engine (568 lines, distinct WS7 scope); merge ACP cluster into one file (independent evolution).

---

## Platform cross-repo references

When a spec needs platform architecture, shared contracts, or ADRs, cite **`nexus-platform`** paths (side-by-side checkout: `../nexus-platform/.mstar/designs/...`):

| Need | Platform path |
| --- | --- |
| Architecture umbrella | `v1-spec/architecture.md` |
| ADR | `v1-spec/adr/{adr-file-name}.md` |
| Shared contracts | `v1-spec/shared/...` |
| Platform HTTP / product | `v1-spec/platform/...` |

**Wire JSON in this repo:** `schemas/` → `nexus-contracts`.

---

## Archived (superseded specs)

| Former spec | Notes |
| --- | --- |
| [daemon-api-workspace-write-architecture.md](../../archived/knowledge/daemon-api-workspace-write-architecture.md) | Stale route table |
| [local-fs-layout-creator-workspace.md](../../archived/knowledge/local-fs-layout-creator-workspace.md) | Pointer stub |
| `nexus42-single-binary-daemon-runtime-architecture.md` | Merged into [daemon-runtime.md](daemon-runtime.md) |
| `agent-host-architecture.md` | Merged into [agent-host.md](agent-host.md) §8 |
| [fl-d-conditional-routing-exploration-v1.35-prepare.md](../../archived/knowledge/fl-d-conditional-routing-exploration-v1.35-prepare.md) | Superseded by [preset-conditional-routing-fl-d.md](preset-conditional-routing-fl-d.md) |

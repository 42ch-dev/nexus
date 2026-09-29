# Novel writing specs (`specs/novel-writing/`)

Normative specifications for `work_profile: novel` — layout, presets, quality loop, author desk, and sync.

**Relocated**: 2026-06-17 from flat `specs/novel-*.md` (spec hygiene).

**Parent index**: [specs/README.md](../README.md) · **Rules**: [specs/AGENTS.md](../AGENTS.md)

---

## Read order

```text
workflow-profile.md     → artifact layout, chapter SSOT, preset gates, completion, cron staggering (§11), auto-chronology (§11.5)
quality-loop.md         → findings, review presets, rules, Logs, 96h escalation
author-experience.md    → author path, status UX, remediation copy
sync-contract.md        → shipped chapter discovery/bundle library (companion; not a cloud upload path)
multi-work-lifecycle.md → completion lock, reopen, runtime_lock (V1.41+)
work-pool.md            → selection + inspiration pools (V1.41)
manuscript-audit.md     → DF-69 on-demand audit (out-of-band)
```

**Draft overlays (V1.49 / V1.50 P-last — folded)**:

| Overlay | Merge target | Status |
| --- | --- | --- |
| `findings-lifecycle.md` | [Cross-profile findings Master](../findings-lifecycle.md) owns lifecycle, `target_executor` routing, and UI remediation; `quality-loop.md` §2 owns only the novel produce side | **Superseded** — historical overlay folded into `quality-loop.md` §2 at V1.49 P-last; lifecycle authority promoted to the root Master at V1.77 |
| `narrative-indexes.md` | `workflow-profile.md` §4.6 | **Superseded** (V1.49 P-last) |
| `cron-staggering.md` | `workflow-profile.md` §11 | **Superseded** (V1.50 P-last) |
| `auto-chronology.md` | `workflow-profile.md` §11.5 | **Superseded** (V1.50 P-last) |

---

## Document index

| Document | Class | Status |
| --- | --- | --- |
| [workflow-profile.md](workflow-profile.md) | Feature line | Shipped V1.36 → V1.50 (§11 cron + auto-chronology) |
| [quality-loop.md](quality-loop.md) | Feature line | Normative — V1.51 Shipped |
| [author-experience.md](author-experience.md) | Feature line | Shipped V1.49 P2 |
| [manuscript-audit.md](manuscript-audit.md) | Feature line | Shipped V1.44 |
| [multi-work-lifecycle.md](multi-work-lifecycle.md) | Feature line | Shipped V1.41 |
| [work-pool.md](work-pool.md) | Feature line | Shipped V1.41 |
| [sync-contract.md](sync-contract.md) | Companion | Normative — shipped V1.36 layout library (`nexus-orchestration::sync_module`); no cloud upload integration |

---

## Authority matrix (novel domain)

| Topic | Primary SSOT |
| --- | --- |
| `Works/<work_ref>/` layout + chapter frontmatter | `workflow-profile.md` |
| Per-Work cron staggering (3-role) | `workflow-profile.md` §11 |
| Per-Work auto-chronology (opt-in) | `workflow-profile.md` §11.5 |
| Cross-profile findings lifecycle, executor routing, UI remediation | [findings-lifecycle.md](../findings-lifecycle.md) (Master) |
| Novel findings produce side / review chain | `quality-loop.md` §2 |
| F### / E### index files | `workflow-profile.md` §4.6 (5-col schema) |
| World KB promotion state machine | [entity-scope-model.md §5.5](../entity-scope-model.md#55-world-kb-promotion-state-machine-v150-normative) |
| Author happy path + remediation copy | `author-experience.md` |
| On-demand chapter audit | `manuscript-audit.md` |
| Multi-work completion + locks | `multi-work-lifecycle.md` |
| Pool / default Work | `work-pool.md` |
| Sync scan roots | `sync-contract.md` (layout SSOT: `workflow-profile.md` §3, §7) |
| Top-level CLI groups / preset dispatch | Current authority: [cli-spec.md §6.0B](../cli-spec.md#60b-v2-命令信息架构权威). [creator-run-preset-entry.md](../creator-run-preset-entry.md) is historical: generic runner retired v1.193 P2-T1; no replacement CLI preset-dispatch entrance. |

---

## Maintaining this subtree

1. Edit canonical files under `novel-writing/` only.
2. On overlay promotion (P-last), fold into the merge-target Master and archive the overlay.
3. Update this README when adding or retiring a novel spec.

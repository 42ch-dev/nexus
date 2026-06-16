# Novel writing specs (`specs/novel-writing/`)

Normative specifications for `work_profile: novel` — layout, presets, quality loop, author desk, and sync.

**Relocated**: 2026-06-17 from flat `specs/novel-*.md` (spec hygiene).

**Parent index**: [specs/README.md](../README.md) · **Rules**: [specs/AGENTS.md](../AGENTS.md)

---

## Read order

```text
workflow-profile.md     → artifact layout, chapter SSOT, preset gates, completion
quality-loop.md         → findings, review presets, rules, Logs, 96h escalation
author-experience.md    → author path, status UX, remediation copy
sync-contract.md        → chapter sync module scan rules (companion)
multi-work-lifecycle.md → completion lock, reopen, runtime_lock (V1.41+)
work-pool.md            → selection + inspiration pools (V1.41)
manuscript-audit.md     → DF-69 on-demand audit (out-of-band)
```

**Draft overlays (V1.49 P-last — folded)**:

| Overlay | Merge target | Status |
| --- | --- | --- |
| [findings-lifecycle.md](findings-lifecycle.md) | `quality-loop.md` §2 | **Superseded** (V1.49 P-last) |
| [narrative-indexes.md](narrative-indexes.md) | `workflow-profile.md` §4.6 | **Superseded** (V1.49 P-last) |

---

## Document index

| Document | Class | Status |
| --- | --- | --- |
| [workflow-profile.md](workflow-profile.md) | Feature line | Shipped V1.36 → V1.49 |
| [quality-loop.md](quality-loop.md) | Feature line | Shipped V1.47 → V1.49 |
| [author-experience.md](author-experience.md) | Feature line | Shipped V1.49 P2 |
| [manuscript-audit.md](manuscript-audit.md) | Feature line | Shipped V1.44 |
| [multi-work-lifecycle.md](multi-work-lifecycle.md) | Feature line | Shipped V1.41 → V1.42 |
| [work-pool.md](work-pool.md) | Feature line | Shipped V1.41 |
| [sync-contract.md](sync-contract.md) | Companion | Normative (module contract) |
| [findings-lifecycle.md](findings-lifecycle.md) | Draft overlay (archived) | Superseded (V1.49 P-last) |
| [narrative-indexes.md](narrative-indexes.md) | Draft overlay (archived) | Superseded (V1.49 P-last) |

**Archived**: [novel-findings-maturity.md](../../archived/knowledge/novel-findings-maturity.md) — superseded; folded into `quality-loop.md` §9.

---

## Authority matrix (novel domain)

| Topic | Primary SSOT |
| --- | --- |
| `Works/<work_ref>/` layout + chapter frontmatter | `workflow-profile.md` |
| Findings lifecycle + review chain | `quality-loop.md` §2 (6-state F6) |
| F### / E### index files | `workflow-profile.md` §4.6 (5-col schema) |
| Author happy path + remediation copy | `author-experience.md` |
| On-demand chapter audit | `manuscript-audit.md` |
| Multi-work completion + locks | `multi-work-lifecycle.md` |
| Pool / default Work | `work-pool.md` |
| Sync scan roots | `sync-contract.md` (layout SSOT: `workflow-profile.md` §3, §7) |
| Top-level CLI groups / preset dispatch | [cli-spec.md](../cli-spec.md), [creator-run-preset-entry.md](../creator-run-preset-entry.md) |

---

## Maintaining this subtree

1. Edit canonical files under `novel-writing/` only.
2. On overlay promotion (P-last), fold into the merge-target Master and archive the overlay.
3. Update this README when adding or retiring a novel spec.

# Novel Author Experience — Normative Supplement v1

**Status**: Shipped (V1.43 — 2026-06-12; V1.45 overlay 2026-06-14)  
**Document class**: Feature line (author experience supplement)  
**Created**: 2026-06-12  
**Last updated**: 2026-06-14 (V1.45 P3 — CLI surface updated to preset-id commands per compass §2 migration appendix)
**Scope**: End-user **ongoing serial** happy path — maps [docs/novel-writing-quickstart.md](../../../docs/novel-writing-quickstart.md) (BL-10) to normative CLI surfaces and P1/P2 implement contracts  
**Coordinates with**:

- [creator-centric-entry-model.md](creator-centric-entry-model.md) — §3.1 local bootstrap (≤7 steps)
- [cli-spec.md](cli-spec.md) — §7 first-run UX principles
- [novel-workflow-profile.md](novel-workflow-profile.md) — artifact layout + completion §6
- [novel-quality-loop.md](novel-quality-loop.md) — findings + review visibility (P2)
- [creator-workflow.md](creator-workflow.md) — FL-E stage names in Part I narrative

**Iteration compass**: [v1.43-novel-author-experience-delivery-compass-v1.md](../../iterations/v1.43-novel-author-experience-delivery-compass-v1.md)

---

## 1. Purpose

V1.36–V1.42 implemented novel-writing **capabilities** across crates. V1.43 does **not** add a new profile or preset grammar. It defines the **author-facing contract** for:

1. **Part I — Ongoing serial** (compass grill-me **C**): bootstrap → World-bound Work → first finalized chapter → auto-chain chapter 2+ → completion stop → quality-loop signals authors can understand.
2. **Part II — Appendix** (compass grill-me **B**): multi-work switch, multi-volume, inspiration pool — documentation only; no new runtime requirements in V1.43.

---

## 2. Quickstart document structure (BL-10)

| Section | Title | Implement owner |
| --- | --- | --- |
| Part I §1 | Prerequisites & bootstrap (`system doctor` … `creator bootstrap`) | P0 doc; P1 copy |
| Part I §2 | World + `novel-project-init` | P0 doc; P1 gate/scaffold errors |
| Part I §3 | First chapter: outline → draft → finalize | P0 doc |
| Part I §4 | Serial: auto-chain, `creator works status`, chapter N | P0 doc; P2 visibility |
| Part I §5 | Quality loop: findings, review, 96h banner | P0 doc; P2 visibility |
| Part I §6 | Completion: when writing stops | P0 doc; P2 visibility |
| Part II A | Multi-work desk (`creator works …`) | P0 doc only |
| Part II B | Multi-volume (`volume` in status tables) | P0 doc only |
| Part II C | Inspiration pool (optional) | P0 doc only |

**Invariant**: Every command in Part I must exist in [cli-spec.md](cli-spec.md) or [cli-command-ia.md](cli-command-ia.md) at ship time.

---

## 3. CLI copy alignment (P1)

When any of the following conditions occur, CLI or daemon **user-visible** output must include a **single-line next action** referencing the quickstart section id (e.g. `See docs/novel-writing-quickstart.md §3`):

| Condition | Minimum remediation |
| --- | --- |
| Daemon not reachable | - [x] Shipped (V1.43 P1) — Start daemon; cite Part I §1 step 5 |
| `preset_gates_failed` | - [x] Shipped (V1.43 P1) — Name failing gate; cite Part I §2 or §3 |
| Missing scaffold / intake incomplete | - [x] Shipped (V1.43 P1) — Cite Part I §2 |
| Work completed (auto-chain stopped) | - [x] Shipped (V1.43 P1) — Cite Part I §6 |
| Open findings blocking progress (if applicable) | - [x] Shipped (V1.43 P1) — Cite Part I §5 |

**Non-goals (P1)**: New commands, new API fields, interactive wizards.

---

## 4. Author visibility (P2)

Authors must be able to answer without reading JSON APIs:

| Question | Surface (minimum) | Status |
| --- | --- | --- |
| Which chapter is active? | `creator run status` or `creator works status` | — [x] Shipped (V1.43 P2) — `current_chapter` + chapter table in status output |
| Is the Work complete? | Clear terminal/completed marker per novel-workflow-profile §6 | — [x] Shipped (V1.43 P2) — completed banner with `COMPLETED` marker, quickstart §6 link |
| Are there open findings? | Count + severity summary; link to review preset name | — [x] Shipped (V1.43 P2) — `findings:` line with severity breakdown, top findings, review hint |
| Is 96h master-review banner active? | Existing daemon banner; ensure visible in status path | — [x] Shipped (V1.39 P4 T3) — already wired in `creator works status`; verified V1.43 P2 |
| How do I run master-decision review? | `creator run novel-review-master [<work_id>] [--finding-id <id>] [--auto-schedule]` — enqueues the `novel-review-master` preset for master decisions on open findings; use `creator works status` to list findings first | — [x] Shipped (V1.45 P0–P2) — see [novel-quality-loop.md](novel-quality-loop.md) §3.4 |

Normative detail remains in [novel-quality-loop.md](novel-quality-loop.md); P2 implements **presentation** only unless a spec gap is found (then amend loop spec in same plan).

---

## 5. P-last author-path tech-debt (pointer)

See plan [2026-06-12-v1.43-hygiene-and-residuals.md](../../plans/2026-06-12-v1.43-hygiene-and-residuals.md) §2 for residual IDs. This spec does not duplicate `status.json` rows.

---

## 6. Promotion (iteration close)

At V1.43 P5/P-last hygiene:

- [x] **Kept as Feature line supplement** — §2–§4 map quickstart sections to CLI surfaces and shipped implementation status. No merge into cli-spec.md required; this document continues to serve as the author-experience normative supplement.
- [x] **Status promoted to Shipped (V1.43)** — all P0/P1/P2 surfaces implemented; §2 row 4 amended (`creator run status` → `creator works status` per R-V143P0-001).
- [x] **R-V143P0-002 registered in V1.44 compass** — [v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md](../../iterations/v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md) P1; implement via [novel-quality-loop.md](novel-quality-loop.md) §3.4.
- [ ] **DF-69 audit entry** — quickstart §5 cross-ref when P0 ships ([novel-manuscript-audit.md](novel-manuscript-audit.md)).

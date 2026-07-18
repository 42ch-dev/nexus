# V1.122 — Three-pillar product framing (iteration-scoped)

> **Status:** Draft (Phase 1 product-manager). Normative ship lives in P0 edits to `STRATEGY.md`, `CONCEPTS.md`, and `.mstar/specs/*`. This file is the iteration-scoped product framing for Review & Edit + implement handoff.
>
> **Not knowledge:** Do not promote to `{KNOWLEDGE_DIR}/` until iteration-close (`mstar-compound`).

## One-sentence thesis

**Nexus is the local-first creative-writing tool where a World's Timeline is the central instrument, AI agents are harnessed through Canvas, and Computable modules make worlds react.**

## Pillars

| Pillar | Author language | Domain mapping (today) | V1.122 deliverable | Deferred |
|--------|-----------------|------------------------|--------------------|----------|
| **Harness** | Control how AI agents execute creative work | Orchestration engine + agent host + capability registry + presets; UI copy still "Strategy/Preset" | Canonize name + definition in STRATEGY/CONCEPTS; cross-ref `orchestration-engine.md` | UI rename Strategy → Harness (`DF-V1122-HARNESS-RENAME`) |
| **Canvas** | Spatial steering surface for seeing and shaping creative material | React Flow shell; Strategy / Outline+Timeline / World KB surfaces | Canonize Canvas pillar; **elevate Timeline** as World-building hero peer surface | Deeper WB, Fork UI (`DF-V1122-DEEPER-WB`, `DF-V1122-FORK-UI`) |
| **Computable** | Worlds that *react* via WASM modules | `compute-module-abi` + `wasm-host` + combat-engine preset | Canonize pillar (distinct from `Compute (Capability)` mechanism) | Computable UI surfacing; compute-on-timeline |

## Naming rules (product)

1. **Harness** is a product pillar name, not a new runtime crate. Do not invent a parallel domain object that duplicates Orchestration/Agent Host.
2. **Computable** (pillar) ≠ **Compute (Capability)** (mechanism). Pillar = product thesis; capability = WASM unit authors/agents invoke.
3. **Canvas** is the product surface family; **Timeline** is the hero surface *within* Canvas for World building.
4. UI strings this iteration: keep "Strategy", "Outline", "World KB" labels unless P1 needs a minimal "Timeline" nav label for the new surface. No global Harness rename.

## Spine vs projection (domain alignment)

| Layer | Concept | Product surface | Role |
|-------|---------|-----------------|------|
| Spine | World, Timeline, Fork, KeyBlock | Timeline (hero), World KB (peer) | Truth of the narrative universe |
| Projection | Work, Outline, Manuscript | Outline (Work default), Reading | Authoring plan and prose bound to a World |

Authors should feel: **World first for World building; Work first for chapter writing.** Dual entry defaults encode that (see `timeline-hero-product-spec.md`).

## Success for this framing (P0)

Binary checks live as **AC-V1122-1..4** on the delivery compass. This framing is successful only if STRATEGY Vision + CONCEPTS entries + canvas Draft overlay all use the same pillar names and spine/projection language without contradiction.

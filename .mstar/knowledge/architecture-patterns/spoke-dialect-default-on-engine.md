---
module: nexus-spoke-adapter, nexus-moment-context-assembly
date: 2026-08-04
problem_type: knowledge
category: architecture-patterns
severity: medium
plan_id: 2026-08-04-v1.149-p0-activation-engine-default-on-and-spoke-dialect
tags: [spoke-dialect, modules-activation, lore-activation, regex-redos, neutral-byte-equivalence, default-on, consumer-only]
applies_when: consuming a spoke `modules.*` dialect as a nexus engine; promoting a flag-gated spike to default-on; parsing untrusted author regex
---

# Spoke-dialect consumption as a default-on engine — the lore activation pattern

## Context

V1.149 P0+P1 promoted the V1.146 flag-gated `apply_activation` spike to a **default-on**, spoke-dialect-faithful lore-activation engine with Relation hop expand (DF-74). The spike → production promotion surfaced several non-obvious traps (truth-table logic rewrite, ReDoS, CJK panic, duplicate pull, neutral-only byte-equivalence). This pattern distills the durable lessons for future spoke-dialect consumption (DF-75 preset slots, DF-76 inspector, or any new `modules.*` engine).

## Guidance

### 1. Consumer-only — spoke owns the dialect wire, nexus owns the engine

The spoke handbook (`domain-profile-lore-activation.md`) defines the `modules.activation` field set (`keys`/`secondary_keys`/`logic`/`constant`/`order`/`priority`/`position_hint`/`match`). Nexus **parses** these; it must NEVER push matching/activation logic into `spoke-operations`. The engine (`apply_activation` + `expand_relation_hops`) lives in `nexus-spoke-adapter`; MCA calls it generically (V1.146 Architecture Lock Decision 5 — MCA stays dialect-unaware). Unknown fields are ignored + round-tripped (serde defaults, no `deny_unknown_fields`); spike aliases (`key`→`keys`, `constant_seeds∋self`→`constant`) bridge one minor version.

### 2. The handbook truth table is NOT the obvious logic

V1.146 spike evaluated **primary keys only**. The handbook truth table is: `secondary_keys` absent/empty ⇒ fires on **ANY primary match**, `logic` ignored; `secondary_keys` present ⇒ `and_any`/`and_all`/`not_any`/`not_all` combine primary+secondary. When promoting a dialect-driven engine from spike to production, **re-read the handbook truth table** — do not carry the spike's simplified logic forward. Regression-test the old-vs-new behavior explicitly.

### 3. ReDoS — use the linear-time `regex` crate for untrusted author input, NOT `regress`

The workspace pins `regress` (a backtracking JS-regex engine) for **spoke codegen** fidelity to JSON Schema `format:regex` (ECMA 262). But `regress` has **no timeout API** and is catastrophically vulnerable to ReDoS on untrusted input: `(a+)+b` on 28 chars → >1.5s; `.*a.*b` on 6.5KB → >1.5s. The spoke handbook declares regex flavor **product-local** → nexus may choose the linear-time `regex` crate (ReDoS-immune by construction, no backrefs/lookaround — which keyword matching doesn't need). 

**Decision rule:** `regress` for codegen (ECMA 262 fidelity); `regex` for any runtime matching of untrusted/author-supplied patterns. The key≤256/scan≤64KiB input-size bounds cap **input size**, NOT **runtime** — they do not prevent backtracking blowup.

### 4. Default-on promotion → neutral-only byte-equivalence is the HARD ship gate

When promoting a feature from flag-gated to default-on, the critical regression risk is: **authors without the feature (no `modules.activation`) must get byte-identical assembled output**. This is verified by a golden test (default-on output == explicit-off output, byte-compared via the shipped serializer). Without this golden, the default-on change silently regresses every author who hasn't opted into the feature. Make the golden a ship gate (AC-level), and use a **single shared store** in the test (two separately-seeded HashMap stores have non-deterministic iteration order → flaky golden).

### 5. Graph hop expand via storage list, not spoke RelationPort

Spoke `RelationPort` is **get/put only** (no list-by-entity). For hop expansion (DF-74), use a nexus **inherent** adapter method (`NexusAdapter::list_hop_edges_for_world` wrapping `list_relationships_for_world` storage) → pure `HopEdge[]` into the engine. The engine is pure (BFS over edges); the adapter loads from storage; MCA passes edges in. Pre-seed `visited` with ALL primary-matched ids (not just hop seeds) to prevent duplicate pulls of neutral entries already in the matched set.

### 6. `whole_word` match must be char-boundary-safe

Multi-byte (CJK) keys + `whole_word` match: advancing `offset = start + 1` splits a char → panic. Advance by `chars().next().len_utf8()`. Test with CJK keys (`whole_word_match("王", "王宫")`).

## Why this matters

- The ReDoS trap is invisible until production (author packs import untrusted regex; default-on puts it on every `assemble_moment`). The `regress`→`regex` swap is the single most important security decision in V1.149.
- The neutral-only byte-equivalence guarantee is what makes "default-on" safe for existing authors. Without it, promoting a dialect feature from spike to default-on silently changes everyone's assembled context.
- The handbook-truth-table rewrite + pre_visited dedup are the kind of "the spike was simplified, production must be handbook-faithful" traps that recur in any dialect-driven engine promotion.

## When to apply

- Consuming a new spoke `modules.*` dialect (DF-75 preset slots, DF-76 inspector).
- Promoting any flag-gated spike to default-on.
- Parsing untrusted author-supplied regex (always linear-time `regex`, never `regress`).
- Building a graph-expansion engine over the relation storage (use inherent adapter list, pre-seed visited).

## Examples

- V1.149 P0 `crates/nexus-spoke-adapter/src/adapter/activation.rs` (default-on engine, truth-table logic, `regex` crate match, char-boundary whole_word, neutral-only golden).
- V1.149 P1 `expand_relation_hops` + `NexusAdapter::list_hop_edges_for_world` (graph hops via storage list, pre_visited dedup).
- Spec: `.mstar/specs/spoke-adapter-architecture.md` §7.4 Lore activation engine (normative contract).
- Spoke handbook: `spoke/.mstar/specs/domain-profile-lore-activation.md` (the dialect — consumer-only).

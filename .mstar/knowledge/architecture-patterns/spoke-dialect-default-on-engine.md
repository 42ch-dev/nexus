---
module: nexus-moment-context-assembly / nexus-spoke-adapter
date: 2026-08-04
last_updated: 2026-08-05
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-08-04-v1.149-p0-activation-engine-default-on-and-spoke-dialect
tags: [lore-activation, spoke, modules-activation, mca, assemble-moment, neutral-only, byte-equivalence, default-on, preset-slots, moment-directive, df-75]
applies_when: consuming a spoke modules.* dialect / promoting a flag-gated engine to default-on / building prompt-control over activated lore
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

---

## V1.150 (DF-75) extension — preset slots + Moment Directive: product-local prompt control over activated lore

V1.150 extends the default-on engine pattern with two **product-local** prompt-control layers that consume the V1.149-emitted activated candidate list. Neither touches spoke wire.

### 7. Slot routing = thin post-activation step (never matching logic)

Once V1.149 decides what fires (the activated candidate list, priority-then-order), a **pure routing step** reads each entry's parsed `modules.activation.position_hint` / `outlet` and assigns it to a named slot (`world.before` / `world.after` / `kb.outlet.<name>` / `style.post_history` / default fallback). Slots render as sub-sections inside `## World Knowledge Base` in a fixed order; within-slot order is the V1.149 order unchanged. **Key discipline:** routing adds NO matching logic, NO spoke calls, NO source-entry mutation — it only shapes assembly output. The same neutral-only byte-equivalence guarantee extends: an entry with no `position_hint` lands in the default fallback, which must be byte-identical to the V1.149 flat block (P0 golden test). `position_hint:"depth"` is parsed + preserved but NOT actioned (chat-history depth is not Nexus-native).

### 8. Activation off-switch gates ALL activation-product shaping

The V1.149 off-switch (`NEXUS_MCA_LORE_ACTIVATION=off` ⇒ "every candidate entry unchanged, V1.146 flag-off semantics") must gate **both** activation AND every downstream activation-product step (slot routing, generation-stage gating). When off, skip routing entirely and emit the flat V1.149 block — do NOT reshape hinted entries into sub-sections. This was a P0 QC Warning (the routing step initially ran outside the `activation_enabled` branch); the contract is: off-switch means the activation product is invisible, not just the matcher.

### 9. Moment Directive is product-local prompt control, NEVER on spoke wire

The Moment Directive (Author's-Note analogue: body + insert depth + TTL generations/chapters + clear-on-scene-change, scoped per-Work with World override) injects into a distinct `## Moment Directive` section above lore. It is **not** a `modules.*` object, **not** a KnowledgeEntry, **not** an AssemblePacket `placement[]`/`activation_trace[]` entry, never in pack export/import. Persistence is a local-DB table (`moment_directives`) mirroring the `prompt_injection.rs` repository pattern but with a persistent scoped-directive lifecycle (at-most-one-active per scope, soft-delete + `replaced_by` audit), NOT the injection queue. The directive loads via a `DirectiveStore` trait with a `NoDirectiveStore` default so `assemble_moment` stays decoupled (and the no-directive path emits nothing — byte-equivalent). Compile-time `query_as!` is mandatory for static SELECTs in `nexus-local-db` (crate rule); runtime `query_as` without a SAFETY comment is a QC-blocker.

### 10. Generation-stage gating is internal-only (`wire_contracts_changed: false`)

Slot filling is gated by the creator-workflow `stage` (`intake`/`research`/`produce`/`review`/`persist` + `work_maintenance`/`system_maintenance`) via an internal `MomentRequest.generation_stage: Option<GenerationStage>` field. `MomentRequest` is NOT in `schemas/` (it's constructed in the CLI/orchestration, internal to MCA) → adding the field is **not** a wire/contracts change. `run_intent` is **derivable** from `stage` (no separate field). Default `None` ⇒ `Unspecified` ⇒ all slots on (neutral golden + direct-CLI/inspector path). The `Unspecified` arm must be a true zero-cost pass-through (byte-identical to pre-gate) — the stage gate is a no-op for the default path.

### Why V1.150 matters (additive to V1.149)

- The **layered contract**: spoke owns the dialect wire (`modules.activation`); nexus owns the engine (V1.149) + the prompt-control layers (V1.150 slots + Directive) + the gating (V1.150 stage). Each layer is additive and independently byte-equivalence-safe for the neutral author.
- The **off-switch scope** lesson (§8): a flag that disables a stage of a pipeline must disable every downstream product of that stage, not just the stage itself.
- The **product-local-vs-wire** discipline (§9): author-facing prompt control (Directive) and prompt shaping (slots) are never pushed onto the cross-product dialect wire — they live in MCA / orchestration / local-DB only.

### V1.150 examples

- V1.150 P0 `crates/nexus-moment-context-assembly/src/slots.rs` (slot routing + emit order; gated behind `activation_enabled`).
- V1.150 P1 `crates/nexus-moment-context-assembly/src/directive.rs` + `crates/nexus-local-db/src/moment_directive.rs` (Moment Directive + `DirectiveStore` trait + `NoDirectiveStore` default + compile-time `query_as!`).
- V1.150 P2 `crates/nexus-moment-context-assembly/src/generation.rs` (`apply_stage_gate` §4 matrix; `Unspecified` zero-cost pass-through).
- Spec: `.mstar/specs/spoke-adapter-architecture.md` §7.4 (V1.150 slot + Directive matrix promoted by P2 sweep).
- Iteration guide: `.mstar/iterations/v1.150/guides/mca-section-audit.md` (MCA section-heading evidence).

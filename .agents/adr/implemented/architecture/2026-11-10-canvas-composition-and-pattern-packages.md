# ADR: Canvas Composition and Pattern Packages — Strategy Orchestration Cutover

Status: implemented — the cutover landed across v1.239–v1.248: P3 composition and approval (v1.239), Apply/Execute (v1.241), the P4 dual-read loader (v1.242–v1.243), P5 IR-primary authoring (v1.243), P6 conditions 2–7 (v1.243–v1.247), and P6 condition 1 in v1.248, where embedded and sample strategies construct their `WorkflowDocument` from the embedded `preset.yaml` plus compiled-in assets, `resolve_applied_document` returns that document with `apply_id = "embedded:<preset_id>"` and the canonical content hash, admission freezes the identity, recovery re-compares it, and every remaining embedded execution path refuses rather than building a manifest graph. `preset.yaml` stays the authoring and persistence format; `ir-twins/*.json` is a CI equivalence artifact and no production path reads it.
Date: 2026-11-10
Scope: The strategy-orchestration cutover, complete. Every shipped surface is described by the composition spec rather than here: the Canvas composition surface, the compose operations, the pattern packages, the candidate lifecycle up to Approve, Apply/Execute, the P4 dual-read loader, P5 IR-primary authoring, the IR-primary Canvas intake with `preset generate`, and the embedded-strategy IR execution source. `preset.yaml` import/export is retained as the compatibility format.
Source: split from the archived Canvas + Workflow IR redesign and Dynamic Workflow Generation and Orchestration Simulation records

## Problem

P0/P1 shipped the [Workflow IR document model](../../implemented/architecture/2026-11-10-workflow-ir-document-model.md),
P2 shipped the [strategy simulation fence](../../implemented/architecture/2026-11-10-strategy-simulation-fence.md),
P3 shipped composition and approval in v1.239, and P3.5 shipped Apply/Execute in v1.241. Canvas
composes, packages are published with pinned identity, a candidate can be simulated and approved,
and an approved snapshot becomes the identity production intake reads.

Two stakes drove this record. Both were closed with a limit rather than cleanly, and both limits are
closed too: approved identity is no longer persisted-but-never-applied, and the candidate lifecycle no
longer stops at Approve. The applied identity survives admission and recovery (I-000443), and an
unbuildable document no longer applies — `validate_ir_buildability` runs in
`crates/nexus-core/src/apply.rs` before the apply row is written, so the refusal intake would raise
(I-000448) lands as `DocumentUnbuildable` at Apply instead.

1. **Production intake was YAML-first.** A controlled dual-read transition had to exist before
   IR-primary execution could replace the YAML-only mapper. It could begin on imported preset bundles
   — the intake of the time — but the applied path already existed, so dual-read was a loader decision
   rather than a missing stage.
2. **New strategy authoring, compatibility export, and retirement of the hand-authored YAML main path**
   needed explicit exit criteria so the two grammars would not evolve independently.

## Decision

### 1–4. Shipped surface — read the spec, not this record

The composition surface, the nest/compose operations, the pattern-package contract and the candidate
lifecycle up to Approve shipped in v1.239 (#515). Apply/Execute followed in v1.241 (#520) as its own
stage, which P6's fourth acceptance item ("compose and generate candidates pass validate, simulate,
approve, and apply") and §6's rollback-by-new-Apply both assume.

That stage preceded P4 in practice, as predicted: dual-read began by loading imported preset bundles to
prove the IR→Graph mapping, and an *applied* IR identity then existed to load. What P3.5 did not settle
was durability — the applied identity answered admission and recovery from the YAML descriptor, so a
restart reverted to the preset graph (I-000443) — which made that question P4's opening problem rather
than its follow-up. It is closed: see §5.

The shipped contract — manifest fields, the content-hash pin, the four operations and their CAS, the
approval freeze, the flag and its per-audience default — lives in
[the composition spec](../../../../docs/specs/orchestration/strategy-composition.md). Two facts from the
implementation are decisions rather than contract details:

- **The pin is the canonical content hash of the fragment.** The manifest version is a label; a package
  edited under an unchanged version must fail closed, which is why approve records and verifies hashes.
- **A compose operation is refused, not repaired, when the parent level cannot carry what it would
  receive.** `unnest` refuses rather than dropping a body's role/asset/signal declarations or the removed
  slot's own control references.

### 5. P4–P6 ladder, as shipped

P0/P1/P2 are governed by their implemented records. P3 and P3.5 are governed by the composition spec.

P3.5's exit criteria were met as stated — an approved candidate is applied and an existing schedule runs
from that applied identity, and a rollback installs the previous snapshot through a new Apply. The
qualification P3.5 carried — authoritative within one process lifetime only — is closed: admission
resolves a strategy's current apply before it validates bindings, and recovery re-enters that same
resolution, so a daemon restart during an in-flight applied run rebuilds the applied graph. At admission
the role set bound to providers comes from the document that runs, and recovery reads the frozen
descriptor's `agent_bindings` rather than resolving them again (I-000443). That reuse cannot cross
documents: recovery compares the identity admission froze with the strategy's current apply and refuses
with `reconstruction_unavailable` before it builds anything, so a re-apply during an in-flight run is a
refused restart rather than bindings from one document and a graph from another.

The composition spec owns the resulting precedence: a user bundle with a current apply runs from the
applied document, every other strategy keeps its descriptor, and — since v1.248 — an embedded preset
resolves its document from the embedded bundle and freezes `embedded:<preset_id>` with the canonical
content hash, so no embedded execution path reads a manifest graph.

P4's buildability half (I-000448) landed in `da84c588f`: `crates/nexus-core/src/apply.rs` refuses an
unbuildable document through the same `validate_ir_buildability` the intake runs
(`validate_ir_buildability` in `crates/nexus-orchestration/src/preset_runtime.rs`), before the
`INSERT INTO strategy_applies` (`apply_strategy` in `crates/nexus-core/src/apply.rs`). The limits that
remain on the applied path (I-000449, I-000462–I-000465) are recorded there rather than here.

P4's third exit criterion — the IR→Graph mapping compared against the IR→Task table in
orchestration-engine §8.2.1 — is satisfied. The evidence is the amended table itself: §8.2.1 carries nine
amendments (`8e6f22225` plus the QC correction round) and no longer states anything the projection does
not do. Reading §8.2.1 against `build_wired_outer_graph_from_ir`
(`build_wired_outer_graph_from_ir` in `crates/nexus-orchestration/src/preset_runtime.rs`) yields 34
constructs — 25 already matching, 8 whose table row was the stale side, 1 the table omitted, 0 where the
code diverged. Node-type coverage is closed in both directions; the one real omission found — the
inner-graph synthetic `end` task — is now in the table; edge semantics were corrected in four places;
and no third mapping exists. The criterion is tracked by I-000508. The old §8.2.1 prose promised an
`ir_unmappable` refusal for `nests[]` that no guard enforces; the amended table states the truth (the
projection does not read `nests[]`) and the hardening is an issue, not a gap in this decision.

| Phase | Change | Exit criteria |
| --- | --- | --- |
| **P4 Dual-read loader** | Engine loads applied IR or a preset bundle, and applied IR survives admission and recovery | Met: existing YAML-preset schedules ran unchanged; the applied identity is authoritative across a daemon restart; the IR→Graph mapping matches the IR→Task table |
| **P5 IR-primary authoring** | Documentation and samples are authored as IR; YAML export remains compatibility output | Met: repository strategies land as IR |
| **P6 Retire hand-authored YAML main path** | Hand-edit and YAML patch cease to be the default authoring/intake path; import/export remains | Met: the retirement gate below passes, the legacy mapper is off, and the import tool remains |

### 6. Dual-read, coexistence, and rollback

| Stage | Intake/write behavior |
| --- | --- |
| Dual-read | Loader accepts applied IR or preset bundle; both use the shared semantic core |
| IR-primary write | Generate/Compose and new edits write IR with revision CAS; YAML export is derived |
| IR-primary execute | Production admission prefers applied IR identity, including embedded bundles |
| Retire hand-authored main path | The YAML mapper is off for the primary path; partner/integrator import/export remains |

YAML and IR never become independently evolving grammars: `nexus-preset` is the import/compat loader,
retains `PresetSourceIdentity` hash semantics, and shares semantic validation with IR.

Rollback of a bad Apply installs the previous approved snapshot through a new Apply; it never rewrites a
hash silently. Execute continues refusing missing or changed source bytes. Both shipped in v1.241:
rollback is an append naming a superseded approval, and intake refuses bytes that no longer hash to the
recorded identity (`applied_document_changed`) or cannot be read at all
(`applied_document_unverifiable`). Pattern and asset hashes recorded at Approve participate in source
freeze.

### 7. Retirement scope

Retired as the primary paths:

- Hand-authored `preset.yaml` as the primary authoring path.
- TypeScript `parsePresetYaml` + `buildStrategyGraph` as the sole Canvas source.
- `strategy.patch_*` YAML mutators as the sole write path.
- YAML→Graph as the sole production intake.
- Copy-sample-directory reuse and explainability-only Canvas framing.

The retained pieces are graph-flow, `OrchestrationEngine`, SessionStorage, `WaitingForInput` / `wait_id`,
the capability registry, source identity freeze, integrator versus creator-host execution surfaces, and
documented `preset.yaml` import/export. Outline, Timeline, World KB Canvas, Actor/Viewpoint preset IR,
Work-tree artifacts, and a first-party player app are not replaced.

## Alternatives considered

- **Keep Canvas explainability-only and extend YAML patches indefinitely.** Rejected: the human remains a YAML surgeon and cannot compose patterns or drafts as first-class IR units.
- **Make Canvas the only authoring entry or require drag-from-scratch.** Rejected: CLI, Agent, programmatic Compose, and Generate remain valid paths to the same candidate.
- **Make Canvas a read-only Agent-output viewer.** Rejected: it discards the composition duty.
- **Copy sample directories as the reuse mechanism.** Rejected: independently versioned packages with exact content pins provide reusable identity and fail-closed updates.
- **Allow hot update under an unchanged approved version.** Rejected: source identity would change silently; a new Approve/Apply is required.
- **Delete `preset.yaml` import/export at P6.** Rejected: partner/integrator bundles remain a public compatibility format; only the hand-authored main path retires.
- **Maintain YAML and IR as independent grammars.** Rejected: YAML becomes import/projection through the shared semantic core.
- **Big-bang intake replacement.** Rejected: P3.5–P6 preserve existing schedules, emergency YAML re-enable, mixed workspaces, and previous-snapshot rollback.
- **Replace graph-flow, add a first-party player, or absorb Work Artifact IR.** Rejected as separate or explicitly excluded responsibilities.

## Consequences

The retirement gate that P6 had to satisfy, all of it met on a reference workspace by v1.248:

1. Samples and embedded creator strategies run from applied IR with YAML intake off.
2. The primary Strategy Canvas route calls `parsePresetYaml` only inside the legacy import wizard.
3. No CI job requires hand-maintained `preset.yaml` for new strategies; generated export may remain.
4. Compose and Generate candidates pass validate, simulate, approve, and apply.
5. Recovery refuses mismatches in IR content hash and recorded pattern/asset pins.
6. The `yaml-only` kill-switch was removed at its own boundary rather than left as a documented flag.
7. Existing partner/integrator bundles have a deprecation note and import path that converts them into candidates passing validation and simulation.

The public bundle format was never cut without that compatibility path.

### Standing constraints

1. **Encoding and identity:** the implemented document-model record owns the schema/identity baseline; this decision creates no second encoding authority.
2. **Gate placement:** enqueue-time gates remain schedule-owned even when IR has metadata carriers; `cli_args` remains in `meta.cli_args`.
3. **Nest policy:** the depth limit and the choice between `expand_in_candidate` and `defer_until_apply` are exercised without permitting cycles or hidden effects.
4. **Generator home:** builtin capability, Develop-hub tool, and CLI-first operation share one candidate state and do not bypass the fence.
5. **Simulation fidelity:** P2 answers static reachability only; the Canvas preserves its disclosures rather than presenting it as output preview.
6. **Approval trust:** creator versus developer policy for third-party patterns must not create a weaker publish or Approve channel.
7. **Compatibility artifacts:** YAML exports are generated output, never a second authoring SSOT.
8. **Partial regeneration:** local Agent regeneration of nested regions merges by nest id into one candidate rather than creating shadow graphs.
9. **External effects:** simulation never promises exactly-once effects or commits write/external capabilities.
10. **Cutover availability:** YAML-only schedules and partner bundles survive through dual-read, the kill-switch window, and import/export rather than a breaking default change.

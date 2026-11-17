# ADR: Canvas Composition and Pattern Packages — Strategy Orchestration Cutover

Status: proposed — P6 is six of seven conditions in. P3 shipped in v1.239 (#515) and Apply/Execute in v1.241 (#520); P4 and P5 in v1.242–v1.243; P6 conditions 3, 5 and 7 in v1.243 (a committed `preset export` generator, the pin re-comparison at Apply and recovery, and a partner-bundle deprecation note with an import path); condition 6 in v1.243 and its `yaml-only` kill-switch removed at its own v1.244.0 boundary in v1.247; conditions 2 and 4 in v1.247 — the primary Canvas reads applied IR only (`parsePresetYaml` has no caller on that path) and `CoreService::generate_strategy_candidate` ships behind `nexus42 preset generate`, an agent tool and `POST /v1/daemon/strategies/{id}/generate` with a bounded lint-correct loop. **Condition 1 is not met**: samples and embedded creator strategies still execute the embedded `preset.yaml` (`nexus-preset/src/lib.rs` `load_embedded_preset`), and `applied_documents.rs` answers `None` for every non-user bundle, so no embedded strategy runs from applied IR. The twenty `ir-twins` that condition 1 was previously credited for prove twin/YAML conformance, not an execution source — that reading was wrong and this line now states what a symbol can show.
Date: 2026-11-10
Scope: The strategy-orchestration cutover, complete except condition 1. Every shipped surface is described by the composition spec rather than here: the Canvas composition surface, the compose operations, the pattern packages and the candidate lifecycle **up to Approve** (v1.239), **Apply/Execute** (v1.241), the P4 dual-read loader (v1.242–v1.243) and P5 IR-primary authoring (v1.243), the IR-primary Canvas intake of v1.247 with `preset generate`, with `preset.yaml` import/export retained. What remains open is the embedded-strategy execution source: `load_embedded_preset` still reads `preset.yaml`, so condition 1 needs the embedded path switched before this record can move to `implemented/`. `parsePresetYaml` stays exported for an import surface that does not exist yet; its absence is tracked separately rather than claimed here.
Source: split from the archived Canvas + Workflow IR redesign and Dynamic Workflow Generation and Orchestration Simulation records

## Problem

P0/P1 shipped the [Workflow IR document model](../../implemented/architecture/2026-11-10-workflow-ir-document-model.md),
P2 shipped the [strategy simulation fence](../../implemented/architecture/2026-11-10-strategy-simulation-fence.md),
P3 shipped composition and approval in v1.239, and P3.5 shipped Apply/Execute in v1.241. Canvas
composes, packages are published with pinned identity, a candidate can be simulated and approved,
and an approved snapshot now becomes the identity production intake reads. What the cutover still
lacked when this section was written — P4 and P5 have since shipped, and P6's conditions 2 and 4 are
the remainder:

Two stakes that drove this record are closed and are kept only because they explain why the
remaining stages are shaped as they are: approved identity is no longer persisted-but-never-applied,
and the candidate lifecycle no longer stops at Approve. Each was closed with a limit rather than
cleanly, and both limits are closed too: the applied identity survives admission and recovery
(I-000443), and an unbuildable document no longer applies — `validate_ir_buildability` runs in
`crates/nexus-core/src/apply.rs` before the apply row is written, so the refusal intake would raise
(I-000448) lands as `DocumentUnbuildable` at Apply instead.

1. **Production intake was still YAML-first.** A controlled dual-read transition had to exist before
   IR-primary execution could replace the YAML-only mapper. It could begin on imported preset bundles
   — the intake of the time — but the applied path already existed, so dual-read was a loader decision
   rather than a missing stage.
2. **New strategy authoring, compatibility export, and retirement of the hand-authored YAML main path**
   needed explicit exit criteria so the two grammars would not evolve independently.

## Proposal

### 1–4. Shipped surface — read the spec, not this record

The composition surface, the nest/compose operations, the pattern-package contract and the candidate
lifecycle **up to Approve** shipped in v1.239 (#515). **Apply/Execute followed in v1.241 (#520)** as
its own stage, which is what P6's fourth acceptance item ("compose and generate candidates pass
validate, simulate, approve, and apply") and §6's rollback-by-new-Apply both assume.

That stage precedes P4 in practice, as predicted here: dual-read can begin by loading imported preset
bundles (today's intake) to prove the IR→Graph mapping, and an *applied* IR identity now exists to
load. What P3.5 did not settle was durability — the applied identity answered admission and recovery
from the YAML descriptor, so a restart reverted to the preset graph (I-000443) — which made that
question P4's opening problem rather than its follow-up. It is closed: see §5.

The shipped contract — manifest fields, the content-hash pin, the four operations and their CAS, the
approval freeze, the flag and its per-audience default — lives in
[the composition spec](../../../../docs/specs/orchestration/strategy-composition.md). This record keeps
only the reasoning for what is still unshipped.

Two facts from the shipped implementation worth carrying here, because they are decisions rather than
contract details:

- **The pin is the canonical content hash of the fragment.** The manifest version is a label; a package
  edited under an unchanged version must fail closed, which is why approve records and verifies hashes.
- **A compose operation is refused, not repaired, when the parent level cannot carry what it would
  receive.** `unnest` refuses rather than dropping a body's role/asset/signal declarations or the removed
  slot's own control references.

### 5. P4–P6 ladder

P0/P1/P2 shipped and are governed by their implemented records. P3 shipped in v1.239 and P3.5 in
v1.241, both governed by the composition spec. What remains:

P3.5's own exit criteria were met as stated — an approved candidate is applied and an existing
schedule runs from that applied identity, and a rollback installs the previous snapshot through a
new Apply. The qualification P3.5 carried — authoritative within one process lifetime only — is
closed: admission resolves a strategy's current apply before it validates bindings, and recovery
re-enters that same resolution, so a daemon restart during an in-flight applied run rebuilds the
applied graph. At admission the role set bound to providers comes from the document that runs, and
recovery still reads the frozen descriptor's `agent_bindings` rather than resolving them again
(I-000443). That reuse can no longer cross documents: recovery compares the identity admission froze
with the strategy's current apply and refuses with a typed error, `reconstruction_unavailable`, before
it builds anything, so a re-apply during an in-flight run is a refused restart rather than bindings
from one document and a graph from another.
The composition spec owns the resulting precedence: a user bundle with a current apply runs
from the applied document, embedded and system presets have no apply branch, and every other
strategy keeps the YAML descriptor. P4's other half — the buildability predicate apply and intake
must share (I-000448) — landed after this record was written, in `da84c588f`: `crates/nexus-core/src/apply.rs`
refuses an unbuildable document through the same `validate_ir_buildability` the intake runs
(`validate_ir_buildability` in `crates/nexus-orchestration/src/preset_runtime.rs`), and it does so before the `INSERT INTO
strategy_applies` (`apply_strategy` in `crates/nexus-core/src/apply.rs`). The limits that remain on the applied path (I-000449,
I-000462–I-000465) are recorded there rather than here. P4's **third** exit criterion — the IR→Graph
mapping compared against the IR→Task table in orchestration-engine §8.2.1 — is satisfied. That criterion's evidence is the amended table itself, not a working note: §8.2.1 now
carries nine amendments (`8e6f22225` plus the QC correction round) and no longer states anything the
projection does not do. Reading §8.2.1 against `build_wired_outer_graph_from_ir`
(`build_wired_outer_graph_from_ir` in `crates/nexus-orchestration/src/preset_runtime.rs`) yields 34 constructs — 25 already matching,
8 whose table row was the stale side, 1 the table omitted, **0 where the code diverged** — which is
the comparison this criterion asked for, reproducible from the two tracked artifacts by anyone.
What it established, beyond that it ran:
node-type coverage is closed in both directions, the one real omission found — the inner-graph
synthetic `end` task (`build_inner_graphs` in `preset_runtime.rs`) — now being in the table; edge semantics were
corrected in four places (conditional routing is in-task `resolve_expression_target` → `GoTo` with
no builder edge, pinned by `parity_edges_conditional_and_default_route_by_expression` in `preset_runtime.rs`; `default` is the family fallback, not an edge;
`edges[kind=depends_on]` is a phantom with no producer and no reader; and the judge's no-go routes
rather than waits on every routed `next` form, pinned by `parity_exit_when_llm_judge_go_continues_and_nogo_waits` in `preset_runtime.rs`); and no
third mapping exists —
the five `NextTarget` forms all trace to table rows, and refusal names are generated from the
table's own row names (`IrOuterGraphError` in `preset_runtime.rs`). The zero `code-divergent` count is a result, not an
absence of effort: every rule-1 and rule-2 row diverged with the table as the stale side — what a
spec-only plan should expect — and the refusal and carrier rows matched symbol-for-symbol. The
criterion is tracked by I-000508. One hole the comparison surfaced stays open: the old §8.2.1 prose
promised an `ir_unmappable` refusal for `nests[]` that no guard enforces — the amended table now
states the truth (the projection does not read `nests[]`), the behaviour is unchanged, and the
hardening is filed as an issue rather than closed here.

| Phase | Change | Exit criteria |
| --- | --- | --- |
| **P4 Dual-read loader** | Engine loads applied IR or a preset bundle, and applied IR survives admission and recovery | Existing YAML-preset schedules remain unbroken; an applied identity is authoritative across a daemon restart, not only within one process; IR→Graph mapping matches the IR→Task table in orchestration-engine §8.2.1 |
| **P5 IR-primary authoring** | Documentation and samples are authored as IR; YAML export remains compatibility output | New repository strategies land as IR |
| **P6 Retire hand-authored YAML main path** | Hand-edit and YAML patch cease to be the default authoring/intake path; import/export remains | Retirement acceptance below passes; the legacy YAML mapper defaults off in new installs and the import tool remains |

P4 requires review of the P0/P1 IR→Task mapping table against existing YAML→Task semantics. P6 requires
the full retirement evidence and documented kill-switch below.

### 6. Dual-read, coexistence, and rollback

| Stage | Intake/write behavior |
| --- | --- |
| Dual-read | Loader accepts applied IR or preset bundle; both use the shared semantic core; Canvas may retain a YAML projection for legacy bundles |
| IR-primary write | Generate/Compose and new edits write IR with revision CAS; YAML export is derived |
| IR-primary execute | Production admission prefers applied IR identity; YAML-only bundles import on load; remaining YAML-only usage is observable |
| Retire hand-authored main path | YAML mapper defaults off for that path; partner/integrator import/export remains |

The dual-read window allows emergency re-enable of YAML intake by configuration. YAML-only bundles and mixed workspaces remain supported until P6. YAML and IR never become independently evolving grammars: `nexus-preset` becomes the import/compat loader, retains `PresetSourceIdentity` hash semantics, and shares semantic validation with IR.

Rollback of a bad Apply installs the previous approved snapshot through a new Apply; it never rewrites a hash silently. Execute continues refusing missing or changed source bytes. **Both shipped in v1.241:** rollback is an append naming a superseded approval, and intake refuses bytes that no longer hash to the recorded identity (`applied_document_changed`) or cannot be read at all (`applied_document_unverifiable`). Pattern and asset hashes recorded at Approve participate in source freeze.

### 7. Retirement scope

P3.5–P6 add, and P4–P6 replace:

- Hand-authored `preset.yaml` as the primary authoring path.
- TypeScript `parsePresetYaml` + `buildStrategyGraph` as the sole Canvas source.
- `strategy.patch_*` YAML mutators as the sole write path.
- YAML→Graph as the sole production intake.
- Copy-sample-directory reuse and explainability-only Canvas framing.

The retained pieces are graph-flow, `OrchestrationEngine`, SessionStorage, `WaitingForInput` / `wait_id`, capability registry, source identity freeze, integrator versus creator-host execution surfaces, and documented `preset.yaml` import/export. Outline, Timeline, World KB Canvas, Actor/Viewpoint preset IR, Work-tree artifacts, and a first-party player app are not replaced.

The Canvas transition removes YAML-only projection from the hot path only after dual-read ends. Developer explainability is expected to become a read-only view of the applied document rather than YAML-first editing; no such surface ships today, and naming it here is a direction, not a claim about the codebase.

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

## Acceptance criteria

P3, shipped in v1.239, required a staged creator compose→simulate→approve flow using two patterns. P4 exit requires existing YAML-preset schedules to run unchanged while applied IR loads through the reviewed IR→Task mapping. P5 exit requires new samples and strategies to land as IR with YAML only as compatibility export.

P6 retirement requires all of the following on a reference workspace:

1. Samples and embedded creator strategies run from applied IR with YAML intake off.
2. The primary Strategy Canvas route calls `parsePresetYaml` only inside the legacy import wizard.
3. No CI job requires hand-maintained `preset.yaml` for new strategies; generated export may remain.
4. Compose and Generate candidates pass validate, simulate, approve, and apply.
5. Recovery refuses mismatches in IR content hash and recorded pattern/asset pins.
6. A documented kill-switch re-enables YAML intake for one minor release after default-off; import/export remains afterward.
7. Existing partner/integrator bundles have a deprecation note and import path that converts them into candidates passing validation and simulation.

P6 never cuts the public bundle format without that compatibility path.

## Risks

1. **Encoding and identity:** JSON, CBOR, or YAML-IR trade human diff friendliness against canonical hash stability. The implemented document-model record owns the schema/identity baseline; this proposal must not create another encoding authority.
2. **Gate placement:** enqueue-time gates remain schedule-owned even when IR has metadata carriers; `cli_args` remains in `meta.cli_args`.
3. **Nest policy:** the hard depth limit and the default between `expand_in_candidate` and `defer_until_apply` still need to be exercised without permitting cycles or hidden effects.
4. **Generator home:** builtin capability, Develop-hub tool, or CLI-first operation must not fork candidate state or bypass the fence.
5. **Simulation fidelity:** P2 answers static reachability only; Canvas must preserve its disclosures rather than presenting it as output preview. The current contract is linked to the implemented simulation record.
6. **Approval trust:** creator versus developer policy for third-party patterns, including trust labels or signatures, must not create a weaker publish or Approve channel.
7. **Compatibility artifacts:** whether dual-read YAML exports are Git-tracked generated output or local install artifacts must be explicit without making them a second authoring SSOT.
8. **Partial regeneration:** local Agent regeneration of nested regions must merge by nest id into one candidate rather than create shadow graphs.
9. **External effects:** simulation must never promise exactly-once effects or commit write/external capabilities.
10. **Cutover availability:** YAML-only schedules and partner bundles must survive P4/P6 through dual-read, kill-switch, and import/export rather than a breaking default change.

# ADR: Canvas Composition and Pattern Packages — Strategy Orchestration Cutover

Status: proposed — P3.5 Apply and P4–P6 not implemented; P3 shipped in v1.239 (#515) and is governed by the composition spec, not by this record
Date: 2026-11-10
Scope: The still-unshipped part of the cutover — Apply/Execute from a candidate, the dual-read loader, IR-primary authoring, and retirement of the hand-authored YAML main path while retaining `preset.yaml` import/export. The Canvas composition surface, the compose operations, the pattern packages and the candidate lifecycle **up to Approve** shipped in v1.239 and are owned by the composition spec.
Source: split from the archived Canvas + Workflow IR redesign and Dynamic Workflow Generation and Orchestration Simulation records

## Problem

P0/P1 shipped the [Workflow IR document model](../../implemented/architecture/2026-11-10-workflow-ir-document-model.md),
P2 shipped the [strategy simulation fence](../../implemented/architecture/2026-11-10-strategy-simulation-fence.md),
and P3 shipped composition and approval in v1.239. Canvas composes, packages are published with pinned
identity, and a candidate can be simulated and approved. What the cutover still lacks:

1. **Approved identity is persisted but never applied.** Approve writes an immutable snapshot carrying the
   document, its content hash and its pins; nothing turns that snapshot into the identity production intake
   reads. Apply is the missing stage, and it is what makes "applied IR" mean something.
2. **Production intake is still YAML-only.** A controlled dual-read transition has to exist before
   IR-primary execution can replace the YAML-only mapper. It can begin on imported preset bundles — today's
   intake — but loading an *applied* IR identity waits on (1).
3. **New strategy authoring, compatibility export, and retirement of the hand-authored YAML main path**
   need explicit exit criteria so the two grammars do not evolve independently.
4. **The candidate lifecycle stops at Approve.** Review needs compose, simulation, approval *and*
   application to remain one lifecycle over one IR truth; today the last step is missing from the surface,
   not merely unpolished.

## Proposal

### 1–4. Shipped surface — read the spec, not this record

The composition surface, the nest/compose operations, the pattern-package contract and the candidate
lifecycle **up to Approve** shipped in v1.239 (#515). **Canvas Apply and Execute did not**, and P6's fourth
acceptance item ("compose and generate candidates pass validate, simulate, approve, and apply") together
with §6's rollback-by-new-Apply both assume a stage that does not exist yet.

Apply is therefore its own stage — **P3.5 Apply** below — and it precedes P4 in practice: dual-read can
begin by loading imported preset bundles (today's intake) to prove the IR→Graph mapping, but it cannot load
an *applied* IR identity until Apply produces one. The plan that takes P4 owns placing Apply first.

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

### 5. P3.5–P6 ladder

P0/P1/P2 shipped and are governed by their implemented records. P3 shipped in v1.239 and is governed by
the composition spec. What remains:

| Phase | Change | Exit criteria |
| --- | --- | --- |
| **P3.5 Apply/Execute** | Turn an approved snapshot into the identity production intake reads; Execute continues to run graph-flow | An approved candidate is applied and an existing schedule runs from that applied identity; a rollback installs the previous snapshot through a new Apply |
| **P4 Dual-read loader** | Engine loads applied IR or a preset bundle | Existing YAML-preset schedules remain unbroken; IR→Graph mapping matches the IR→Task table in orchestration-engine §8.2.1 |
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

Rollback of a bad Apply installs the previous approved snapshot through a new Apply; it never rewrites a hash silently. Execute continues refusing missing or changed source bytes. Pattern and asset hashes recorded at Approve participate in source freeze.

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

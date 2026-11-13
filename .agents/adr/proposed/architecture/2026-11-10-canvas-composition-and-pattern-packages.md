# ADR: Canvas Composition and Pattern Packages — Strategy Orchestration Cutover

Status: proposed — P3 shipped in v1.239 (#515, `34d348fe2`) and is governed by
[docs/specs/orchestration/strategy-composition.md](../../../../docs/specs/orchestration/strategy-composition.md); P4–P6 not implemented
Date: 2026-11-10
Scope: The unshipped Canvas composition surface, nest/compose operations, pattern packages with pinned content hashes, candidate lifecycle UX, dual-read loader, IR-primary authoring, and retirement of the hand-authored YAML main path while retaining `preset.yaml` import/export.
Source: split from the archived Canvas + Workflow IR redesign and Dynamic Workflow Generation and Orchestration Simulation records

## Problem

P0/P1 shipped the [Workflow IR document model](../../implemented/architecture/2026-11-10-workflow-ir-document-model.md), and P2 shipped the [strategy simulation fence](../../implemented/architecture/2026-11-10-strategy-simulation-fence.md). They do not make Canvas a composition surface, publish reusable pattern packages, load applied IR in production, or change the primary authoring path.

The remaining problems are:

1. Canvas still risks being an explainability-only YAML projection with node-granular patches rather than a surface for composing patterns and Agent drafts.
2. Reuse by copying a sample directory and renaming `preset.id` provides no versioned, pinned package identity.
3. Production intake still needs a controlled dual-read transition before IR-primary execution can replace the YAML-only mapper.
4. New strategy authoring, compatibility export, and retirement of the hand-authored YAML main path need explicit exit criteria so the two grammars do not evolve independently.
5. Candidate review needs compose, simulation, approval, and application to remain one lifecycle over one IR truth.

## Proposal

### 1. Canvas as a composition surface

Strategy Canvas remains a `CanvasSurfaceKind = 'strategy'` peer under the shared Canvas Shell, but its adapter reads IR DTOs through `projectIrGraph` rather than treating parsed YAML and `buildStrategyGraph` as the sole source. Rust may supply canonical layout hints. Composition and HITL are the duty; explainability is the applied-IR inspector view.

| Concern | Proposed target |
| --- | --- |
| Graph source | IR DTO projection, with YAML confined to compatibility import |
| Primary operations | Nest pattern, unnest, replace subtree, bind slot, generate-into-region, simulate, approve |
| Advanced operations | IR node/edge, exit, template, and convergence patches with revision CAS |
| Overlay | Session status plus candidate simulation trajectory |
| Alternate view | Candidate outline with nests, pins, effect summary, and diagnostics |

Canvas is not the only entry: CLI, Agent, and programmatic Compose remain valid. Users need neither drag every edge from scratch nor accept a read-only Agent dump. Generate and Compose both open the same candidate type and feed Validate → Simulate → Approve → Apply:

- **Generate:** intent goes through Host/ACP and returns a `WorkflowCandidate` in review/compose mode.
- **Compose:** the user starts blank, from the pattern library, or from an imported preset-as-IR.
- Generate/Compose alone never opens a driven production session.

### 2. Nest and compose operations

Creator-tier operations are:

- Add a library pattern as a collapsed `PatternSlot`.
- Nest a selected subgraph into a `NestGroup`; extract a selection into a pattern draft through the developer channel.
- Bind slot parameters in an inspector.
- Expand/collapse and drill into nested flows using the existing React Flow parent/group behavior.
- Replace a nested region through scoped Generate, merged by nest id.

Developer/power operations rewire edges, edit exit conditions and templates, and change convergence as CAS-guarded IR patches. Local or partial Agent regeneration merges into the single candidate IR; it never forks an unreconciled shadow candidate.

Nesting rejects cycles and has a bounded depth policy. Child effect classes roll into the parent by worst-case lattice join. Pattern expansion at Apply or a defined pre-Apply step is deterministic for the recorded pin set and produces the same outer/inner graph model used by graph-flow. Nesting never hides `write_candidate` or `external` effects.

### 3. Pattern packages with pinned hashes

A package contains a manifest, an IR fragment, optional templates, and non-normative human documentation:

```text
my-pattern/
  pattern.toml          # or pattern.yaml
  workflow.ir.json      # or .cbor
  templates/
  README.md
```

| Manifest field | Rule |
| --- | --- |
| `id` | Registry id `[a-z][a-z0-9._-]*` |
| `version` | Semver |
| `ir_compat` | Minimum/maximum supported `ir_version` |
| `description` | Human label |
| `effect_class` | Matches validator rollup of the fragment |
| `slots` | Named string, asset, and capability bind parameters |
| `requires_capabilities` | Unioned into the parent on instantiate |

Packages are independently versioned artifacts. A filesystem registry under `~/.nexus42/patterns/` with content hashes is sufficient for the first form; a crate/registry service is not required by this proposal.

Compose defaults to an exact id/version pin. Approve locks every resolved pattern and asset to a recorded content hash, including exact pins and any later range resolution. Apply verifies those hashes and fails closed if an object changed under the same version. Hot update never mutates frozen source identity; it requires a new approved snapshot and Apply. Applied provenance records the pins and, for migrated strategies, the imported preset id.

Developers author and publish patterns as a first-class channel. Content creators may publish only through a policy-bound, developer-approved channel after the same validation and simulation gates; there is no weaker second publish path.

### 4. Candidate lifecycle and API boundaries

```text
Generate | Compose → Validate → Simulate → Approve → Apply → Execute
```

| Stage | TypeScript responsibility | Rust responsibility | Proposed route |
| --- | --- | --- | --- |
| Generate | Intent and progress | Generator emits candidate IR | `POST /v1/daemon/strategies/generate` |
| Compose | Nest/combine and scoped regeneration | IR patch/merge and pattern resolution | `POST /v1/daemon/strategies/{id}/compose` |
| Validate | Diagnostics | Shared semantic and nest-policy facade | `POST /v1/daemon/strategies/{id}/validate` |
| Simulate | Trajectory overlay | Isolated walk and effect rollup | `POST /v1/daemon/strategies/{id}/simulate` |
| Approve | Explicit confirmation | Immutable snapshot, pin lock, recorded hashes | `POST /v1/daemon/strategies/{id}/approve` |
| Apply | Success/conflict UX | Applied identity, optional YAML export, schedule preparation | `POST /v1/daemon/strategies/{id}/apply` |
| Execute | Live overlay and waits | Existing graph-flow engine and wait CAS | Existing schedules/sessions routes |

The internal resource remains `/strategies`, with existing `preset` wire field names; renaming it to `/workflows` is a separate product decision. Routes follow [daemon-api-surface-conventions.md](../../../../docs/specs/runtime/daemon-api-surface-conventions.md), not colon-action forms.

Approve shows effect summary, recorded pin/asset hashes, validation, and simulation for the current document hash. Patches after Validate/Simulate rerun both and bind approval to the new hash. Approve never renders as an enabled no-op. Apply verifies the frozen identity before schedule binding. Auto-apply is forbidden, and run-time `WaitingForInput` remains a separate continuation gate.

Permission layering is:

| Role | Powers |
| --- | --- |
| Content creator | Generate, Compose with published patterns, nest/unnest, bind slots, simulate, policy-bound approve, apply to own schedules; publish only through the approved channel |
| Developer | Creator powers plus advanced edits, pattern author/publish, legacy import, and raw IR view |
| System | Embedded/system workflows remain read-only outside controlled release channels |

### 5. P3–P6 ladder

P0/P1/P2 are shipped and are governed by their implemented records. **P3 also shipped**, in v1.239
(#515): pattern packages with content pins, the four compose operations with candidate revision CAS, the
approval freeze, and the flagged Canvas compose surface. Its current contract lives in
[docs/specs/orchestration/strategy-composition.md](../../../../docs/specs/orchestration/strategy-composition.md);
this record keeps only the unshipped ladder.

| Phase | Change | Exit criteria |
| --- | --- | --- |
| ~~**P3 Canvas compose MVP**~~ | **Shipped in v1.239** — see the composition spec | Met: a creator composes two patterns into a candidate, simulates, and approves in staging |
| **P4 Dual-read loader** | Engine loads applied IR or a preset bundle | Existing YAML-preset schedules remain unbroken; IR→Graph mapping matches orchestration-engine §8.2 |
| **P5 IR-primary authoring** | Documentation and samples are authored as IR; YAML export remains compatibility output | New repository strategies land as IR |
| **P6 Retire hand-authored YAML main path** | Hand-edit and YAML patch cease to be the default authoring/intake path; import/export remains | Retirement acceptance below passes; the legacy YAML mapper defaults off in new installs and the import tool remains |

Flagged P3 UI may start after P1, but default-on and P3 exit require the P2 trajectory. P4 requires review of the P0/P1 IR→Task mapping table against existing YAML→Task semantics. P6 requires the full retirement evidence and documented kill-switch below.

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

P3–P6 replace:

- Hand-authored `preset.yaml` as the primary authoring path.
- TypeScript `parsePresetYaml` + `buildStrategyGraph` as the sole Canvas source.
- `strategy.patch_*` YAML mutators as the sole write path.
- YAML→Graph as the sole production intake.
- Copy-sample-directory reuse and explainability-only Canvas framing.

The retained pieces are graph-flow, `OrchestrationEngine`, SessionStorage, `WaitingForInput` / `wait_id`, capability registry, source identity freeze, integrator versus creator-host execution surfaces, and documented `preset.yaml` import/export. Outline, Timeline, World KB Canvas, Actor/Viewpoint preset IR, Work-tree artifacts, and a first-party player app are not replaced.

The Canvas transition removes YAML-only projection from the hot path only after dual-read ends. Developer explainability moves to a read-only applied-IR inspector rather than YAML-first editing.

## Alternatives considered

- **Keep Canvas explainability-only and extend YAML patches indefinitely.** Rejected: the human remains a YAML surgeon and cannot compose patterns or drafts as first-class IR units.
- **Make Canvas the only authoring entry or require drag-from-scratch.** Rejected: CLI, Agent, programmatic Compose, and Generate remain valid paths to the same candidate.
- **Make Canvas a read-only Agent-output viewer.** Rejected: it discards the composition duty.
- **Copy sample directories as the reuse mechanism.** Rejected: independently versioned packages with exact content pins provide reusable identity and fail-closed updates.
- **Allow hot update under an unchanged approved version.** Rejected: source identity would change silently; a new Approve/Apply is required.
- **Delete `preset.yaml` import/export at P6.** Rejected: partner/integrator bundles remain a public compatibility format; only the hand-authored main path retires.
- **Maintain YAML and IR as independent grammars.** Rejected: YAML becomes import/projection through the shared semantic core.
- **Big-bang intake replacement.** Rejected: P3–P6 preserve existing schedules, emergency YAML re-enable, mixed workspaces, and previous-snapshot rollback.
- **Replace graph-flow, add a first-party player, or absorb Work Artifact IR.** Rejected as separate or explicitly excluded responsibilities.

## Acceptance criteria

P3 exit requires a staged creator compose→simulate→approve flow using two patterns. P4 exit requires existing YAML-preset schedules to run unchanged while applied IR loads through the reviewed IR→Task mapping. P5 exit requires new samples and strategies to land as IR with YAML only as compatibility export.

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

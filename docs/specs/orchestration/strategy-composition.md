# Strategy composition — pattern packages, compose ops, approval and apply

## Scope

The wire contract for pattern packages, the four compose operations, what an approval freezes, and how
an approval becomes the running identity. The simulator contract is separate:
[strategy-simulation.md](strategy-simulation.md). The IR → Task mapping an applied identity executes
through is [orchestration-engine.md §8.2.1](orchestration-engine.md).

## Current state

### 1. Pattern packages

A pattern is a directory under the local registry (`~/.nexus42/patterns/<id>/` by default) holding
`pattern.toml` and `workflow.ir.json`. The manifest declares `id` (`[a-z][a-z0-9._-]*`, equal to the
directory name), SemVer `version`, an inclusive `ir_compat` range, a human `description`, `effect_class`,
`slots`, and `requires_capabilities`. Unknown fields are rejected.

**The pin is the canonical content hash of the IR fragment, never the manifest and never the version
string.** Two packages may declare `1.0.0` with different bytes; the version is a label, the hash is the
identity. A declared `effect_class` that disagrees with the validator's rollup of the fragment makes the
package unavailable rather than silently preferred — neither value wins.

`GET /v1/daemon/patterns` lists packages with cursor pagination bounded to 1–200, ordered by UTF-8 byte
order, each entry carrying `available` and, when false, a `reason`. A malformed package never removes its
peers from the listing: the reader isolates per-package failures, and a directory name that is not UTF-8
carries an escaped identity so two such entries stay distinct. **Availability means readable and
manifest-valid, not structurally valid** — structural validation is composition's job, and a per-fragment
validate would emit spurious nest violations for fragments meant to be nested.

### 2. The candidate

A strategy's draft lives in one `strategy_candidates` row: `strategy_id` is the primary key, `document_json`
holds the complete `WorkflowDocument` with `state: candidate` and `id == strategy_id`, and `revision` is the
**single compare-and-swap authority** — the embedded `document.revision` mirrors the row in the same
transaction. A strategy with no row projects the preset import at revision `0` and persists nothing.

`GET /v1/daemon/strategies/{id}/candidate` returns the row when present, the revision-`0` projection
otherwise. It never creates a row.

### 3. Compose operations

`POST /v1/daemon/strategies/{id}/compose` carries `{base_revision, op, payload}` with `op` one of `nest`,
`unnest`, `replace_subtree`, `bind_slot`.

- **`nest`** inserts a collapsed `pattern_slot` carrying the resolved `{id, version, content_hash}` pin.
  An optional `attach_to` writes the edge from that target to the slot; a target naming no node is refused
  `unknown_attach_target` and a target equal to the slot is refused `self_attach`, both before any write.
  A slot with no inbound edge is unreachable to the static walk and therefore unnamed by any trajectory.
- **`unnest`** flattens a slot's children into the parent level; it refuses `nest_unflattenable` rather than
  dropping a body's `roles`/`assets`/`signals` or the removed slot's own `depends_on`/`on_timeout`.
  Slot bindings are dropped and reported as `slot_binding_dropped`.
- **`replace_subtree`** re-pins a region in place and is the only operation allowed to establish a new pin.
- **`bind_slot`** validates the value's kind against the pinned manifest's slot declaration.

Depth and cycle rules come from `nexus-workflow-ir` (`MAX_NEST_DEPTH = 3`), and resolved pattern fragments
participate in that walk — a compose-local depth constant or cycle algorithm is a defect. Effects join by
worst case over `pure < read < write_candidate < external`, so nesting can never lower a parent's class or
hide `write_candidate`/`external`.

**Recorded pins are verified before use.** An operation resolves the pins it consumes and refuses when a
package's scanned hash differs from the recorded one (`pin_changed`) or the package is gone
(`pin_unavailable`); remaining stale pins surface as diagnostics rather than blocking unrelated operations,
so two stale regions cannot refuse each other's repair.

A stale `base_revision` returns 409 `strategy_candidate_conflict` with `{strategy_id, base_revision,
current_revision, conflicting_path, recovery_hint}` **before any write**. The client reloads the canonical
candidate and lets the user reapply; it never retries automatically.

### 4. What approval freezes

`POST /v1/daemon/strategies/{id}/approve` writes one immutable snapshot: the document, its canonical
content hash, the candidate revision, the exact simulation trajectory it was approved against, every
resolved pin `{id, version, content_hash}`, the approver and the timestamp.

Refusals: `simulation_missing`, `simulation_stale` (recorded trajectory hash ≠ current document),
`simulation_mismatch`, `pin_changed`, `pin_unavailable`, and 409 `strategy_already_approved` for the same
content (enforced by `UNIQUE(strategy_id, content_hash)`). The revision check and the insert are one
statement, so a compose landing in the window cannot leave a snapshot attributing the wrong revision.

Approve freezes identity; it does not apply, bind a schedule or execute. **Approve never renders as an
enabled no-op**: every surface that offers it shows what the current document hash's simulation level, the
stubbed judges, the effect fence and the pins are, or it renders disabled with a reason.

### 5. Apply — the running identity

`POST /v1/daemon/strategies/{id}/apply` carries `{content_hash}` and installs that approved snapshot as the
identity production intake reads. The body names the approval by content hash only; the id is derived from
the rows, never supplied.

**Every refusal happens before a row exists**, and each is a safety property rather than an input check:
`approval_not_found` (no approval carries that hash), `approval_strategy_mismatch` (the hash belongs to
another strategy, so no row could pair it with this one), `approval_hash_changed` (the frozen document no
longer hashes to the identity the approval recorded — installing it would install a changed hash),
`strategy_not_user_preset` (the intake only consults an apply for a user bundle, so an apply on any other
source records a row nothing reads), `document_unbuildable` (the approved document cannot become an outer
graph, so applying it would install an identity intake refuses to build — the payload carries the builder's
own reason), `strategy_has_no_schedule` (nothing would run) and `already_applied` (the named approval is
already current). `already_applied` and `approval_hash_changed` return 409; the rest are refusals on the
same envelope. Buildability is checked before both the schedule guard and the already-current guard, so two
orderings follow: a strategy that is both unscheduled and unbuildable reports `document_unbuildable` (the fix
is a re-approval, not a schedule), and a repeat apply of an approval that is already current but whose
document no longer builds reports `document_unbuildable` rather than 409 `already_applied` (the identity is
not usable, so idempotent success would be a lie; this is the one status change a client treating 409 as
success sees).

**The table is append-only and carries no current pointer.** The current apply of a strategy is the row with
no successor; a superseded approval is a legal rollback, expressed as a new apply naming it. There is no
separate rollback code path and nothing ever updates or deletes an apply row. The already-current guard, the
supersede link and the insert are **one statement** — it reads the current row through its own subqueries in
the same statement that writes, so two concurrent applies cannot both believe they are first and no writer
supersedes a row it did not see.

`GET /v1/daemon/strategies/{id}/applied` answers the Canvas' and the intake's question from those rows: the
current apply and the highest-`candidate_revision` approval, each null when absent. One total order backs it
(`candidate_revision` DESC, `approved_at` DESC, `approval_id` DESC).

`GET /v1/daemon/strategies/{id}/approvals` lists every approval newest-first. **This read is what makes
rollback reachable**: the status read reports only the newest approval, so without it a client cannot name a
superseded one. Its order is the status read's, so the first item is exactly `newest_approval`. Applying a
listed row appends an apply and leaves the list untouched.

#### 5.1 What intake does with an applied identity

**Anchors.** Every code reference below names the file and the item that decides the claim — function,
match arm, struct field or table row — and that name is authoritative; a `:line` only locates the item in
the current tree, so formatting may move it while the claim stays with the name.

**One rule, four admission entries.** Schedule admission (`WorkflowRunCoordinator::admit_schedule_inner`,
`crates/nexus-core/src/execution/workflow.rs`), `WorkflowRunCoordinator::start_session`
(`workflow.rs`) and both preset trait entries — `start_session_with_preset` and
`start_session_with_preset_for_creator`, which share `start_preset_entry`
(`crates/nexus-orchestration/src/engine.rs`) — ask the same question before they build: which document does
this strategy run? All four answer it through `resolve_applied_document`
(`crates/nexus-core/src/execution/applied_documents.rs`), the one route through `is_user_preset`,
`current_apply` and `verified_apply_document` (`crates/nexus-core/src/apply.rs`). The two trait entries
reach that route through the installed `AppliedDocumentSource` (`start_preset_entry` in `engine.rs`; the
daemon installs the Creator-backed source in `crates/nexus-core/src/execution/lifecycle.rs`).

**The rule.** When the answer is a document, the entry builds the graph from that document with
`IrTemplateSource::OwnSource` and freezes the `AppliedRunIdentity` the answer carries on the run descriptor
it writes; when the answer is `None`, the intake mode decides what happens next — under `yaml-only` it
builds the YAML bundle (`build_wired_outer_graph`) and freezes no identity, and under the default `ir-only`
a user bundle with no current apply is refused before graph construction instead (the intake-mode switch
below). For a run admitted with a resolved document, no entry admits YAML and recovers into IR: the graph
the run executes and the identity its descriptor freezes come from the same resolution. A run admitted
under `yaml-only` while the strategy had no current apply resolves `None`, freezes no identity, and
recovers through the `(None, current)` arm below — the one case in which a restart can still put a run on a
document it never ran, and it exists because the run was admitted under none; under the default `ir-only`
that admission is the refusal instead, so no such run exists. Both admission entries resolve before the
binding gate, so the roles validated are the roles the applied graph consumes — `export_preset(&document)`
when an apply exists, the loaded manifest otherwise (`export_preset` and `validate_agent_bindings` in both
`start_session` and `admit_schedule_inner`).

| Entry | Resolves through | Applied graph | Descriptor write |
| --- | --- | --- | --- |
| schedule admission | `resolve_applied_document` (`workflow.rs:2980`) | `build_wired_outer_graph_from_ir(.., OwnSource)` (`workflow.rs:3020`) | `admit_schedule_run_with_input` sets `descriptor.applied` (`engine.rs:4541`) |
| `start_session` | `resolve_applied_document` (`workflow.rs:1605`) | `build_wired_outer_graph_from_ir(.., OwnSource)` (`workflow.rs:1653`) | `start_preset_run_with_input` sets `descriptor.applied` (`engine.rs:4428`) |
| `start_session_with_preset` | `AppliedDocumentSource::applied_document` (`engine.rs:4347`) | `build_wired_outer_graph_from_ir(.., OwnSource)` (`engine.rs:4360`) | `start_preset_run` sets `descriptor.applied` (`engine.rs:4283`) |
| `start_session_with_preset_for_creator` | `AppliedDocumentSource::applied_document` (`engine.rs:4347`) | `build_wired_outer_graph_from_ir(.., OwnSource)` (`engine.rs:4360`) | `start_preset_run` sets `descriptor.applied` (`engine.rs:4283`) |

**Precedence.** The document a run is built from follows from the loader's classification and that apply
read, in exactly three cases:

| Strategy the loader resolved | Document the run is built from |
| --- | --- |
| a user bundle with a current apply, under `ir-only` (the default) or `yaml-only` | under `ir-only`: the applied `WorkflowDocument`, through the [§8.2.1 mapping](orchestration-engine.md#821-ir--task-mapping); under `yaml-only`: the YAML descriptor — the apply is never read |
| an embedded or `_system.` preset | the YAML descriptor — the apply branch is never entered and the intake key is never consulted |
| a user bundle with no current apply | `ir-only` (the default): refused before graph construction — `yaml_intake_disabled`, never a YAML fallback; `yaml-only`: the YAML descriptor of [§8.1](orchestration-engine.md) |

The apply branch is entered only for a user bundle: `is_user_preset` requires the source identity the
loader recorded to be a directory bundle and refuses every `_system.` id, so an embedded or system preset
never names a strategy and is never looked up. The discriminator is that loader classification, never a
preset lookup — the embedded-first lookup would hand an applied user bundle the embedded preset's graph
when the ids collide. A user bundle whose apply rows have no successor-less row is on the YAML path under `yaml-only`:
`current_apply` returning `None` is an answer, not a refusal. Under the default `ir-only` it is the
refusal — see the intake-mode switch below.

**Recovery** re-enters through the same route instead of a second reader (`AppliedDocumentSource` →
`is_user_preset` → `current_apply` → `verified_apply_document`), and a strategy with no apply recovers its
descriptor graph exactly as before — for legacy rows. A row that records an intake mode follows the
recorded mode instead (see below).

**The intake-mode switch.** `[strategy_intake] mode = "ir-only" | "yaml-only"` in the home `config.toml`
selects which document a **user bundle** runs; `strategy_intake_mode`
(`crates/nexus-home-layout/src/strategy_intake.rs`) reads it per new admission. A missing file, table or
key defaults to `ir-only`; an unreadable document or an unknown mode value is a typed refusal,
`RunControlError::InvalidIntakeConfig` at the coordinator entries and `intake_config_invalid` at the engine
entries — a mistyped key fails closed instead of silently selecting an arm. Embedded and `_system.` presets
never consult the key: the classification is `is_user_strategy_bundle`
(`strategy_intake.rs`), the same decision `is_user_preset` (`crates/nexus-core/src/apply.rs`) delegates to,
so both sides of the gate agree.

- **New admissions** (`intake_mode_for_admission` in `crates/nexus-core/src/execution/applied_documents.rs`
  gates `admit_schedule_inner` and `start_session`; the same gate inlined in `start_preset_entry` in
  `crates/nexus-orchestration/src/engine.rs` covers both preset trait entries): under `ir-only` a current
  apply runs through `resolve_applied_document` and a user bundle with none refuses with
  `RunControlError::YamlIntakeDisabled` (`yaml_intake_disabled` at the engine entries) before graph
  construction — never a YAML fallback. Under `yaml-only` the YAML manifest and graph run without reading
  the current apply.
- **The descriptor is the observability record.** `RunDescriptorV1.intake_mode`
  (`crates/nexus-orchestration/src/run_state.rs`) freezes `Some(IrOnly)` or `Some(YamlOnly)` on new
  user-preset runs and is omitted — byte for byte — on embedded/system and legacy rows. Child descriptors
  inherit the root's value; the auto-chain and raw-construction sites record none.
- **Recovery follows the recorded mode, not the current config** (the `descriptor.intake_mode` match in
  `reconstruct_runner`, `engine.rs`): `Some(YamlOnly)` skips `applied_document()` and rebuilds the YAML
  bundle even if the config flipped or the strategy gained an apply; `Some(IrOnly)` resolves and checks the
  recorded identity as the `Some(recorded)` rows always have, and a `Some(IrOnly)` row without a recorded
  identity refuses (`yaml_intake_disabled`, non-replayable) instead of rebuilding from YAML; `None` retains
  the legacy `(None, current)` arm below. The current config affects new admissions only.
- **Lifetime.** `yaml-only` is the emergency compatibility arm for the v1.243 minor line and is removed at
  v1.244.0. Bundle discovery and import/export have no removal boundary: the switch changes which document
  executes, never whether a bundle exists or can be imported or exported.

**The legacy `(None, current)` arm.** `reconstruct_runner` (`engine.rs:3842`) matches the descriptor's
recorded identity against that resolution **before** it builds (`engine.rs:4109`):

| `descriptor.applied` | Resolved current apply | Graph rebuilt from |
| --- | --- | --- |
| `None` | any | the resolved applied document when there is one, else the YAML bundle — the `(None, current)` arm |
| `Some(recorded)` | identity equal | the applied document — the `current.identity == *recorded` guard |
| `Some(recorded)` | a different apply, or none | typed refusal `reconstruction_unavailable` — the `(Some(recorded), current)` fallback |

The first row is the pre-freeze arm: a descriptor that records no identity rebuilds from whatever the
strategy's current apply is, exactly as it did before the identity existed. That is what lets a no-apply
run reconstruct, and it is also the last way a restart can switch a run onto a document it never ran, so
its reachability is bounded rather than assumed:

- **A run admitted with a current apply cannot reach it.** All four entries freeze the identity they
  resolved, so the descriptor is `Some(recorded)` and recovery compares: the same apply rebuilds the same
  document, any other apply or no current apply refuses. The trait entries resolve through the installed
  `AppliedDocumentSource`; an engine without one resolves no apply and freezes no identity, which is the
  row-1 case below.
- **A run admitted while its strategy had no current apply records no identity by definition**, and an
  apply installed before that run restarts reinterprets its persisted position through row 1. The freeze
  closes the runs admitted under an apply; it cannot close the absence of one.
- **A descriptor written outside those four entries records no identity either.** Those constructions are
  the sites below, and the verdict on each is what bounds this arm.

Both sides re-run the same recomputation before building: an apply whose `document_json` no longer hashes
to its recorded `content_hash` is `applied_document_changed`, and one that cannot be read is
`applied_document_unverifiable`. Bytes edited after apply are therefore a typed refusal, not a
reinterpreted run.

**The I-000467 reclassification.** Every site the ticket named, plus `build_descriptor`, is decided by the
named item; the `:line` locates that item in the current tree.

| Site | Verdict |
| --- | --- |
| `crates/nexus-core/src/execution/handle_ops.rs:1450` `freeze_descriptor` (`applied: None`, `:1564`) | **Inert** — this is a schedule admission *payload*, not a run descriptor: `admit_schedule_inner` re-reads only `work_id`, `input`, `agent_bindings` and `source` from it (`workflow.rs:2936-2957`) and resolves the apply itself (`workflow.rs:2980`) |
| `crates/nexus-core/src/execution/workflow.rs:2514` `prepare_legacy_row` (`applied: None`, `:2615`) | **Inert** — the same payload write, read back by the same four fields |
| `crates/nexus-orchestration/src/auto_chain.rs:1383` `enqueue_descriptor_json` (`applied: None`, `:1420`) | **Unreachable with a user apply** — it requires `embedded_source_identity` (`:1399`), so the source is never `Directory` and `is_user_preset` is false |
| `crates/nexus-orchestration/src/storage/sqlite.rs:473` `child_descriptor` (`applied`, `:489`) | **Already correct** — `applied: root.applied.clone()`; a child run inherits the root's identity |
| `crates/nexus-orchestration/src/engine.rs:4642` `start_session_with_creator` (`applied: None`, `:4678`) | **Callable, no known production caller** — `GraphFlowEngine::start_session` forwards here unconditionally (`:4638`) and `OrchestrationEngine::start_session_with_graph` (`:4850`) exposes that route, so a store-backed engine started this way writes `applied: None`; no in-repo production caller holds a store (the callers are `start_session` and the two `workflow_store == None` fallbacks, `:4323` and `:4481`) |
| `crates/nexus-orchestration/src/engine.rs:4799` `new_session` (`applied: None`, `:4819`) | **Callable, no known production caller** — the `GraphFlowEngine` trait implementation is public and its store arm writes `applied: None`; only `EngineProxy::new_session` refuses (`:3262`, `NoGraphLoaded`), and nothing in this tree calls the other one in production |
| `crates/nexus-orchestration/src/engine.rs:4241` `build_descriptor` (`applied: None`, `:4266`) | **Fixed** — the shared skeleton starts empty, and all three callers overwrite it with the identity they resolved before persisting: `start_preset_run` (`:4283`), `start_preset_run_with_input` (`:4428`), `admit_schedule_run_with_input` (`:4541`) |

**Known limits on the applied path.** Each bounds what an applied run can do; none of them turns the
applied document back into a hint.

- **Every applied prompt runs under the `default` role.** The prompt capability sends `agent_ref: None`, so
  a prompt role declared only in the applied document passes admission and never drives a prompt (I-000462).
- **`required_prompt_roles` scans `llm_judge` exits and inner-graph `acp_prompt` nodes only.** A document
  that prompts through a capability enter action contributes no required role, so a run can be admitted
  without the `default` binding and fail at the prompt instead of at admission (I-000463).
- **A run whose descriptor records no applied identity still resolves against the current apply.** Rows
  persisted before the descriptor carried one, runs whose descriptor is built outside the four admission
  entries, and runs admitted while the strategy had no current apply hold no identity, so an apply installed
  before such a run restarts reinterprets that run's persisted position. The freeze closes only the runs
  admitted under an apply; the out-of-admission construction sites are the reclassification table above,
  where each is inert, unreachable with a user apply, already inheriting the root identity, or a raw
  constructor with no known production caller whose store-backed entry stays callable.
- **Reconstruction validates inner descendants against the descriptor manifest** while it builds the graph
  from the applied document (I-000465). The applied path admits one inner shape — a declared inner graph
  with no steps (see below) — and that shape is built and enterable: `project_inner_graphs` keeps it,
  `build_inner_graphs` inserts it, and `EnterAction::InnerGraph` spawns a child session for it. On restart
  the child's `graph_name` is checked against the descriptor manifest's `valid_inner` before the terminal
  short-circuit, so an inner graph only the applied document declares already makes the run
  non-replayable.
- **Apply validates the bytes it installs, not whether intake can build a graph from them**, so an applied
  document installs and then refuses at admission (I-000448). The applied path carries no bundle root —
  serving a template would mean reading another package's bytes — so the §8.2.1 template refusal is
  unconditional there rather than conditional on a bad path: **any state whose `exit_when` is `llm_judge`
  is refused, and so is every inner step of every inner graph**, whether or not it names a template — the
  schema allows an inner graph with no steps, which is the only inner-graph shape the applied path admits
  (I-000449).
- **A standalone `judge`, `manual_wait` or `join` node has no projected preset representation and never
  will**, so a build from such a document refuses it `ir_unprojected` permanently
  ([§8.2.1](orchestration-engine.md#821-ir--task-mapping)). The authoring form is the `outer_state`
  carrier: `exit_when.kind=llm_judge` plus its branch edges for a judgment, `exit_when.kind=manual` for a
  wait, and `merge` / `converge` / `timeout_ms` / `on_timeout` for a join. No direct projection of the
  kind spelling is planned (COMPASS D10).

### 6. Why one structural authority

The Canvas may not write structure through two authorities. With the compose flag on, a candidate document
owns the graph, and every legacy YAML structural write — node and edge patches, transition create/update —
is refused at a single dispatch point (`strategy-yaml-authority`). Per-control guards cannot converge: each
round closes one control and leaves its neighbour reachable. With the flag off, the legacy authority
dispatches unchanged.

The flag is `strategy_canvas_compose`, resolved per surface. Developer entrance defaults on, content-creator
default off, an explicit selection wins. Flag-off renders exactly the pre-compose surface.

### 7. Boundaries

Compose, approve and apply ship. IR-primary authoring for the daemon surfaces and retirement of the
hand-authored YAML path do not: intake still prefers the YAML loader for every strategy without a current
apply, and YAML remains the authoring path. Who may write the pattern registry is not defined here: local
presence is not a trust claim, and a publishing channel needs its own policy (I-000422). A followed symlink
target and the member check/open window are the trust-boundary work that policy owns.

#### 7.1 Repository samples

Every bundle under `strategy-samples/` is IR-primary: `workflow.json` is a `WorkflowDocument` with
`state: candidate` and is the hand-edited authoring source, and `preset.yaml` is that document's export,
not a file an author edits. Authoring is direct JSON editing — no candidate row, compose op, approve or
apply is involved, and Canvas is not required for repository sample authoring. CI closes the pair: it
imports each committed `preset.yaml`, re-exports the sample's `workflow.json` through
`nexus_preset::export_preset` (`crates/nexus-preset/src/workflow_ir.rs`), and fails at the divergent YAML
path when the committed manifest is not that export. `assets[]` is the one member the export does not
project, so it stays hand-maintained: one entry per carrier-named template, each `content_hash` a BLAKE3
digest over the template's bytes.

Two surfaces stay YAML-primary and this page's boundary holds for them: `examples/first-workflow` and the
embedded presets keep `preset.yaml` as their source, and they are the live evidence that a hand-authored
YAML bundle still imports and exports. Nothing in the sample switch touches the intake precedence of
§5.1, the legacy YAML write path of §6, or the pattern registry policy above.

## Invariants

- **One pin authority.** Identity is the canonical content hash of the IR fragment; the manifest version
  and the manifest itself never are. Approve records hashes and apply recomputes them.
- **One compare-and-swap authority.** `strategy_candidates.revision` is the only one, and the embedded
  `document.revision` mirrors it in the same transaction.
- **One structural authority per flag value.** With compose on, no legacy YAML write reaches structure;
  with it off, the legacy authority dispatches unchanged.
- **Nothing refuses after a write.** Every refusal in compose, approve and apply is decided before the
  row exists — the stale-revision check, the already-current guard and the insert are one statement each.
- **Append, never mutate.** `strategy_applies` and `strategy_approvals` are read-only after write; the
  current apply is derived as the row with no successor, so rollback is another append.
- **Approval is not a no-op surface.** Every control that offers approve shows the level, the stubbed
  judges, the fence and the pins for the hash being approved, or renders disabled with a reason.

## Where it lives

- Core: `crates/nexus-core/src/apply.rs` (apply, current-apply read), the strategy candidate and approval
  services beside it.
- Storage: `strategy_candidates`, `strategy_approvals`, `strategy_applies` in `crates/nexus-local-db`.
- Contracts: `schemas/daemon-api/orchestration/strategy-{apply-request,applied-status,approvals}.schema.json`.
- Service: `apps/nexus-service/src/presets.ts` (routes), `apps/nexus-service/src/errors.ts` (status mapping).
- Surface: `apps/web/src/components/canvas/strategy-canvas/`.

## Change obligations

A change to a refusal, a status code, the totals order backing the applied and approval reads, or the
composition flag's audience default updates this page and its schema in the same change. A second write
authority, a current pointer, or an in-place update of an apply row contradicts this page and needs its
own decision record first.

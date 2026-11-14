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
source records a row nothing reads), `strategy_has_no_schedule` (nothing would run) and `already_applied`
(the named approval is already current). `already_applied` and `approval_hash_changed` return 409; the rest
are refusals on the same envelope.

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

For a **user bundle with a current apply**, admission rebuilds the graph from the applied document through
the [§8.2.1 mapping](orchestration-engine.md#821-ir--task-mapping) and runs that; every other strategy keeps the YAML loader of
[§8.1](orchestration-engine.md). The discriminator is the source identity the loader recorded, never a
preset lookup — the embedded-first lookup would hand an applied user bundle the embedded preset's graph when
the ids collide.

Admission re-runs the same recomputation before it builds: an apply whose `document_json` no longer hashes
to its recorded `content_hash` is `applied_document_changed`, and one that cannot be read is
`applied_document_unverifiable`. Bytes edited after apply are therefore a typed refusal, not a reinterpreted
run.

**Known limit — source freeze holds within one process lifetime.** An applied run is admitted and recovered
with the YAML descriptor, so a daemon restart during an in-flight run rebuilds the *preset* graph, and
`validate_agent_bindings` still derives its role sets from the preset, so a prompt role that exists only in
the applied document is not provider-checked and the run fails at the prompt executor after admission
(I-000443). A second limit is adjacent and wider than "the templates could not be resolved": apply
validates the bytes it installs, not whether intake can build a graph from them, so an applied document
installs and then refuses at admission (I-000448). The applied path carries no bundle root — serving a
template would mean reading another package's bytes — so the §8.2.1 template refusal is unconditional
there rather than conditional on a bad path: **any state whose `exit_when` is `llm_judge` is refused, and
so is every inner step of every inner graph**, whether or not it names a template — the schema allows an
inner graph with no steps, which is the only inner-graph shape the applied path admits (I-000449).

### 6. Why one structural authority

The Canvas may not write structure through two authorities. With the compose flag on, a candidate document
owns the graph, and every legacy YAML structural write — node and edge patches, transition create/update —
is refused at a single dispatch point (`strategy-yaml-authority`). Per-control guards cannot converge: each
round closes one control and leaves its neighbour reachable. With the flag off, the legacy authority
dispatches unchanged.

The flag is `strategy_canvas_compose`, resolved per surface. Developer entrance defaults on, content-creator
default off, an explicit selection wins. Flag-off renders exactly the pre-compose surface.

### 7. Boundaries

Compose, approve and apply ship. The dual-read loader, IR-primary authoring and retirement of the
hand-authored YAML path do not: intake still prefers the YAML loader for every strategy without a current
apply, and YAML remains the authoring path. Who may write the pattern registry is not defined here: local
presence is not a trust claim, and a publishing channel needs its own policy (I-000422). A followed symlink
target and the member check/open window are the trust-boundary work that policy owns.

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

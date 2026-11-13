# Strategy composition — pattern packages, compose ops, approval freeze

Status: current · Owns the wire contract for pattern packages, the four compose operations, and what an
approval freezes. The simulator contract is separate: [strategy-simulation.md](strategy-simulation.md).

## 1. Pattern packages

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

## 2. The candidate

A strategy's draft lives in one `strategy_candidates` row: `strategy_id` is the primary key, `document_json`
holds the complete `WorkflowDocument` with `state: candidate` and `id == strategy_id`, and `revision` is the
**single compare-and-swap authority** — the embedded `document.revision` mirrors the row in the same
transaction. A strategy with no row projects the preset import at revision `0` and persists nothing.

`GET /v1/daemon/strategies/{id}/candidate` returns the row when present, the revision-`0` projection
otherwise. It never creates a row.

## 3. Compose operations

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

## 4. What approval freezes

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

## 5. Why one structural authority

The Canvas may not write structure through two authorities. With the compose flag on, a candidate document
owns the graph, and every legacy YAML structural write — node and edge patches, transition create/update —
is refused at a single dispatch point (`strategy-yaml-authority`). Per-control guards cannot converge: each
round closes one control and leaves its neighbour reachable. With the flag off, the legacy authority
dispatches unchanged.

The flag is `strategy_canvas_compose`, resolved per surface. Developer entrance defaults on, content-creator
default off, an explicit selection wins. Flag-off renders exactly the pre-compose surface.

## 6. Boundaries

P3 ships compose and approve. It does not ship the dual-read loader, IR-primary authoring, retirement of the
hand-authored YAML path, or Apply/Execute from the Canvas. Who may write the pattern registry is not defined
here: local presence is not a trust claim, and a publishing channel needs its own policy (I-000422). A
followed symlink target and the member check/open window are the trust-boundary work that policy owns.

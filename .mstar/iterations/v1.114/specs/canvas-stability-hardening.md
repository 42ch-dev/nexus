# Canvas Stability & Validation Hardening — Iteration Spec

| Attribute | Value |
| --- | --- |
| **Status** | Draft (V1.114) |
| **Document class** | Iteration-scoped spec draft |
| **Scope** | Server-side validation gap closure + command-registry collision detection + conflict-modal harmonization + canvas residual cluster closure |
| **Coordinates with** | [canvas-strategy-surface.md](../../../specs/canvas-strategy-surface.md) §3.5 (write boundary), [daemon-api-surface-conventions.md](../../../specs/daemon-api-surface-conventions.md) |
| **Parent plan** | `2026-07-13-v1.114-canvas-stability-hardening` |

## Product framing (why authors care)

Authors trust Nexus because the canvas is a **structured write surface**, not a
free-form whiteboard that can silently corrupt Strategy graphs. When a self-loop
edge or duplicate transition rule can only be blocked in the browser, any other
client (desktop webview edge cases, future API tools, tests) can land invalid
structure — and the author only discovers it when the graph stops making sense.

This plan is the **stability + honesty** half of the canvas foundation: the
daemon owns the invariants authors already expect, conflict recovery feels the
same on every surface, and the residual slate for canvas is honest enough that
"foundation" is not a claim papered over open debt.

Stability is product quality: **authors should never have to wonder whether the
server accepted an impossible graph.**

## User stories

1. **As an author editing Strategy transitions**, I cannot create a self-loop
   edge even if a bug bypasses the client dialog — the daemon rejects it with a
   clear error.
2. **As an author (or agent) patching Strategy rules**, I cannot accumulate
   duplicate `(to, when)` transition rules that make the state machine
   ambiguous.
3. **As an author who hits a write conflict**, focus, keyboard, and screen-reader
   announcements behave consistently whether I am on Strategy, Outline, World
   KB, or Relationships — I learn the recovery pattern once.
4. **As a product team claiming a canvas foundation**, we can show ≥10
   canvas-cluster residuals actually closed or consciously accepted — not left
   as noise under a "stable" banner.

## Problem

The canvas write boundary has two known server-side validation gaps where the
daemon trusts client-only guards:

1. **Self-loop guard missing server-side** (`R-V1109-P1-QC2-W001`): the Strategy
   edge-rewiring `op:create` does not reject `source == new_target` on the
   server. Client guards exist in `state-machine.tsx` + the edge-create dialog,
   but `outline.rs` has the server precedent — the Strategy path should match.
2. **Duplicate transition rules** (`R-V1109-P1-QC2-W002`): `op:create` appends a
   transition rule without dedup; the daemon can store duplicate `(to, when)`
   rules. Validation checks target existence but not rule uniqueness.

Separately:

3. **Command registry collision** (`R-V1111P0QC2-W002`): the command palette
   registry is an id-keyed `Map` (last-write-wins) with no runtime collision
   detection. Two surfaces registering the same id silently overwrite. Low risk
   at 7 commands today, but the canvas command surface is growing — silent
   overwrite means authors may invoke the wrong action with the same shortcut.
4. **Conflict-modal drift**: the four surfaces each host a conflict modal
   (`conflict-modal.tsx`, `outline-conflict-modal.tsx`,
   `world-kb-conflict-modal.tsx`, `world-kb-relationship-conflict-modal.tsx`).
   They share `conflict-modal-base.tsx` but have accumulated surface-specific
   drift in focus management, keyboard shortcuts, and ARIA announcement timing
   — recovery feels different per surface.

Finally, the canvas residual cluster in `status.json` has ~15 open low/nit
items targeting the canvas trajectory. Closing a bounded batch keeps the slate
honest for the foundation iteration.

## Goals

1. **Server-side validation guards** — ship the self-loop rejection and the
   duplicate-transition-rule dedup on the Strategy patch path. The daemon
   returns 422 for these invariants, not just the client guard.
2. **Command-registry collision detection** — at minimum, dev-mode `console.warn`
   when two commands register the same id. Optionally, hard error in dev mode.
3. **Conflict-modal harmonization** — audit the four conflict-modal hosts for
   focus-management, keyboard-shortcut, and ARIA-timing drift; align them on
   the shared base without losing surface-specific copy.
4. **Canvas residual closure** — close ≥10 canvas-cluster open residuals from
   `status.json` (verify each against current code before closing; accept with
   rationale where the residual is now intentional).

### Architect decisions (locked by @architect — Review chain Seat 2)

- **Duplicate transition dedup → reject 422.** The daemon already implements
  this in `append_conditional_rule()` (`strategy.rs` ~L900) with error code
  `strategy_transition_duplicate` (`BadRequest`). Silent-dedup is rejected:
  it hides the client error and contradicts the structured-write-boundary
  principle (no silent corruption). **T2's job is to VERIFY the existing guard
  against current code and CLOSE `R-V1109-P1-QC2-W002` as already-resolved** —
  not to reimplement it. If a gap is found in the guard's coverage (e.g.
  `update` op path), fix that gap; do not change the reject semantics.
- **Self-loop guard → reject 422** with error code `strategy_self_loop`.
  `apply_transition_create()` has no `source_state_id == new_target` check
  today — this is the real gap (`R-V1109-P1-QC2-W001`). Add the guard at the
  handler level (fail fast, before YAML mutation), matching the `outline.rs`
  precedent (~L1259). Do not rely solely on `validate_preset_yaml` catching it
  after mutation.

## Non-goals

- Rewriting the conflict-modal base (harmonize on the existing base, do not
  redesign)
- Closing the entire residual slate (bounded ≥10 canvas-cluster closures only)
- Adding new canvas validation rules beyond the two named gaps (self-loop +
  dedup)
- Changing command-palette UX for end users (collision detection is safety for
  developers / multi-surface registration, not a new palette feature)
- P0 adapter/layout work (owned by canvas-architecture-foundation)

## Acceptance criteria (product-facing)

- [ ] Bypassing the client, a self-loop Strategy edge create is rejected by the
  daemon with a clear 422-class error (authors never persist impossible edges)
- [ ] Duplicate `(to, when)` transition rules cannot accumulate as distinct
  stored rules — **reject 422** (`strategy_transition_duplicate`), not
  silent-dedup (locked by @architect; already implemented in
  `append_conditional_rule`)
- [ ] The four conflict modals share the same focus trap / keyboard dismiss /
  ARIA timing behavior (copy may differ)
- [ ] ≥10 canvas-cluster residuals closed or accepted with written rationale in
  residual SSOT
- [ ] Command id collisions are detectable in dev (warn at minimum) so silent
  overwrite cannot hide wrong-action risk as surfaces grow

## Verification

- New server-side tests: self-loop `op:create` returns 422 (`strategy_self_loop`);
  verify the existing duplicate-rule test
  (`patch_transition_create_rejects_duplicate_rule` → `strategy_transition_duplicate`)
  still passes against current code.
- Command-registry test: registering a duplicate id emits the warning/error.
- Existing canvas tests all pass.
- Each closed residual verified against current code (not stale) — `R-V1109-P1-QC2-W002`
  must be verified already-resolved before closing.

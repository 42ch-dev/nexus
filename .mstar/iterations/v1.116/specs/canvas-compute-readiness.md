# Canvas / Compute Readiness Research (V1.116 P2)

> Iteration-scoped research brief for V1.116 P2. **Research + writing only —
> no production code.** Not a normative `{SPECS_DIR}` Master. Output stays in
> this iteration workspace until a later iteration promotes chosen work.

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-13-v1.116-canvas-compute-readiness-research` |
| **Tier** | Should |
| **Audience** | Next-iteration PM + architect (direction pick) |
| **primary plan** | `.mstar/plans/2026-07-13-v1.116-canvas-compute-readiness-research.md` |
| **Output artifact** | This file (filled by P2 execution) + three candidate skeleton sections |

## Problem framing

V1.114 made Canvas and Compute **visible as foundations**. V1.115 made those
foundations **honest and reusable** (adapter complete across product
orchestrators; compute manifest single-source). The roadmap names three
deep-dive candidates for a later iteration:

1. **Strategy `onConnect`** — inner-graph groups (canvas capability depth)
2. **Compute state editor** — human-writable module `body.state` (compute depth)
3. **5th canvas surface** — compute graph / session replay on the adapter

Before PM picks one for V1.117+, V1.116 must answer: **are the foundations
actually ready, or are there hidden gaps that would explode mid-implement?**

This plan produces **evidence**, not a new product surface.

## User value

| Who | Why they care |
| --- | --- |
| **Next-iteration PM** | Can enter Prepare on a chosen deep-dive with gap list + skeleton already drafted — no blind direction pick. |
| **Architects / implementers** | Know which foundation claims are verified vs aspirational; avoid rediscovering adapter/compute gaps mid-sprint. |
| **Authors (deferred)** | Next capability wave lands on proven rails instead of half-finished foundations. |

## Goals

1. **Canvas adapter extensibility audit** — can a 4th/5th orchestrator adopt
   `useCanvasSurface` / `CanvasSurfaceAdapter` cleanly after V1.115?
2. **Compute state-write readiness audit** — is module state still read-mostly?
   What write boundary / OCC / conflict model is missing for a state editor?
3. **Gap analysis** for each of the three candidates (wire/API/UI/domain).
4. **Recommended priority ordering** with product + risk rationale.
5. **Three candidate spec skeletons** (Problem / Scope / key Interfaces / open
   questions) — enough for next iteration to enter Prepare without a blank page.

## Non-goals

- Implementing any candidate
- Full Draft / normative Master specs (skeletons only)
- Wire schema changes or production code
- Writing new `{KNOWLEDGE_DIR}` docs (iteration-close compound only)
- Closing residual burn-down outside readiness implications

## Why Should (not Must)

P2 is **Should**:

- It does **not** unblock authors on first launch (that is P0).
- It does **not** unblock daily maintainer honesty (that is P1).
- It **does** prevent a blind deep-dive next iteration — high leverage, not a
  ship blocker for V1.116 author-facing Done.

If capacity forces a cut, **cut P2 before P0/P1**. Prefer shipping a thinner
readiness note over skipping P0 detection honesty.

## Target state (research Done)

A single readiness document (this file, completed) that a reader who was not
in grill-me can use to:

1. Understand foundation readiness for Canvas and Compute.
2. Compare three candidates with explicit gaps.
3. See a recommended pick order with rationale.
4. Open a skeleton and start Prepare for the chosen candidate.

## Acceptance criteria (PM/maintainer-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P2-1** | Readiness assessment delivered in this workspace file | File exists under `v1.116/specs/canvas-compute-readiness.md` with audit sections filled (not stub headings only) |
| **AC-P2-2** | Canvas adapter extensibility verdict is evidence-based | Section cites concrete code/spec paths; states ready / ready-with-gaps / not-ready |
| **AC-P2-3** | Compute state-write readiness verdict is evidence-based | Section cites runtime/schema paths; states what write path exists or is missing |
| **AC-P2-4** | Three candidate skeletons present | Each of Strategy `onConnect`, compute state editor, 5th surface has Problem / Scope / key Interfaces / open questions |
| **AC-P2-5** | Recommended priority ordering with rationale | Explicit ordered list + why #1 over #2/#3 for next iteration |
| **AC-P2-6** | Compass roadmap "Immediate next" remains aligned | Compass references this readiness output as the pick enabler |

## Candidate inventory (locked — skeletons only)

| Candidate | Pillar | Product question |
| --- | --- | --- |
| Strategy `onConnect` for inner-graph groups | Canvas depth | Can authors wire group connections on Strategy without a second graph model? |
| Compute state editor | Compute depth | Can authors inspect and edit module state in human-readable form safely? |
| 5th canvas surface (compute graph / session replay) | Canvas breadth | Can a new surface reuse the V1.115 adapter recipe without forking shell code? |

## Skeleton template (each candidate)

Each skeleton section must include:

```markdown
### Candidate: <name>

#### Problem
#### Scope (in / out)
#### Key interfaces (known or hypothesized)
#### Open questions (for next Prepare)
#### Dependencies on foundation gaps (from audits above)
```

## Product decisions (locked)

| Decision | Choice | Rationale |
| --- | --- | --- |
| Deliverable shape | Readiness spec + 3 skeletons (one file OK) | Grill-me #4 |
| Implementation | None this iteration | Stabilize + research only |
| Priority tier | Should | Not author first-impression; enables next pick |
| Artifact boundary | `v1.116/specs/` only | No premature `{SPECS_DIR}` / knowledge promotion |

## Architect decisions (seat 2 — resolved)

### AD-1: Evidence-bar checklist for readiness verdicts

Every readiness verdict ("ready" / "ready-with-gaps" / "not-ready") must be
backed by concrete evidence. The checklist:

| Verdict | Bar (ALL must hold) |
| --- | --- |
| **ready** | (1) Production code path exists and is exercised by ≥1 passing test; (2) Interface contract is typed (not `any`/`untyped`); (3) At least one product orchestrator/consumer uses it end-to-end today; (4) No open residual blocks the candidate's write path |
| **ready-with-gaps** | (1) Core code path exists; (2) But: missing test coverage, OR missing typed boundary at the extension point, OR one open residual affects the candidate but has a documented workaround |
| **not-ready** | (1) Code path does NOT exist, OR (2) exists but is stub/`todo!()`, OR (3) has a blocking residual with no workaround, OR (4) the extension surface would require a breaking change to the existing contract |

**Canvas adapter extensibility audit — code paths to cite:**

| Audit dimension | Code path to read |
| --- | --- |
| Adapter contract | `apps/web/src/canvas/` — `useCanvasSurface` hook + `CanvasSurfaceAdapter` interface (typed props, edge/node ops, layout trigger) |
| Existing consumers | Strategy, Outline+Timeline, World KB orchestrators — verify each implements the adapter cleanly |
| Extension point | What a 4th/5th orchestrator must provide: node types, edge types, layout fn, panel components. Is this a typed interface or ad-hoc? |
| Knowledge pattern | `.mstar/knowledge/architecture-patterns/canvas-surface-implementation-pattern.md` — V1.115 recipe |
| Spec | `.mstar/specs/canvas-strategy-surface.md` — normative contract |

**Compute state-write readiness audit — code paths to cite:**

| Audit dimension | Code path to read |
| --- | --- |
| Current runtime | `crates/nexus-daemon-runtime/src/api/handlers/` — compute module routes (are they read-only GET, or is there a write POST?) |
| Module state shape | `crates/nexus-contracts/` — `ModuleManifest` / `ModuleDetail` / module `body.state` typed shape |
| Write boundary | Does any handler accept state mutations? Is there an OCC/version field? Conflict resolution? |
| Compute ABI | `.mstar/specs/` — compute module manifest / ABI spec (is `body.state` documented as mutable?) |

### AD-2: Candidate skeleton depth

Each candidate skeleton must include **key interfaces** — not just names, but
the hypothesized function/component signatures a Prepare-phase spec would
start from. Examples:

- **Strategy `onConnect`:** `onConnect(sourceNodeId, targetNodeId, edgeConfig)
  => void | ConflictResult` — what wire message? What canvas event?
- **Compute state editor:** `PATCH /v1/daemon/compute/modules/{id}/state`
  with `If-Match` header (OCC)? Or a command-style `SetState` operation?
- **5th surface:** `CanvasSurfaceAdapter` implementation for compute graph —
  what are the node/edge types? What layout algorithm?

These are **hypothesized**, not final — but they give the next PM a concrete
starting point instead of a blank page.

### AD-3: Recommendation strength

Present a **decision matrix** (not just an ordered list) so PM can re-evaluate
if product priorities shift. Columns: candidate, foundation readiness, effort
estimate (S/M/L), author impact (high/med/low), risk of mid-implement gap
discovery. Then state the architect's recommended #1 pick with rationale —
but PM owns the final call.

## Mapping to plan tasks

| AC | Plan tasks |
| --- | --- |
| AC-P2-1..3 | T1 Canvas audit + T2 Compute audit |
| AC-P2-4..5 | T3 gap analysis + skeletons + priority |
| AC-P2-6 | Compass alignment (already drafted; confirm at research close) |

---

## Research output (filled during P2 execute)

> Leave structure below for implementers; seat 1 seeds headings only.

### Canvas adapter extensibility audit

_(To be filled — verdict + evidence paths.)_

### Compute state-write readiness audit

_(To be filled — verdict + evidence paths.)_

### Candidate: Strategy `onConnect`

#### Problem
_(skeleton during P2)_

#### Scope (in / out)
_(skeleton during P2)_

#### Key interfaces
_(skeleton during P2)_

#### Open questions
_(skeleton during P2)_

#### Dependencies on foundation gaps
_(skeleton during P2)_

### Candidate: Compute state editor

#### Problem
_(skeleton during P2)_

#### Scope (in / out)
_(skeleton during P2)_

#### Key interfaces
_(skeleton during P2)_

#### Open questions
_(skeleton during P2)_

#### Dependencies on foundation gaps
_(skeleton during P2)_

### Candidate: 5th canvas surface

#### Problem
_(skeleton during P2)_

#### Scope (in / out)
_(skeleton during P2)_

#### Key interfaces
_(skeleton during P2)_

#### Open questions
_(skeleton during P2)_

#### Dependencies on foundation gaps
_(skeleton during P2)_

### Recommended priority ordering

_(To be filled — ordered list + rationale.)_

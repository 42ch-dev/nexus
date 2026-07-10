# Author Surfaces Backlog (V1.107)

**Status:** Index — product triggers locked (§5.1 PM); writing-complete (§5.3)  
**Owner:** @project-manager (promote to Must when trigger fires)  
**Related:** FB-V1107-016  
**Deliverable:** Index + domain badge matrices in Studio `/components` — **not** full Surfaces routes this iteration

DESIGN documents Reading, SOUL, Canvas, Memory, and Findings chrome, but Design Studio has **no** Surfaces routes for these domains yet. V1.107 lands badge preview matrices and a promotion-ready backlog so the next iteration can pilot Surfaces without rediscovering scope.

## Promotion triggers (when to move from backlog → Must)

| Domain | App areas (examples) | **Promote when** | Suggested first fixture | Suggested owner |
|--------|----------------------|------------------|-------------------------|-----------------|
| **Reading** | `apps/web/src/components/reading/**` | V1.107 P0 Done **and** product prioritizes reading-loop polish **or** reading chrome drifts from DESIGN | Reading chrome strip / annotation highlight row | @frontend-dev |
| **SOUL** | `apps/web/src/components/soul/**` | Product asks SOUL visualization polish **or** SOUL status chips ship in App without Studio preview | Growth curve / status chips strip | @frontend-dev |
| **Canvas** | `apps/web/src/components/canvas/**` | Canvas IA iteration starts **or** new canvas chrome ships App-first | Canvas shell + context menu chrome | @frontend-dev |
| **Memory** | `apps/web/src/components/memory/**` | Memory review-loop polish scheduled **or** TaskKind badges need Surfaces beyond `/components` matrix | TaskKind badge row in Surfaces context (extends 016 matrix) | @frontend-dev |
| **Findings** | `apps/web/src/components/findings/**` | Findings triage polish scheduled **or** finding status pills ship without Studio matrix | Finding status pill matrix in Surfaces (extends 016) | @frontend-dev |

## V1.107 deliverable (FB-016)

1. **Badge matrices** under Studio `/components`: Status, Chapter, Finding, TaskKind — light+dark smoke.
2. **This index** kept current with owners and triggers above.
3. **Does not block** Must close: no `/surfaces/reading|soul|canvas|memory|findings` routes required in V1.107.

## Anti-patterns (do not promote silently)

- App-first Surfaces chrome without Studio fixture + DESIGN contract
- Full domain Surfaces route before badge matrix proves token coverage
- Blocking V1.107 close on Reading/SOUL/Canvas/Memory/Findings routes

## Next iteration default

After P0 Done: PM reviews this index at iteration-close; first pilot typically **Reading** or **Findings** depending on product priority queue.

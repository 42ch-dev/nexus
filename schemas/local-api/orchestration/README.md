# Local API — Orchestration Schemas

JSON Schema subtree for the **Orchestration** daemon endpoints under `/v1/local/orchestration/`. These are cross-language contracts consumed by future WebApp/Web-UI clients.

## Subtrees

| Directory | Contents | Wave |
|-----------|----------|------|
| [sessions/](sessions/) | Engine session list + detail-status (READ-only) | V1.63 P3 |
| [capabilities/](capabilities/) | Capability registry list | V1.63 P3 |

**Note:** Schedule list DTOs live under [`local-api/schedule/`](../schedule/) (V1.63 P1) — no duplication needed. Preset management DTOs live under [`local-api/preset-management/`](../preset-management/) (V1.63 P3).

## Deferred (V1.64+)

- Agent-host sessions/operations/events (SSE) — stateful long-lived DTOs deferred per compass §1.2 non-goal.
- Create/signal/cancel session write-side DTOs.

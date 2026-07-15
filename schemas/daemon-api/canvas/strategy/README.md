# Daemon API — Canvas Strategy Schemas

JSON Schemas for the **Strategy Canvas** structured write boundary under `/v1/daemon/strategies/*`. These are cross-language contracts consumed by the local Web UI (`apps/web`).

V1.71 — promoted from the Draft write-boundary contract in `canvas-strategy-surface.md` §3.5 and `daemon-api-surface-conventions.md` §7.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/patch` | `strategy-patch-state-request.schema.json`, `strategy-patch-response.schema.json` |
| `POST /v1/daemon/strategies/{strategy_id}/transitions/patch` | `strategy-patch-transition-request.schema.json`, `strategy-patch-response.schema.json` |
| `POST /v1/daemon/strategies/{strategy_id}/states/{state_id}/prompt/patch` | `strategy-patch-prompt-template-request.schema.json`, `strategy-patch-response.schema.json` |

## Shared / error shapes

| Shape | Schema file |
|-------|-------------|
| Structured 409 conflict detail (placed in `ErrorResponse.details`) | `strategy-conflict-error.schema.json` |

## Related

- **Consumer:** `@42ch/nexus-contracts` (npm) — generated TypeScript types
- **Handlers:** `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs`
- **Specs:** `.mstar/specs/canvas-strategy-surface.md` §3.5, `.mstar/specs/daemon-api-surface-conventions.md` §7
- **Compass:** `.mstar/iterations/v1.71/delivery-compass.md` §1.1 Track A

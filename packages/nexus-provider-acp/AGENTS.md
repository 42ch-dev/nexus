# @42ch/nexus-provider-acp

Real `@agentclientprotocol/sdk` stable-v1 provider adapter. **Implemented** by
P2-T2 and shipped here: `src/acp.ts` (session/probe/execute/cancel), `src/delivery.ts`
(bounded per-operation delivery accumulator), `src/process-owner.ts` (owned child
spawn + identity-checked reap), `src/identity.ts` (OS process identity), `src/recipe.ts`
(recipe admission), `src/errors.ts` (sanitized wire errors).

The module is loaded at runtime by the native host (it is not a compile-time
dependency of the daemon), so the pins below are the compatibility contract.

## Contract (P2-T2)

- Public entry: `createAcpProvider(): ProviderCallbacks` from `src/index.ts`.
  The caller supplies no launch recipe; Rust probe/launch payloads carry the
  generated `ValidatedProviderRecipe` after native admission.
- Stable SDK surface only (`ClientSideConnection`, `ndJsonStream`,
  initialize/newSession/prompt/cancel/sessionUpdate). No `experimental/*`
  exports, no copied JSON-RPC stack.
- Retain process/SDK state per recipe generation; keep
  child/stdin/stdout/stderr/connection until cleanup; readiness requires a
  no-model handshake and a reaped probe child.

## Pins

`@agentclientprotocol/sdk` 1.4.0, `zod` 4.6.2 (exact), `@42ch/nexus-contracts`
workspace. `zod` satisfies the SDK peer range `^3.25 || ^4`.

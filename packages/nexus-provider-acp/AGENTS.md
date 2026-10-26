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
- SDK 2.0 app API on `@agentclientprotocol/sdk/unstable` (`client()`,
  `ndJsonStream`, `onNotification`/`onRequest`, and
  `connection.agent.request`/`notify` for initialize, session/new,
  session/set_mode, session/set_config_option, session/prompt, session/cancel).
  The `unstable` entrypoint is required for `session/set_config_option`; no
  `experimental/*` exports, no copied JSON-RPC stack.
- Retain process/SDK state per recipe generation; keep
  child/stdin/stdout/stderr/connection until cleanup; readiness requires a
  no-model handshake and a reaped probe child.

## Pins

`@agentclientprotocol/sdk` 2.0.0, `zod` 4.6.7 (exact), `@42ch/nexus-contracts`
workspace. `zod` satisfies the SDK peer range `^3.25.76 || ^4.0.0`.

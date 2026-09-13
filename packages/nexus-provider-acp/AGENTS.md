# @42ch/nexus-provider-acp

Real `@agentclientprotocol/sdk` stable-v1 provider adapter. **Implementation is
owned by P2-T2**; this directory currently carries the dependency/tsconfig
reservation only.

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

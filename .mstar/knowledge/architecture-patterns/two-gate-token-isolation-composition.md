---
module: nexus-spoke-adapter, apps/nexus42, connect-host
date: 2026-08-08
problem_type: architecture_pattern
category: architecture-patterns
severity: high
tags: [capability-token, tenant-isolation, enforcement-composition, spoke-connect, peer-scope, fail-closed, issuer-key, intersection]
applies_when: enforcing capability tokens or authz claims on a multi-tenant boundary; composing an external gate with an existing in-app scope gate; adding an issuance path for signed credentials
---

# Two-Gate Token Isolation Composition (V1.155 P1 pattern)

## Context

The capability-token surface was a **structural gate only**: production
defaults had `trusted_issuers` empty, `capability_token_provider = None`,
`require_capability_token = false` — hardcoded in `build_config`. There was no
issuance path, no operator config surface, and no tenant-isolation enforcement
tying token capabilities to world/op scope (residual `R-V1148P3-001`, PD-09
requirement). V1.155 P1 shipped issuance + config wiring + enforcement.

The load-bearing insight: **enforcement is COMPOSED, not duplicated**. The
nexus side does NOT re-implement token checks — it wires config into
spoke-connect's existing gates and intersects them with its own `PeerScope`
allowlist gate. A token can never widen nexus scope.

## Guidance

### 1. Compose existing gates — do not re-implement them

Two INDEPENDENT gates form the fail-closed intersection (verified against
spoke-connect 0.9.2):

- **spoke-side invoke gate**: `evaluate_invoke_token_gate` enforces the
  token-required assertion BEFORE the nexus invoke handler runs — a session
  that has not completed the token challenge is rejected with the `auth_failed`
  wire envelope (zero side effects; the nexus handler never sees the invoke).
- **spoke-side op-dispatch gate**: `required_capability(op)` ⊆
  `negotiated_capabilities` AND `token_authorizes_op`. A token whose
  capabilities don't cover the op is rejected spoke-side (`op_unsupported`).
- **nexus `PeerScope` gate** (invoke.rs, N-C1): the peer's
  `world_scope`/`op_scope`/`module_scope` is enforced independently of the
  token. A token granting `l2-computable` to a peer whose `op_scope` lacks
  `compute` is denied by the nexus gate.

Composition invariant: **effective grants = nexus `PeerScope` scope ∧
spoke-side (negotiated ∩ token)**. No new nexus gate code needed for token
enforcement — the P1 task was primarily a TEST task verifying the composed
invariant.

### 2. Fail-closed everywhere it can be

- `config.json` (`~/.nexus42/connect/config.json`, `serde deny_unknown_fields`):
  malformed file ⇒ boot error (no silent defaults). `require_capability_token:
  true` with EMPTY `trusted_issuers` ⇒ boot error (fail-closed, refuses boot
  with clear message).
- Absent config file ⇒ today's defaults (empty / `None` / `false`) — defaults
  stay opt-in; production ≠ flipping the default on.
- `capability_token_provider` stores the issuer key PATH + enabled flag;
  `build_config` constructs the mint-on-demand closure from the key at that
  path — no event-loop I/O in the provider.

### 3. Issuer key lifecycle is its own trust role

- `~/.nexus42/connect/issuer.key`: Ed25519, **create-once 0600**, never
  overwrites. Distinct from `identity.key` — node identity vs token issuer are
  different trust roles (mirror `connect/identity.rs` helpers).
- Issuer peer id derives from the key; claims `iss` MUST match the
  issuer-derived peer id (spoke normative rule). CLI: `nexus42 connect token
  issue --sub <peer-id> --aud <peer-id> --capabilities <c1,c2> --exp <unix>`.
- No revocation list / refresh / issuance endpoint — spoke non-goal preserved
  (offline validation only). CLI issue is the only issuance path.

## Why This Matters

- **Token can NEVER widen scope**: the intersection rule is what makes the
  surface safe for multi-tenant exposure (PD-09). A compromised or over-broad
  token is still bounded by the nexus allowlist.
- **Zero side effects on denial**: `auth_failed`/`op_unsupported` fire before
  the nexus handler, so denied invokes cannot trigger writes or partial state.
- **No duplicated enforcement**: re-implementing the token-required assertion
  in nexus would drift from spoke's gate and create bypass windows. Compose,
  then verify with tests.

## When to Apply

- Adding an authz-claim (token, capability) boundary where an in-app scope
  gate already exists — compose the intersection, never widen.
- Wiring operator config for a security feature: fail closed on malformed
  config and on enabled-without-trust-roots.
- Issuing signed credentials from a CLI: separate issuer keys from node
  identity keys; create-once with restrictive permissions.

## Examples

### Config (fail-closed schema)

```json
{
  "trusted_issuers": ["12D3KooW..."],
  "require_capability_token": true,
  "capability_token_provider": { "enabled": true, "issuer_key_path": "~/.nexus42/connect/issuer.key" }
}
```

### Invariant test (the enforcement proof)

```rust
// token grants l2-computable, but peer op_scope lacks compute
// -> nexus PeerScope gate denies even though token_authorizes_op passed
assert!(matches!(invoke(&session_with_compute_token, ComputeReq), Err(Reject::ScopeDenied)));
// no token when required -> spoke auth_failed, handler never runs (zero side effects)
assert!(matches!(invoke(&session_without_token, UpsertReq), Err(Reject::AuthFailed)));
```

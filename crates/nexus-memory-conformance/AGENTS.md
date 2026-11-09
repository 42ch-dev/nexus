# nexus-memory-conformance — Independent Durable SPOKE Memory Provider Fixture

Test-only provider used by the N3 memory conformance matrix. It is a second,
genuinely independent implementation of the published `spoke-memory` protocol:
its own SQLite file, schema, bootstrap state, space authority and policy, built
over the published SPOKE crates only. It is a fixture, never a server product.

## Purpose

The matrix needs a durable provider that shares nothing with `nexus-core`'s
local provider, so N3 rows can be proved twice — once through
`LocalMemoryProvider` and once through this fixture — over the same real
protocol boundary. The private operation slices implement the protocol duties,
and `server` assembles the full `MemoryPort` plus the signed Connect memory
service; every scenario therefore observes a signed loopback session rather
than an in-process shortcut.

## Key Rules

- **Test-only, never a product edge**: no product/default graph depends on this
  crate. It is the consumer, and `nexus-core` must stay independent of it.
- **Independence is the property under test**: no `nexus-core` local-provider
  internals, no copied or vendored local storage, no echo mock and no
  repackaging of the local provider. A shortcut here invalidates the matrix.
- **Published dependencies only**: `spoke-connect` / `spoke-operations` come
  from the workspace lockstep pins. Never a sibling path dependency on an
  unpublished upstream engine, never a forked or patched SDK.
- **Real boundary, no rewriting**: observations come from the provider's own
  answers, refusals and receipts over real signed loopback. Do not rewrite wire
  answers to make a row pass.
- **Fail closed with static refusals**: the requester principal derives only
  from the connect-authenticated peer id via the operator table — never from a
  payload. Refusal text is static and never interpolates operator-table values
  or decoder internals.
- **Capability honesty**: only `spoke-memory` is advertised. Reflection and
  transfer are genuinely unavailable and are refused terminally, never
  emulated.
- **Test controls stay behind their own door**: `faults` (fault holds, targeted
  arrival, controllable clock) exists for scenario control only, is unarmed by
  default and must not change admission, authorization or wire answers while
  unarmed.

## Module Layout

- `provider` — `FixtureMemoryProvider`, fixture identity, pool and bootstrap
- `storage` — the fixture's own SQLite schema access
- `spaces` / `candidates` / `records` / `recall` — the protocol slices
- `server` — assembled `MemoryPort`, admission wrapper and signed service
- `policy` — published provider policy and the `PeerDirectory` peer→principal table
- `raw_invoke` — request-scope digest scope and §14.2 error normalization
- `faults` — scenario-only fault and clock controls
- `migrations/` — the fixture's own embedded migration
- `tests/` — matrix targets (`c_*`), remote/Core consumer slices, bootstrap and loopback checks

## Dependencies

- Published/workspace: `spoke-connect` (`remote-adapter`), `spoke-operations`,
  `sqlx` (sqlite), `tokio`, `serde`, `serde_json`, `chrono`, `uuid`,
  `async-trait`, `thiserror`
- Dev-only, public Nexus surfaces: `nexus-core` (`memory-remote`,
  `provider-host`, `execution`, `test-hooks`), `nexus-home-layout`,
  `nexus-contracts`, `nexus-agent-host`, `nexus-provider-ports`
- Direction is one-way: this crate depends on those public surfaces; none of
  them may depend on the fixture.

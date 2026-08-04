---
module: nexus-spoke-adapter, nexus-daemon-runtime, apps/nexus42
date: 2026-08-04
problem_type: knowledge
category: architecture-patterns
severity: medium
plan_id: 2026-08-04-v1.148-p3-spoke-connect-host-facade-n-c0
tags: [spoke-connect, connect-host, feature-gate, libp2p, host-capability-manifest, op-refusal, fl-r, n-c0]
applies_when: adopting a heavy optional transport/network dependency behind a feature gate; building an honest capability manifest; landing a "handshake-only, no-write-ops" host surface
---

# Connect Host — opt-in feature gate for a heavy transport dep (N-C0 pattern)

## Context

V1.148 P3 adopted `spoke-connect` (libp2p 0.56 + noise + yamux + Ed25519 signed-hello) into a CLI product whose default build must stay small and network-surface-free. The Connect Host surface (DF-72 **N-C0**) is the first external-adjacent peer surface: it must handshake + present an honest `HostCapabilityManifest`, but **refuse every inbound write op** (N-C1 write-op exchange is a deliberate later milestone). The pattern below is what made that safe and maintainable.

## Guidance

### 1. The default graph stays free of the heavy dep — verify, don't assume

- Put the dep behind a **non-default cargo feature** on the binary crate (`apps/nexus42`), not in the workspace default:
  ```toml
  # apps/nexus42/Cargo.toml
  [dependencies]
  spoke-connect = { workspace = true, optional = true }
  libp2p = { workspace = true, optional = true }   # only if the binary needs libp2p directly (e.g. fixed-seed identity in tests)
  [features]
  connect-host = ["dep:spoke-connect", "dep:libp2p"]
  default = []
  ```
- The **critical invariant** is gate-checkable, not asserted:
  ```bash
  cargo tree -p nexus42 -i spoke-connect   # default features -> MUST print "did not match any packages"
  cargo tree -p nexus42 -i libp2p           # likewise
  cargo tree -p nexus42 --features connect-host -i libp2p   # present only under the feature
  ```
- Add the invariant to the plan's acceptance criteria and to QC's adversarial checklist. It is the single most important N-C0 property — every other promise (default daemon unchanged, no accidental attack surface) rests on it.

### 2. One capability-manifest builder, shared by every advertiser

- Build the spoke `HostCapabilityManifest` in **one place** (the adapter crate's manifest module) and have **both** the in-process `HostManifestPort` and the Connect Host's `ConnectConfig.local_manifest` consume it. This eliminates drift between "what the daemon tells spoke-protocol consumers" and "what the Connect Host tells peers".
- The builder takes the `host_id` (device-id) as a parameter so tests inject deterministic ids and production resolves from one SSOT.
- **Honesty is a compile-time + runtime contract**: every capability string maps to a production port impl (assert it in a test); never advertise a capability that isn't wired (e.g. `l5-fork` is advertised because `ForkTimelineQueryPort` is production). Banned strings (`"reasoning-complete"`, any `authority`) are asserted absent.
- spoke codegen produces **two** manifest types — `data::HostCapabilityManifest` (port surface) and `connect_hello::HostCapabilityManifest` (wire envelope). They are structurally identical but distinct types; the Connect Host JSON-round-trips from the builder's `data::` form into the `connect_hello::` form. Don't try to make them the same type.

### 3. Total op-refusal via `invoke_handler = None` — not a refuse-each-op handler

- N-C0's "no write ops" is enforced structurally: set `ConnectConfig.invoke_handler = None`. The spoke-connect node maps `None` → `op_unsupported` for **every** inbound invoke before any capability/token check. There is no handler code to reach an adapter write path.
- This is safer than a handler that switches on op-kind: a `None` handler cannot regress into dispatching a write. QC's adversarial check is "could any op reach a `NexusAdapter` write path?" → answer must be structural (no adapter import on the connect path; the `Some(handler)` branch is unreachable upstream).
- The interop test asserts the **real wire envelope** `code == "op_unsupported"` for every core op + garbage, the session stays open after refusal, and there are zero side effects. Inverting these assertions is the N-C1 done-when.

### 4. Identity + allowlist I/O — fail-closed, atomic perms

- Identity key (`~/.nexus42/connect/identity.key`, Ed25519) created with **atomic** `OpenOptionsExt::mode(0o600)` (open-then-chmod leaves a window where the private key is 0644). On reload, self-heal permissive modes (0644→0600) rather than bricking startup.
- Allowlist (`~/.nexus42/connect/allowlist.json` `{peer_ids:[...]}`) is **fail-closed**: missing file → empty → reject all; malformed → hard error. Add `#[serde(deny_unknown_fields)]` so a `peerIds` typo hard-errors instead of silently producing an empty allowlist.
- `--allow-peer` is repeatable and unions with the file.

### 5. Separate OS process, not an in-daemon task

- The Connect Host runs as `nexus42 connect start` — a **separate process**, not a task inside `daemon start`. The default daemon path has zero references to spoke-connect/SpokeConnectNode (grep-verify). Coexistence with Daemon HTTP (the creator UI SSOT) is a product rule (PD-09), not an implementation accident.

### 6. Deterministic interop test (mDNS off, fixed seeds, loopback)

- libp2p timing is the classic CI flake source. The two-node interop test is deterministic by construction: fixed Ed25519 seeds for host + peer, listen `/ip4/127.0.0.1/tcp/0`, **mDNS feature not compiled**, timeout ≥ `DEFAULT_HANDSHAKE_TIMEOUT`, no `sleep`, and a process-wide `OnceLock<Mutex>` (or upstream `NETWORK_TEST_LOCK`) to serialize network tests. Repeat the suite 3–5× locally before trusting it in CI.

## Why this matters

- The feature-gate invariant lets a product adopt a heavy P2P stack without bloating the default binary or exposing a network surface to every install. Losing it would silently put libp2p in every `nexus42` build.
- The shared manifest builder makes "honest capabilities" a property of the code rather than a doc a human keeps in sync.
- `invoke_handler = None` turns "no write ops" from a policy into a type-system-enforced fact — the safest foundation for a milestone (N-C0) whose entire job is to prove the handshake surface before opening the write path (N-C1).

## When to apply

- Adopting any heavy optional transport (libp2p, quic, a gossip layer) behind a feature gate.
- Landing a "metadata/handshake-only" host surface ahead of the full op surface (N-C0→N-C1→N-C2 phasing; any "read profile, refuse writes" milestone).
- Building a capability manifest that must stay honest as capabilities are added/removed.

## Examples

- V1.148 P3 `crates/nexus-spoke-adapter/src/manifest.rs` (shared builder), `apps/nexus42/src/commands/connect/{mod,identity,allowlist,interop}.rs` (Connect Host + interop), `crates/nexus-home-layout/src/device_id.rs` (host_id SSOT).
- Spec: `.mstar/specs/spoke-adapter-architecture.md` §10 (Connect Host N-C0 normative surface).
- N-C1 (write-op exchange) is the next milestone — owner architect, trigger N-C0 dogfood green + partner demand (DF-72 tracker).

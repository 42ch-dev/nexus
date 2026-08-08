# Nexus Runtime

`nexus-runtime` is the headless Connect host binary: it serves the
spoke-connect invoke surface against the shared `~/.nexus42` home and the
active workspace database, with no daemon HTTP router, no embedded web UI,
and no Setup/Canvas/Control Room. It is the integration surface for
third-party reasoners that talk to a nexus installation over spoke-connect
(pinned `=0.9.2`).

`nexus42 connect start` is the in-app equivalent: the same boot
(`connect::build_host_config`), the same invoke surface, the same fail-closed
allowlist and token policy. `nexus-runtime` adds `--home` / `$NEXUS42_HOME`
and prints readiness to stdout; `connect start` prints status to stderr. The
runtime binary has no subcommands — management surfaces (`dial`, `peers
list`, `token issue`) are `nexus42 connect …` commands.

## Build and install

The binary lives in the `apps/nexus42` crate and requires the `connect-host`
feature:

```sh
cargo build --release --bin nexus-runtime --no-default-features --features connect-host
```

`--no-default-features` turns the `web-embed` feature off, so the artifact
carries no embedded SPA bytes. The `connect-host` feature links spoke-connect
(libp2p); the default build stays libp2p-free and does not produce the
binary (`required-features = ["connect-host"]`).

The full `nexus42` CLI exposes the same surface when built with
`connect-host`:

```sh
cargo build --release --features connect-host   # adds `nexus42 connect …`
```

## Run

```sh
nexus-runtime [--listen <MULTIADDR>]... [--allow-peer <PEER_ID>]... [--home <PATH>]
```

| Flag | Meaning |
|------|---------|
| `--listen <MULTIADDR>` | Listen multiaddr, repeatable. Default `/ip4/127.0.0.1/tcp/0` (loopback, ephemeral port). Binding a routable interface is an explicit operator choice. |
| `--allow-peer <PEER_ID>` | Peer IDs to allowlist for this run, repeatable; unioned with `~/.nexus42/connect/allowlist.json`. |
| `--home <PATH>` | Home override: `--home` > `$NEXUS42_HOME` > the user home. The value is the home dir — the parent of the `.nexus42` layout dir (`/home/me` → `/home/me/.nexus42`). Not available on `connect start`. |

Boot sequence (shared with `nexus42 connect start`):

1. PATH enrichment (GUI-launched sidecars inherit a minimal PATH).
2. `~/.nexus42` layout skeleton (idempotent, existing files untouched).
3. `connect::build_host_config`: persisted `connect/identity.key` +
   device-id `host_id` + effective allowlist (fail-closed) + honest
   `HostCapabilityManifest` + active-workspace SQLite pool (WAL) + one
   per-process `NexusAdapter` + the N-C2 invoke dispatch handler.
4. `SpokeConnectNode::start` — the spoke-connect node.

`nexus-runtime` adds one runtime-only step after the shared sequence:

5. Readiness on stdout, then block until Ctrl-C (SIGINT) (`connect start`
   prints status to stderr instead).

Readiness is stdout-only: there is no HTTP health endpoint in this process.

```text
nexus-runtime: Connect Host (N-C2 E2) ready
  peer_id: 12D3KooW…
  host_id: <device-id>
  listen: /ip4/127.0.0.1/tcp/4321
  allowlisted peers: 1 (fail-closed; add via allowlist.json or --allow-peer)
  invokes: upsert/promote/relate/check/assemble/compute served (world+module scoped); project/unknown refused (op_unsupported)
  press Ctrl-C to stop
```

The host refuses to boot without a resolvable active workspace (fail-closed).

## Connect-only surface

The host serves exactly six invoke ops (`invoke::SERVED_OPS`,
machine-checked against the manifest's advertised `served_ops`):

```text
upsert  promote  relate  check  assemble  compute
```

- Everything else — `project`, unknown ops — is refused with
  `op_unsupported`.
- Caller identity is the authenticated session peer (spoke-connect 0.9.2
  `InvokeHandlerV2`); the payload `extensions.nexus.peer_id` is informational
  only and must match the session peer.
- Per-invoke gates, all fail-closed: op scope → world scope (every target
  world in the payload must be in the peer's `world_scope`) → module scope
  for `compute` (`module_scope`; missing/empty denies ALL compute) → the
  host-local module store (`~/.nexus42/modules/`, never peer-supplied
  bytes). `compute` resolves the world from the stored entry inside the
  lane.
- Bounded bridge: 8 concurrent invokes per process, 30 s per-invoke
  deadline, 500 collection entries / 2 MiB request / 2 MiB response.

The daemon HTTP router, embedded SPA, Setup/Canvas/Control Room, ACP, and
schedule/worker supervision never boot in this process (`run_daemon` is
never called).

## Home layout

All state lives under `~/.nexus42/` (path helpers in `nexus-home-layout`):

| Path | Purpose |
|------|---------|
| `config.toml` | CLI/daemon config; active workspace (`active_creator_id` + `active_workspace_slug_by_creator`) |
| `device-id` | Machine identifier (UUID v4); the Connect `host_id` |
| `connect/identity.key` | Connect node Ed25519 identity key (libp2p protobuf; created once, 0600) |
| `connect/allowlist.json` | Peer allowlist + scopes (below) |
| `connect/issuer.key` | Capability-token issuer key (distinct from `identity.key`; created once, 0600) |
| `connect/config.json` | Capability-token operator policy (below) |
| `modules/<id>/<id>.wasm`, `manifest.json` | User-installed compute modules (host-local) |
| `creators/<creator_id>/workspaces/<workspace_slug>/state.db` | Active workspace SQLite DB (WAL) |

The runtime boots against the active workspace selected in `config.toml`;
`connect dial` / `connect peers list` resolve the same DB.

## Coexistence with the creator app

`nexus-runtime` and the creator-facing `nexus42` app share the home and the
workspace DB. Concurrency is governed by SQLite WAL (1 writer + N readers,
`DbPool` busy timeout); the per-work `runtime_lock` is daemon-internal and
the Connect invoke path never acquires it. The daemon may run while the
runtime serves invokes.

## Allowlist and module scope

`~/.nexus42/connect/allowlist.json` is the trust root. Each `peer_ids`
entry is either a bare peer id (no op access) or a scoped object:

```json
{
  "peer_ids": [
    "12D3KooW…",
    {
      "peer_id": "12D3KooW…",
      "world_scope": ["<world-uuid>"],
      "op_scope": ["upsert", "promote", "relate", "check", "assemble", "compute"],
      "module_scope": ["<module-id>"]
    }
  ]
}
```

- Bare entry, or a `--allow-peer` overlay: handshake-allowlisted but can
  never invoke a served op.
- Scoped entry: `world_scope` / `op_scope` / `module_scope` are optional;
  an absent/empty scope denies world access, ops, and — since P2 — ALL
  compute (fail-closed). `module_scope` ids are host-local module names
  (`~/.nexus42/modules/`), never peer-supplied bytes.
- Missing file ⇒ empty ⇒ fail-closed: every remote peer is rejected at the
  handshake. Malformed file, unparseable peer id, or unknown fields ⇒ hard
  boot error (a typo cannot silently open or lock the host).
- No online enroll endpoint: the operator edits the file out-of-band and
  restarts the host.

The allowlist is mutual: `connect dial` requires the dialed peer to be
allowlisted, because a connected peer can invoke this host on the same
session.

## Peer visibility (N-C3)

`nexus42 connect dial <multiaddr>` is the production outbound dial surface:
it boots a Connect node, dials the peer, and records the dialed peer's
manifest into the active workspace's observed-peer store at `connect()`
return (fail-closed — a dial or record error aborts the command).
`nexus42 connect peers list` reads that store.

```sh
nexus42 connect dial /ip4/127.0.0.1/tcp/4321 --allow-peer 12D3KooW…
nexus42 connect peers list
```

`connect peers list` prints a `HOST_ID` / `CAPABILITIES` / `LAST_SEEN`
header followed by one row per observed peer host; an empty store prints
`no peers observed`. Only outbound dialed peers are recorded — inbound-only
peers are not. `connect start` / `nexus-runtime` never dial; `connect dial`
is the shipped trigger. mDNS is never enabled.

## Capability tokens

Two pieces (V1.155 P1): the operator policy file and the issuance command.

### Operator policy — `connect/config.json`

```json
{
  "trusted_issuers": ["12D3KooW…"],
  "require_capability_token": true,
  "capability_token_provider": {
    "enabled": true,
    "issuer_key_path": null
  }
}
```

| Field | Meaning |
|-------|---------|
| `trusted_issuers` | Issuer peer ids whose tokens this host accepts |
| `require_capability_token` | Sessions must complete the capability-token challenge |
| `capability_token_provider.enabled` | Mint outbound proofs on demand at challenge time; the issuer key must exist at boot (missing key ⇒ fail-closed boot error) |
| `capability_token_provider.issuer_key_path` | Optional override of `~/.nexus42/connect/issuer.key`; relative paths resolve against `~/.nexus42/connect/` |

An absent file yields the defaults (empty / false / disabled — pre-V1.155
behavior). A malformed file, unknown fields, or
`require_capability_token: true` without `trusted_issuers` is a boot error
(fail-closed, no silent defaults).

### Issuance — `connect token issue`

```sh
nexus42 connect token issue \
  --sub <PEER_ID> --aud <PEER_ID> \
  --capabilities <C1,C2> --exp <UNIX_SECONDS> [--iss <PEER_ID>]
```

| Flag | Meaning |
|------|---------|
| `--sub <PEER_ID>` | Subject — who may present the token |
| `--aud <PEER_ID>` | Audience — the verifying node |
| `--capabilities <C1,C2>` | Comma-separated capability names granted to `sub` (non-empty) |
| `--exp <UNIX_SECONDS>` | Expiry, Unix time seconds (UTC); must be beyond now + 60 s clock skew |
| `--iss <PEER_ID>` | Issuer override; defaults to the issuer key's derived peer id and MUST equal it |

Loads or creates `~/.nexus42/connect/issuer.key` (create-once, 0600 — a
distinct trust role from `identity.key`) and prints the signed wire proof
`{v, claims, sig}` as JSON on stdout (no secrets echoed).

## Next steps

- [Strategy authoring](strategy-authoring.md) — external strategy format,
  trigger/scheduled lanes, prompt templates, validator.
- [Module authoring](module-authoring.md) — WASM ABI, `manifest.json`
  (incl. `wasm_sha256`), `module_scope` allowlist, operator install.
- [Integrator walkthrough](../strategy-samples/README.md) — worked example
  end to end.
- [Docs index](README.md) — all docs.
- ABI spec: [`.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md).

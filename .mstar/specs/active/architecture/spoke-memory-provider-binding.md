# Spoke Memory Provider Binding

> **Status:** Active architecture contract.
> **Document class:** Master
> **Scope:** Nexus memory-provider selection, open-scoped authority binding, secure remote Connect admission, local/remote behavior separation, and conservative remote-binding removal protection.
> **Related:** [Spoke Adapter Architecture](spoke-adapter-architecture.md) and [Spoke Memory Conformance Staging](spoke-memory-conformance-staging.md).

## 1. Architecture and ownership

A `CoreService` open pins exactly one `MemoryProviderBinding` for its entire lifetime. `LocalMemoryProvider` remains the implicit default. A binding owns:

- one `Arc<dyn MemoryPort + Send + Sync>` used by all standard memory operations;
- the selected `space_id` and `(authority_host_id, authority_generation)`;
- writability derived from the open's access mode or local provider;
- the actual public Connect client identity for a remote binding;
- the concrete local provider or remote adapter handle needed for provider-specific behavior and idempotent close.

Every memory handler and `CoreCharacterMind` receives the same binding. A live open never rereads configuration, changes provider, or changes authority. Closing and reopening is the only provider-switch operation; it is not transfer, migration, or distributed cutover.

Nexus is a Connect memory client only. It does not add a Nexus memory responder, advertise `spoke-memory` as a served Nexus family, or create a second wire protocol.

**Ablation: `MemoryProviderBinding`** — removal would either expose provider-local storage to generic callers or make local legacy operations indistinguishable from remote protocol operations; it is retained to preserve provider-neutral standard operations and local-only behavior.

**Ablation: `memory-remote`** — removal would either add Connect to the default graph or hide remote selection behind an unrelated execution/Connect-host feature; it is retained as the minimal opt-in client dependency boundary.

## 2. Selection contract

### 2.1 Public Rust surface

`nexus_core` exports `MemoryProviderSelection` and `RemoteMemoryProviderSelection`. `CoreOpenOptions` has the following selection surface:

```rust
pub struct CoreOpenOptions {
    pub user_home: PathBuf,
    pub access: CoreAccess,
    pub memory: Option<MemoryProviderSelection>,
}

pub enum MemoryProviderSelection {
    Local,
    Remote(RemoteMemoryProviderSelection),
}

pub struct RemoteMemoryProviderSelection {
    pub space_id: String,
    pub credential_id: String,
    pub connect_endpoint: String,
    pub remote_pubkey_hex: String,
    pub allowlist: Vec<String>,
}
```

`MemoryProviderSelection` and `RemoteMemoryProviderSelection` have secret-safe `Debug` output. The remote representation may show non-secret ids, a sanitized endpoint, public keys, and peer ids, but never credential-file contents, identity seeds, capability proofs, or token material. `MemoryCredential` has a redacted `Debug` implementation and no public secret accessor.

`NativeOpenOptions` and its generated TypeScript contract have no parallel provider-selection DTO. Native and CLI opens pass `memory: None` and therefore use shared on-disk selection. An integrator constructing `CoreOpenOptions` directly may pass an explicit open-only override.

### 2.2 Precedence and persistence

Selection follows this order:

1. `CoreOpenOptions.memory = Some(selection)` supplies a validated open-only override and neither reads nor writes provider configuration. `Some(Local)` therefore overrides a remote disk selection for that open.
2. `None` reads `[memory_provider]` from `<user_home>/.nexus42/config.toml` exactly once while opening.
3. A genuinely absent config directory entry or `kind = "local"` selects local; existing homes retain local behavior. A config path that exists but cannot be inspected or read — an existing symlink whose target is gone, any other filesystem error, or a malformed document — fails the open instead of selecting local.

A remote table contains only:

```toml
[memory_provider]
kind = "remote"
space_id = "<space id>"
credential_id = "<credential file id>"
connect_endpoint = "<ws or wss endpoint>"
remote_pubkey_hex = "<64 lowercase hex characters>"
allowlist = ["<trusted peer id>"]
```

`MemoryProviderConfig` in `nexus-home-layout` is the single typed on-disk shape. `read_memory_provider_config(nexus_root)` reads the optional table independently of `CliConfigSnapshot`. A malformed provider table fails provider selection without changing active-creator or workspace routing. Unknown tables continue to round-trip through the whole-document configuration representation.

`CoreHomeService::set_memory_provider_selection` validates and writes a selection through the existing `with_config_lock` whole-document writer. `clear_memory_provider_selection` removes the table and restores the implicit local default. CLI `creator memory provider show|use local|use remote` uses these shared readers and writers and accepts no secret argument, stdin value, or environment value.

Selection validation enforces:

- `space_id`: 1–128 characters, with no control characters or whitespace;
- `credential_id`: `[A-Za-z0-9][A-Za-z0-9_-]{0,63}`;
- `remote_pubkey_hex`: exactly 64 lowercase hexadecimal characters;
- `allowlist`: at least one non-empty peer id without whitespace;
- `connect_endpoint`: the raw transport-compatible URI rules in §3.2.

Changing the disk table affects only subsequent opens. No open polls or retargets its provider.

## 3. Credential and endpoint discipline

### 3.1 Credential reference

`credential_id` resolves to:

```text
<user_home>/.nexus42/memory-credentials/<credential_id>.json
```

The id is validated before path construction. The loader refuses symlinks/reparse points, non-regular files, files larger than 16 KiB, and Unix permissions broader than `0600`. On platforms with a supported no-follow open, type, size, and permission checks use the opened descriptor; platforms without a race-free no-follow open fail closed. Reads remain bounded even if the file grows after metadata inspection.

The operator provisions the JSON file out of band:

```json
{
  "identity_seed_hex": "<64 lowercase hexadecimal characters>",
  "capability_token": null
}
```

`identity_seed_hex` decodes to exactly 32 bytes. `capability_token` is required and nullable. When non-null, it is the published `CapabilityTokenProof` object `{v, claims, sig}` with:

- string `iss`, `sub`, and `aud` claims;
- a string-array `capabilities` claim;
- unsigned integer `exp`;
- optional typed `iat` and `jti`;
- no unknown proof or claim fields.

Base core validates that serialized shape without importing Connect. The remote feature deserializes it again through the published Connect type. This is structural validation, not token issuance or cryptographic verification. Nexus adds no issuer, refresh flow, cloud account, generic `auth.json` credential map, or reuse of user/Creator tokens as Connect capability tokens.

The seed and proof exist only in the credential loader and remote-adapter construction. They are not logged, returned through public APIs, included in error text, or persisted into configuration, receipts, journals, exports, or dependency metadata. Credential failures use static `memory_credential_invalid` errors.

**Ablation: the core-owned credential-file loader** — removal would require importing CLI `AuthStore` or cloud-sync into core, or placing secrets in configuration; it is retained for authenticated Connect without a forbidden dependency edge.

### 3.2 Endpoint

`connect_endpoint` is validated as the exact raw absolute WebSocket URI passed to `tokio_tungstenite::connect_async_with_config`; validation does not accept a normalized host while retaining an unusable original string. It rejects:

- userinfo, including empty userinfo;
- query and fragment components;
- missing hosts and invalid or zero ports;
- schemes other than `ws` and `wss`.

`ws://` is permitted only for `127.0.0.1`, `::1`, or `localhost`, not the whole `127/8` range. Every non-loopback endpoint requires `wss://`.

The remote implementation reuses `WsTransport`, the product envelope-size configuration, `connect_remote_adapter`, and rustls WebPKI roots for TLS. It adds no HTTP proxy or alternate protocol.

## 4. Remote admission and binding authority

### 4.1 Open sequence

`memory-remote` opens a remote binding in this order:

1. Load the selected credential and derive the public Ed25519 Connect peer id from the same seed used for the signed hello. This public id, not `credential_id`, a path, profile name, seed hash, or payload principal, becomes `client_identity`.
2. Connect to the exact endpoint with a ten-second connection bound and construct `RemoteAdapterOptions` from the loaded identity and non-secret selection.
3. Perform signed hello, remote-key and allowlist checks, and require negotiated `spoke-memory`.
4. Call `memory.describe` and require `memory_protocol_version = 1`; reflection and transfer are not prerequisites.
5. Walk at most eight bounded `memory.spaces.list` pages using the advertised page-size limit. Select exactly the configured `space_id`; zero or multiple matches fail.
6. Require the remote manifest `host_id` to equal the selected space's `authority_host_id` and pin its `authority_generation`.

The adapter ignores the client-side `Principal` argument as authority. The provider's `MemoryAdmission` maps the authenticated `peer_id` to its stable principal; payload creator/principal fields do not grant identity. Reopening with the same identity seed presents the same public peer id and therefore reaches the same server-side principal and deduplication namespace.

There is no automatic in-place reconnect. A failed session closes. A later close/open creates a fresh binding and reads the then-current authority generation. Old-authority and stale-generation cutover writes remain N4-T C-089 work specified in [Spoke Memory Conformance Staging](spoke-memory-conformance-staging.md#4-durable-n4-roadmap).

### 4.2 Internal binding surface

`crates/nexus-core/src/memory_binding.rs` owns the crate-private binding. Its behavioral interface is:

```rust
pub(crate) fn port(&self) -> &(dyn MemoryPort + Send + Sync);
pub(crate) fn space_id(&self) -> &str;
pub(crate) fn authority(&self) -> (&str, NonZeroU64);
pub(crate) fn client_identity(&self) -> Option<&str>;
pub(crate) fn local(&self) -> CoreResult<&Arc<LocalMemoryProvider>>;
pub(crate) fn is_local(&self) -> bool;
pub(crate) fn is_writable(&self) -> bool;
pub(crate) async fn close(&self);
```

Local binds its one default space with local authority generation `1`. Remote binds only after the complete admission sequence succeeds. The authority pair and client identity are immutable crate-private facts used for behavior and dependency namespacing, not a new public DTO.

### 4.3 Failure, cancellation, and close

`CoreInner.memory` is `Mutex<Option<Arc<MemoryProviderBinding>>>`. Feature-off remote selection returns `memory_remote_disabled` before reading credentials or opening a socket. Any credential, transport, handshake, capability, space, or authority failure fails the whole open and never substitutes local.

An established but not-yet-pinned adapter has an ownership guard that closes it if admission fails or the open future is cancelled. After workspace-pool admission, failed provider pin closes only that open's pool and releases only admissions owned by that failed open. It does not evict a concurrent joiner or another open's admission.

Close first applies the existing execution/activity drain and closing barrier, takes the one binding, and closes only that binding's local provider pools or remote adapter session. Local and remote close are idempotent. Close never chooses another provider, closes another open's pool, or transfers data.

SQLite admission lifetime is tied to actual protected users rather than pool-close notification alone. Checked-out, transaction, detached, and raw-clone connections retain the original migration and EngineOwner OS-admission resources through native connection disposal. The open-pool plane separately retains the exact published `WorkspaceWriterGuard`, preserving identity and in-process joinability when a usable pool temporarily has no native connections. Locked close-start notification retires that open plane synchronously through runtime-independent wakeup; native pins retire only at real disposal. Callback ownership is weak and acyclic. Construction failure or cancellation closes only the undelivered owned pool, and registry cleanup does not depend on a live Tokio runtime.

## 5. Provider-neutral and local-only behavior

`MemoryAdapter` stores `Arc<MemoryProviderBinding>` and uses `binding.space_id()` rather than assuming the local default space. `CoreAccess::ReadOnly` skips subject ensure and refuses writes before any port mutation.

| Concern | Contract for every provider | Local-only internals |
|---|---|---|
| Spaces, subjects, grants, capture, candidates, records, recall, profile, receipts | Use upstream `MemoryPort`; remote calls execute through `RemoteAdapter`. | None |
| Legacy merged reads and deterministic `lgc:` ids | Remote reads only remote-provider objects; selection does not merge local legacy content. | Scoped record listing, legacy projection, completion marker |
| Candidate dismiss/review | Remote uses `dismiss_candidate` and `review_candidate`; provider transaction supplies atomicity. | JIT adoption and legacy candidate dismiss/review |
| Review-drain exclusion | Nexus authorization and `ActorActivityLease` remain; remote relies on provider CAS/atomic review. | Memory disposition and legacy source-row rechecks |
| Record promotion and SOUL projection | Remote executes `promote_record`, `get_profile`, and ordinary record reads without skipping the requested operation. | File journal and soul-invalidation bindings |
| Legacy migration | Remote returns structured `not_supported` with no effects or implicit transfer. | `migrate_legacy` |

The local provider retains its legacy bridge, review locks, JIT adoption, writer guards, journals, and invalidation. Local-only functions are reached only after an explicit local branch or through `binding.local()` where a concrete handle is required. Generic remote operations are not replaced with blanket refusals.

Local content remains untouched and becomes visible again when a later open selects local. No dual-read, implicit migration, or remote fallback occurs.

### 5.1 Canonical bearer ownership

Within one space, a second distinct subject naming an already-named `(kind_ns, kind_id)` legacy bearer is rejected with typed `INVALID_INPUT` inside the ensure transaction before insertion. This prevents smallest-id reselection from resurrecting a consumed legacy projection. List and by-id reads resolve the same canonical subject. Dismissed provider-held legacy ids stay consumed; only a genuinely new subject/bearer receives a new grant.

## 6. Conservative remote-attempt and dependency protection

Remote capture protection is content-free and per attempt. An unprovable historical attempt may keep its Nexus binding non-removable indefinitely, even after later retries succeed. This conservative blocking is intentional, not an automatic recovery success.

Before each Core-admitted binding-scoped remote capture, Nexus commits an independently identified admission in the same short writer transaction as provenance validation. The transaction never remains open across the RPC.

Only the definite result of that exact wire attempt retires that admission:

- a definite committed capture atomically publishes its complete candidate references and retires its own admission;
- a definite no-effect refusal retires its own admission;
- cancellation, transport/commit uncertainty, or failed local outcome persistence retains protection;
- no shared counter, folded state, or seen-receipt inference clears another attempt;
- later redelivery is a new admission and does not prove an older unknown outcome.

Uncertain mutation outcomes resolve only through a replay carrying the same provider-scoped idempotency key and identical payload, or through `memory.receipts.get`. A changed payload under the same key is an idempotency conflict. Nexus never invents a new key to settle an old attempt, force-clears an unknown admission, or replays a capture as part of binding removal.

Known provider candidate/record dependencies are stored separately from unknown admissions. Facts are namespaced by actual provider authority, space, loaded public client identity, Nexus bearer, and binding. Live references and monotone consumed/released facts survive acknowledgement-before-publication ordering. Candidate review atomically transfers known dependencies to returned record ids; shared promotion releases the record. Acknowledgements are applied by the actual shared wire senders, not only a legacy-shaped route. A committed dismissal acknowledgement is bound to its own request — the issued space, idempotency key, candidate and `dismissed` outcome — before the known candidate dependency is released; a mismatched or stale acknowledgement releases nothing.

Removal may use positive provider object state to close a known reference. Negative, concealed, or refused reads prove neither absence nor settlement of an unknown admission. Positive-closure updates are monotone or snapshot-conditional. Final binding deletion checks both unknown admissions and live references in one transaction, then may discard terminal bookkeeping and delete the binding atomically. Existing local dependency checks, ownership/activity leases, and valid no-memory deletion remain intact. Stale outcome or acknowledgement writes cannot revive or erase another admission/reference.

No memory text, request bodies, identity seeds, or proofs are copied into recovery metadata. Local provider storage, legacy rows/files, journals, SOUL behavior, shared capture behavior, and default read-only behavior remain unchanged.

## 7. Feature and dependency boundary

`memory-remote` is the smallest opt-in client cohort. It enables only the optional `spoke-connect` remote adapter, `tokio-tungstenite`, `futures-util`, and `ed25519-dalek` edges required by this client. Base core retains pure endpoint and credential-file validation without a Connect edge.

Under `memory-remote`, `connect/mod.rs` exposes `ws_transport`; accept/session/peer-control modules remain under their existing cohorts, and `memory_remote.rs` is the remote client module. The feature does not enable `execution`, `connect-host`, `connect-client`, rmcp, cloud-sync, or cloud-domain. The pre-existing `nexus-cloud-sync legacy-sync` dependency is unrelated and unchanged.

Default `nexus-core` has no Connect edge. Dependency review compares feature-off and feature-on graphs rather than asserting that unrelated workspace crates never contain cloud dependencies.

The current published SPOKE lockstep is `0.15.2` with `libp2p 0.57.1`; workspace manifests own the exact pins. Consumers use the published interfaces and do not substitute sibling checkouts, unpublished packages, vendored forks, or invented public APIs. A future incompatible upstream interface requires an architecture revision rather than a parallel protocol or source-gate waiver.

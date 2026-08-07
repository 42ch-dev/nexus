# Nexus Integrator Guide — Strategy Samples + Headless Runtime + WASM Modules + Connect SDK

This is the partner-facing quickstart for the Nexus MVP loop: install the
headless runtime, fork and edit a strategy, validate it, place a WASM compute
module, link the `@42ch/spoke-connect` SDK, and run the demo import loop.

This repository ships three things you use together:

| Piece | Where | What it is |
|---|---|---|
| Headless runtime | `nexus-runtime` binary (see [Install the runtime](#1-install-the-headless-runtime)) | A Connect-only daemon that serves the N-C1 write-op invoke surface (`upsert` / `promote` / `relate`, world-scoped) against your World KB. No creator UI, no daemon HTTP router. |
| Strategy sample | [`game-narrative/`](./game-narrative/) | A forkable strategy bundle: capability routing + prompt templates for trigger and scheduled lanes. Nothing here is compiled into any binary. |
| Validator | [`validate.sh`](./validate.sh) | One command, daemon-free: runs the real validator core on any strategy directory. |
| WASM modules | `~/.nexus42/modules/` (see [Place a WASM module](#4-place-a-wasm-compute-module)) | User-installed compute modules, loaded at creator-daemon boot. |
| Connect SDK | `@42ch/spoke-connect@0.9.1` (npm) | Your backend's connection + invoke surface to the runtime. |

**The division of labor (read this first).** The strategy declares *capability
routing and prompt templates* — it does not execute on the runtime. Your
backend runs the LLM step (using the templates), and writes the resulting
KnowledgeEntry drafts into the World over Connect. The runtime owns the World
KB and the write ops; your backend owns the orchestration and the game.

**Compute-over-Connect is an E2 preview.** In E1, `nexus-runtime` serves only
`upsert` / `promote` / `relate`. Every other op (`check` / `assemble` /
`project` / `compute`, or anything unknown) is refused with
`ErrorEnvelope.code = "op_unsupported"` and zero side effects. Compute
invocation over Connect (and the deeper interop quickstart) lands in **E2**.

## The MVP loop at a glance

1. Download and verify `nexus-runtime-<os>-<arch>.zip`; check `--version`.
2. Prepare the home (`~/.nexus42`, or `--home` / `NEXUS42_HOME` override).
3. Fork `game-narrative/` and edit the manifest + templates.
4. Validate with one command: `./validate.sh [STRATEGY_DIR]`.
5. Build/place a WASM compute module under `~/.nexus42/modules/<id>/`.
6. `npm install @42ch/spoke-connect@0.9.1` and allowlist your peer.
7. Start the runtime; connect from your backend; run the
   `upsert` → `promote` → `relate` import loop.
8. Verify writes with the read path (`nexus42 creator world kb list`).

---

## 1. Install the headless runtime

The runtime is distributed as a per-platform zip, built by the
`.github/workflows/runtime-build.yml` workflow (triggered manually via
`workflow_dispatch` or by pushing a `runtime-v*` tag — artifacts are
uploaded to the workflow run's **Artifacts** section):

| Platform | Artifact |
|---|---|
| Windows x64 | `nexus-runtime-windows-x64.zip` (contains `nexus-runtime.exe`) |
| macOS arm64 | `nexus-runtime-macos-arm64.zip` (contains `nexus-runtime`) |
| Linux x64 | `nexus-runtime-linux-x64.zip` (contains `nexus-runtime`) |

Each zip ships with a matching SHA-256 file: `nexus-runtime-<os>-<arch>.zip.sha256`.
Verify before running:

```bash
# macOS / Linux
sha256sum -c nexus-runtime-macos-arm64.zip.sha256
# or on macOS: shasum -a 256 -c nexus-runtime-macos-arm64.zip.sha256
# Windows (PowerShell)
Get-FileHash nexus-runtime-windows-x64.zip -Algorithm SHA256
```

Unzip (the binary sits at the zip root) and smoke-test:

```bash
unzip nexus-runtime-macos-arm64.zip
./nexus-runtime --version   # prints "nexus-runtime <version>"
```

> **Unsigned artifact note.** Installer, codesigning, and auto-update are E2+.
> The zip contains a bare, unsigned binary. On macOS, Gatekeeper may quarantine
> it (`xattr -d com.apple.quarantine ./nexus-runtime` if it refuses to run);
> Windows SmartScreen may show a warning for the unsigned `.exe`.

### CLI surface

```
nexus-runtime --version
nexus-runtime --listen /ip4/127.0.0.1/tcp/0      # repeatable; default loopback, ephemeral port
nexus-runtime --allow-peer <PEER_ID>             # repeatable; unioned with allowlist.json
nexus-runtime --home /path/to/home               # home override (see below)
NEXUS42_HOME=/path/to/home nexus-runtime         # same override via env
```

- **Liveness is a stdout line** — there is no HTTP health endpoint. When the
  runtime is ready it prints `nexus-runtime: Connect Host (N-C2 read half) ready`,
  followed by its `peer_id`, `host_id`, listen addresses, the allowlisted-peer
  count, and the served ops. Press Ctrl-C to stop.
- **`--home` / `NEXUS42_HOME` semantics:** the value is the *home directory
  itself — the parent of the `.nexus42` layout dir* (e.g. `/home/me` →
  `/home/me/.nexus42`), not the layout dir. `--home` wins over `NEXUS42_HOME`,
  which wins over the user home.

### First run: prepare the home

The runtime shares `~/.nexus42` with the creator-facing `nexus42` app
(`config.toml`, workspace SQLite, presets, modules). It **fails closed without
an active workspace** — prepare the home first, either with the creator app's
first-run setup, or with the CLI (`nexus42 creator register` +
`nexus42 creator workspace init` — both write the `config.toml` keys the
runtime resolves). For hermetic/test setups, point `--home` / `NEXUS42_HOME`
at an isolated directory and prepare it the same way.

### Home layout (`~/.nexus42`)

| Path | Contents |
|---|---|
| `config.toml` | Active creator + workspace resolution |
| `device-id` | Installation device id (the Connect `host_id`) |
| `connect/identity.key` | Connect host Ed25519 identity (created on first boot) |
| `connect/allowlist.json` | Peer allowlist + per-peer world/op scope (see [Link the SDK](#5-link-the-connect-sdk)) |
| `modules/<id>/` | User-installed WASM compute modules (see [Place a WASM module](#4-place-a-wasm-compute-module)) |
| `presets/<id>/` | User-installed presets (3-tier resolution: user overrides embedded) |
| `creators/<creator_id>/workspaces/<slug>/state.db` | Workspace SQLite (WAL mode; coexistence with the creator daemon is WAL-governed) |

---

## 2. Fork the strategy sample

Copy the bundle anywhere you own (it is a plain directory; nothing under
`strategy-samples/` is embedded or required at build time):

```bash
cp -R strategy-samples/game-narrative my-strategy
```

**One invariant:** the validator enforces that the bundle directory name
equals `preset.id` in the manifest (`check_bundle_id_vs_directory`). Either
keep the directory named `game-narrative`, or edit
`preset.id: my-strategy` (and the `description`) in `my-strategy/preset.yaml`
after copying — validation fails otherwise.

The bundle is the standard preset format (`preset.yaml` + `templates/`). The
sample demonstrates the two orchestration patterns you choose between per
game:

- **Trigger lane** — a game event arrives and is turned into an
  extraction/assembly task (worldview + character-sheet → KnowledgeEntry
  drafts + Relation hints). Entry: the run payload's `mode` selector, routed
  by a judge (`templates/lane-route.md`).
- **Scheduled lane** — an interval sweep (`min_interval: "PT1H"`) that
  inventories new/changed worldview + character documents since the last
  watermark, judges whether anything new exists, and only then runs the same
  extraction templates. The scan + judge live in the `scheduled_sweep` state;
  extraction is a separate `sweep_extract` state (the embedded `research`
  preset's scanning → extracting pattern), so an interval park never re-runs
  extraction.

The templates you will edit:

| Template | Purpose |
|---|---|
| `templates/lane-route.md` | Lane selector judge (reads `{{preset.input.mode}}`) |
| `templates/trigger-game-event.md` | Trigger lane: game event → extraction/assembly task |
| `templates/scheduled-sweep.md` | Scheduled lane: watermark-aware document inventory |
| `templates/scheduled-sweep-exit.md` | Judge: GO if new material, else wait until the next interval |
| `templates/import-worldview.md` | Worldview document → KnowledgeEntry draft set |
| `templates/import-character-sheet.md` | Character sheet → KnowledgeEntry drafts + Relation hints |

The run payload contract (`preset.input.*`) and the customization points
(including the watermark ownership note — the engine does not persist it) are
documented in `preset.yaml` and the templates themselves. Edit prompts,
triggers, and cadence freely; keep the manifest schema shape intact so the
validator stays green.

---

## 3. Validate

The validator runs the **real validator core** (semantic + assets + path
safety) with no daemon required:

```bash
./strategy-samples/validate.sh                     # validates the bundled sample
./strategy-samples/validate.sh my-strategy         # validates your fork
```

Requires the `nexus42` CLI on `PATH` (build it once from the repo root:
`cargo build --bin nexus42`). Exit status: `0` when the strategy validates
clean, non-zero otherwise — the script is `set -euo pipefail` and delegates to:

```bash
nexus42 system preset validate --offline my-strategy
```

`--offline` runs the daemon's exact validation composition in-process (no
daemon — `nexus-runtime` does not serve the daemon HTTP router, so you can
validate on any machine that has the CLI). The path may be a bundle directory,
a `preset.yaml` file, or a standalone YAML file (standalone skips asset
checks). Machine-readable output:

```bash
nexus42 system preset validate --offline --json my-strategy
# {"errors":[],"id":"game-narrative","state_count":5,"valid":true,"version":1}
```

Without `--offline`, the command delegates to the creator daemon
(`POST /v1/daemon/presets:validate`) — for that mode a running `nexus42`
daemon is required. See [Validation behavior notes](#validation-behavior-notes)
for the differences.

---

## 4. Place a WASM compute module

Compute modules are stateless `wasm32-unknown-unknown` pure functions that
read a `ComputeInput` envelope and return a 4-part `ComputeOutput` envelope.
User-installed modules live under:

```
~/.nexus42/modules/<id>/<id>.wasm
~/.nexus42/modules/<id>/manifest.json
```

(`<id>` must match the directory name and the manifest's `module_id`.) The
creator-facing daemon scans this directory at boot (single-level walk; a
module with a missing/mismatched `<id>.wasm` or an invalid `manifest.json` is
skipped with a warning — one bad module does not block startup).

Author a module with:

- **ABI contract:** [`.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md)
  — exports (`memory`, `alloc`, `compute`, optional `init`), the host-import
  whitelist (`nexus::kb_read`, `nexus::narrative_query`), the marshalling
  convention, and the `manifest.json` contract.
- **Authoring guide + reference implementation:**
  [`modules/README.md`](../modules/README.md) and
  [`modules/basic-combat/`](../modules/basic-combat/) (`manifest.json`,
  `Cargo.toml`, `src/lib.rs`) — copy its `alloc`/`compute` marshalling and
  replace the combat logic.

Build with `cargo build --release --target wasm32-unknown-unknown` (install
the target first: `rustup target add wasm32-unknown-unknown`), then copy the
artifacts into place:

```bash
mkdir -p ~/.nexus42/modules/basic-combat
cp target/wasm32-unknown-unknown/release/basic_combat.wasm ~/.nexus42/modules/basic-combat/basic-combat.wasm
cp modules/basic-combat/manifest.json ~/.nexus42/modules/basic-combat/
```

> The repo's own `basic-combat` is also embedded into the `nexus-wasm-host`
> crate at build time from the same source; installing it as a *user* module
> is optional and exercises the exact user-module path described here.

---

## 5. Link the Connect SDK

The TypeScript SDK is published as **`@42ch/spoke-connect`** (this is the
canonical name — not `spoke-connect-ts`), pinned exactly to **0.9.1**:

```bash
npm install @42ch/spoke-connect@0.9.1
```

### Allowlist your peer on the host

The runtime only accepts peers it has allowlisted (missing file ⇒ empty ⇒
fail-closed: every remote peer is rejected at the handshake). Allowlist the
peer id **your SDK node** uses in
`~/.nexus42/connect/allowlist.json` on the host machine:

```json
{
  "peer_ids": [
    {
      "peer_id": "12D3KooW...",
      "world_scope": ["wld_..."],
      "op_scope": ["upsert", "promote", "relate"]
    }
  ]
}
```

- `world_scope` is the list of world UUIDs (e.g. `wld_abc123`, as shown by
  `nexus42 creator world list`) the peer may write. **Absent ⇒ no world write
  access** (fail-closed single-tenant default).
- `op_scope` is the subset of served ops the peer may invoke. Absent ⇒ no ops.
- A bare string entry (`"peer_ids": ["12D3KooW..."]`) is the N-C0 shape —
  handshake-only, no write scope.
- `--allow-peer <PEER_ID>` on the runtime CLI unions with this file.

> **Caller identity = the session peer.** The per-invoke caller `peer_id` is
> the authenticated Connect session peer (spoke-connect 0.9.2
> `InvokeHandlerV2`), not the payload. A payload that still carries
> `extensions.nexus.peer_id` must have it equal the session peer; a differing
> or unparseable claim is denied in full (`op_unsupported`, zero side
> effects). Per-peer `world_scope` / `op_scope` scoping is therefore
> authentic for any number of allowlisted peers.

### Start the runtime

```bash
./nexus-runtime --allow-peer 12D3KooW...          # or rely on allowlist.json alone
```

Note the printed `listen:` multiaddr (default `/ip4/127.0.0.1/tcp/<port>` —
loopback only; binding a routable interface is an explicit operator choice).
Your SDK node dials that address.

### The served invoke surface (N-C1)

The runtime serves exactly three write ops (everything else →
`ErrorEnvelope.code = "op_unsupported"`, zero side effects):

| Op | Payload (wire) | Effect |
|---|---|---|
| `upsert` | `{ "extensions": { "nexus": { "peer_id": "..." } }, "knowledge_entries": [ ... ] }` | Create/update KnowledgeEntries (OCC via the entry's `revision` — see below) |
| `promote` | `{ "extensions": { "nexus": { "peer_id": "..." } }, "candidate": { ... } }` | Transition a `provisional` entry to `confirmed` |
| `relate` | `{ "extensions": { "nexus": { "peer_id": "..." } }, "relation": { "schema_version": 1, "relation_id": "...", "relation_type": "...", "from_id": "...", "to_id": "...", "extensions": { "nexus": { "world_id": "wld_..." } } } }` | Create/update typed relations between entries (OCC) |

Every entry/relation must carry `extensions.nexus.world_id`; a payload missing
a world id, claiming a world outside the peer's `world_scope`, or replaying a
stored world mismatch is denied **in full** with `op_unsupported` and zero
side effects. `relate` additionally requires both endpoints to exist in the
claimed world.

**OCC — optimistic concurrency.** Writes are CAS-guarded on the stored
revision. Send the last-known `revision` on the entry/relation when updating.
Rejects map to the `ErrorEnvelope` (P1 locked table):

| `ErrorEnvelope.code` | Meaning | Retry-safe? |
|---|---|---|
| `knowledge_entry_already_exists` | Create path hit an existing id (re-read, update) | yes |
| `stored_revision_stale` | Stored revision is newer than your base (re-read, retry) | yes |
| `revision_conflict` | Your base is ahead of the store (re-read, retry) | yes |
| `internal_error` | Server fault (details carry the reject) | no |

`reject.message` flows into `ErrorEnvelope.message`; `reject.details` (when
present) into `ErrorEnvelope.details`.

---

## 6. Run the demo loop

The full MVP loop, end to end:

```bash
# 1. Runtime up (see §1–§5)
./nexus-runtime --allow-peer 12D3KooW...

# 2. Validate your strategy (see §3)
./strategy-samples/validate.sh my-strategy

# 3. From your backend: connect via @42ch/spoke-connect@0.9.1 and
#    write the extraction drafts — upsert provisional entries,
#    promote verified drafts to confirmed, relate the characters.
#    Wire payloads + OCC retry semantics: see §5 and the extraction
#    templates (game-narrative/templates/import-worldview.md,
#    game-narrative/templates/import-character-sheet.md).

# 4. Verify the writes on the host (read path)
nexus42 creator world kb list wld_...
```

The extraction templates (`import-worldview.md`, `import-character-sheet.md`)
define the KnowledgeEntry draft shape (schema_version, entry_id, entry_type,
canonical_name, status, body, source_anchor) and the exact SDK-side import
pattern (`upsert` provisional → `promote` → `relate`), including the
`entry_type` vocabulary to use.

**Where the strategy runs.** The preset lanes (trigger/scheduled) execute on
the creator-facing daemon's orchestration engine via its schedule API —
`nexus-runtime` intentionally ships without the daemon HTTP router and without
schedule supervision. In E1 your backend typically drives the strategy side
(its own timer/event loop + LLM step using the templates) and writes results
into the World over Connect; see the run-payload contract in
`game-narrative/preset.yaml` for the schedule shape when you do use the daemon
API.

---

## Validation behavior notes

Two known differences between the daemon-free (`--offline`) path and the
daemon-backed path — neither affects `validate.sh` (which always passes
`--offline`):

- **Directory argument.** `--offline` accepts a bundle *directory* (it
  resolves `<dir>` → `<dir>/preset.yaml`), but the daemon-backed mode cannot:
  the daemon's bundle-root inference only fires for files literally named
  `preset.yaml`, and a directory arg without `--offline` fails with
  `FILE_READ_ERROR`. Pass the `preset.yaml` file path (or use `--offline`)
  when the creator daemon is the validation backend.
- **`--json` on hard failures.** On oversize / YAML parse / depth failures the
  offline path exits 1 with **no JSON body** (the error is printed to stderr),
  whereas the daemon-backed endpoint returns a `valid: false` JSON body for
  the oversize case. Treat a non-zero exit as "could not validate", and prefer
  the offline path for scripted validation.

---

## What's next (E2 preview)

- **Compute-over-Connect invocation** — `compute` over Connect, plus the
  `"reasoning-complete"` semantic milestone (roles `computable-engine` /
  capability `l2-computable`) in the host manifest: **P2 of the E2 wave**.
  `check` / `assemble` are already served (N-C2 read half); `project` stays
  refused with `op_unsupported`.
- Release polish (GitHub Release automation, checksums on the release page),
  installer / codesigning / auto-update, and multi-host / multi-peer session
  identity are E2+.
- The WASM ABI itself is stable and documented now — build modules today; the
  E2 change is the *invocation transport*, not the ABI.

## Reference

- Sample bundle: [`game-narrative/`](./game-narrative/)
- Validator wrapper: [`validate.sh`](./validate.sh)
- WASM compute ABI: [`../.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md)
- Module authoring guide: [`../modules/README.md`](../modules/README.md)
- Reference module: [`../modules/basic-combat/`](../modules/basic-combat/)
- Headless runtime spec: [`../.mstar/specs/daemon-runtime.md`](../.mstar/specs/daemon-runtime.md) §4.6
- Connect invoke surface (N-C1): `apps/nexus42/src/commands/connect/invoke.rs`

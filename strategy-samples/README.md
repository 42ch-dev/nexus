# Nexus Integrator Guide — Headless Runtime + Connect SDK + Strategy Samples + WASM Modules

This is the partner-facing quickstart for the full Nexus E2 loop: install and
boot the headless runtime, connect from your backend with the
`@42ch/spoke-connect` SDK, import world lore, run reasoning (`check` /
`assemble`), invoke a host-local compute module (`compute`), and fork + validate
a turn strategy offline.

This repository ships everything you use together:

| Piece | Where | What it is |
|---|---|---|
| Headless runtime | `nexus-runtime` binary (see [Install and boot the runtime](#1-install-and-boot-the-runtime)) | A Connect-only daemon serving the full N-C2 invoke surface (`upsert` / `promote` / `relate` / `check` / `assemble` / `compute`, world- and module-scoped) against your World KB. No creator UI, no daemon HTTP router. |
| Strategy samples | [`game-narrative/`](./game-narrative/) and [`react-trpg-turn/`](./react-trpg-turn/) | Forkable strategy bundles: capability routing + prompt templates for lore import lanes (game-narrative) and for a TRPG turn loop (react-trpg-turn). Nothing here is compiled into any binary. |
| Validator | [`validate.sh`](./validate.sh) | One command, daemon-free: runs the real validator core on any strategy directory. |
| WASM compute modules | `~/.nexus42/modules/<id>/` (see [Compute](#5-compute-basic-combat-n-c2-compute-half)) | Operator-installed, host-local compute modules (e.g. `modules/basic-combat`) the runtime invokes over Connect. Bytes are never peer-supplied. |
| Connect SDK | `@42ch/spoke-connect@0.9.2` (npm) | Your backend's connection + invoke surface to the runtime (and to any SPOKE connect host). |

**The division of labor (read this first).** The strategy declares *capability
routing and prompt templates* — it does not execute on the runtime. Your
backend runs the LLM step (using the templates), and writes the resulting
KnowledgeEntry drafts into the World over Connect. The runtime owns the World
KB, the write ops, the read/reasoning ops, and the host-local compute modules;
your backend owns the orchestration and the game. The AI understands intent
and narrates; the host-local rules module settles deterministically; the AI
never computes, rewrites, or overrides settlement results.

## The E2 loop at a glance

1. Download and verify `nexus-runtime-<os>-<arch>.zip`; check `--version`.
2. Prepare a home (`~/.nexus42`, or `--home` / `NEXUS42_HOME` override — a
   temp dir keeps a run hermetic).
3. Start the runtime; the readiness line is `nexus-runtime: Connect Host
   (N-C2 E2) ready`.
4. Install the Connect SDK: `npm install @42ch/spoke-connect@0.9.2`, and
   allowlist your peer (with `module_scope` for compute).
5. Import lore: `upsert` → `promote` → `relate` over Connect (N-C1 write ops).
6. Reason: `check` / `assemble` over Connect (N-C2 read half).
7. Compute: install `basic-combat` under `~/.nexus42/modules/`, stage a
   compute session, invoke `compute` (N-C2 compute half — read-only over
   Connect; you persist confirmed results through your own write path).
8. Fork a strategy sample and validate it offline:
   `./strategy-samples/validate.sh my-strategy`.

---

## 1. Install and boot the runtime

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

> **Unsigned artifact note.** Installer, codesigning, and auto-update are
> follow-on packaging work. The zip contains a bare, unsigned binary. On
> macOS, Gatekeeper may quarantine it (`xattr -d com.apple.quarantine
> ./nexus-runtime` if it refuses to run); Windows SmartScreen may show a
> warning for the unsigned `.exe`.

### CLI surface

```
nexus-runtime --version
nexus-runtime --listen /ip4/127.0.0.1/tcp/0      # repeatable; default loopback, ephemeral port
nexus-runtime --allow-peer <PEER_ID>             # repeatable; unioned with allowlist.json
nexus-runtime --home /path/to/home               # home override (see below)
NEXUS42_HOME=/path/to/home nexus-runtime         # same override via env
```

- **Liveness is a stdout line** — there is no HTTP health endpoint. When the
  runtime is ready it prints this readiness block:

```
nexus-runtime: Connect Host (N-C2 E2) ready
  peer_id: <peer-id>
  host_id: <device-id>
  listen: /ip4/127.0.0.1/tcp/<port>
  allowlisted peers: <n> (fail-closed; add via allowlist.json or --allow-peer)
  invokes: upsert/promote/relate/check/assemble/compute served (world+module scoped); project/unknown refused (op_unsupported)
  press Ctrl-C to stop
```

  `served_ops` is also advertised in the host manifest
  (`extensions.nexus.served_ops` — the same six ops, machine-checked against
  the dispatch). Press Ctrl-C to stop.
- **`--home` / `NEXUS42_HOME` semantics:** the value is the *home directory
  itself — the parent of the `.nexus42` layout dir* (e.g. `/tmp/nexus-e2` →
  `/tmp/nexus-e2/.nexus42`), not the layout dir. `--home` wins over
  `NEXUS42_HOME`, which wins over the user home.

### First run: prepare the home

The runtime shares `~/.nexus42` with the creator-facing `nexus42` app
(`config.toml`, workspace SQLite, presets, modules). It **fails closed without
an active workspace** — prepare the home first, either with the creator app's
first-run setup, or with `nexus42 creator workspace init` (with no creator
daemon running, its FS fallback writes the `config.toml` keys the runtime
resolves: `active_creator_id=local`, the workspace slug, and `state.db`).
`nexus42 creator register` is optional and platform-only (auth token +
network, writes `auth.json`) — it is not needed for a local run.

**Home binding.** The `nexus42` CLI resolves its home from `$HOME` only
(`$HOME/.nexus42`); `NEXUS42_HOME` is a nexus-runtime env var. For hermetic/
test setups, export `HOME` at an isolated directory and prepare it the same
way — the runtime resolves the same home (`--home` > `NEXUS42_HOME` > `$HOME`):

```bash
export HOME=/tmp/nexus-e2
nexus42 creator workspace init workspace "E2 demo"         # FS fallback: active_creator_id=local + slug `default` + state.db
nexus42 creator world create --title "E2 demo world"       # creates a world; note the wld_... id
./nexus-runtime --allow-peer 12D3KooW...                   # boot against the same temp home ($HOME/.nexus42)
```

> The workspace slug defaults to `default`; pass `--workspace-slug e2-demo` to
> `nexus42 creator workspace init workspace` to name it. Run `nexus42 creator
> world list` to confirm the world id.

### Home layout (`~/.nexus42`)

| Path | Contents |
|---|---|
| `config.toml` | Active creator + workspace resolution |
| `device-id` | Installation device id (the Connect `host_id`) |
| `connect/identity.key` | Connect host Ed25519 identity (created on first boot) |
| `connect/allowlist.json` | Peer allowlist + per-peer world/op/module scope (see [Link the Connect SDK](#2-connect-with-the-sdk)) |
| `modules/<id>/` | Operator-installed WASM compute modules (see [Compute](#5-compute-basic-combat-n-c2-compute-half)) |
| `presets/<id>/` | User-installed presets (3-tier resolution: user overrides embedded) |
| `creators/<creator_id>/workspaces/<slug>/state.db` | Workspace SQLite (WAL mode; coexistence with the creator daemon is WAL-governed) |

---

## 2. Connect with the SDK

The TypeScript SDK is published as **`@42ch/spoke-connect`** (this is the
canonical name — not `spoke-connect-ts`), pinned exactly to **0.9.2**:

```bash
npm install @42ch/spoke-connect@0.9.2
```

The SDK ships the connect wire family: the core session helpers (`.`), a Node
WebSocket client (`@42ch/spoke-connect/node`), and the message-oriented
RemoteAdapter / MultiPeerRouter (`@42ch/spoke-connect/remote`) that drop into
any consumer transport. The wire contract — the envelopes this walkthrough
shows — is identical on every transport; the runtime itself is a libp2p node
(the Rust `spoke-connect` reference), so a backend that is not itself a
spoke-connect peer reaches it through a transport bridge (see
[References](#7-references) for the TS-side patterns and the connect-demo).

### Allowlist your peer on the host

The runtime only accepts peers it has allowlisted (missing file ⇒ empty ⇒
fail-closed: every remote peer is rejected at the handshake). Allowlist the
peer id **your node** uses in
`${NEXUS42_HOME:-$HOME}/.nexus42/connect/allowlist.json` on the host machine
(the path under the home the runtime resolved at boot; default
`~/.nexus42/connect/allowlist.json`). The scoped entry form:

```json
{
  "peer_ids": [
    {
      "peer_id": "12D3KooW...",
      "world_scope": ["wld_..."],
      "op_scope": ["upsert", "promote", "relate", "check", "assemble", "compute"],
      "module_scope": ["basic-combat"]
    }
  ]
}
```

- `world_scope` is the list of world UUIDs (e.g. `wld_abc123`, as shown by
  `nexus42 creator world list`) the peer may access. **Absent ⇒ no access**
  (fail-closed single-tenant default).
- `op_scope` is the subset of served ops the peer may invoke. Absent ⇒ no ops.
- `module_scope` is the list of host-local compute modules the peer may
  invoke. **Missing or empty ⇒ all compute is denied** (fail-closed; the
  denial is `module_not_scoped`). It is required for the `compute` op.
- A bare string entry (`"peer_ids": ["12D3KooW..."]`) is the N-C0 shape —
  handshake-only, no op scope.
- `--allow-peer <PEER_ID>` on the runtime CLI unions with this file
  (handshake-allowlist only; scoped entries come from the file).

> **Caller identity = the session peer.** The per-invoke caller identity is
> the authenticated Connect session peer (the node that passed the allowlist,
> signed hello, and envelope-auth gates) — never a payload-carried claim. A
> payload that carries `extensions.nexus.peer_id` carries it
> **informationally only**: it must equal the session peer; a differing or
> unparseable claim is denied in full (zero side effects). Omitting the field
> is fine. Per-peer `world_scope` / `op_scope` / `module_scope` scoping is
> therefore authentic for any number of allowlisted peers — there is no
> spoofing path.

### Start the runtime

```bash
./nexus-runtime --allow-peer 12D3KooW...          # or rely on allowlist.json alone
```

Note the printed `listen:` multiaddr (default `/ip4/127.0.0.1/tcp/<port>` —
loopback only; binding a routable interface is an explicit operator choice).
Your peer dials that address.

### The served invoke surface

The runtime serves exactly six ops (everything else — `project`, unknown —
→ `ErrorEnvelope.code = "op_unsupported"`, zero side effects):

| Op | Surface | Effect |
|---|---|---|
| `upsert` | N-C1 write | Create/update KnowledgeEntries (OCC via the entry's `revision` — see below) |
| `promote` | N-C1 write | Transition a `provisional` entry to `confirmed` |
| `relate` | N-C1 write | Create/update typed relations between entries (OCC) |
| `check` | N-C2 read | Run checker(s) over a world scope; returns findings |
| `assemble` | N-C2 read | Assemble a context packet over a world scope |
| `compute` | N-C2 compute | Invoke a host-local WASM module (read-only — see [Compute](#5-compute-basic-combat-n-c2-compute-half)) |

All six are world-scoped; all denials happen before any orchestrator call with
zero side effects. Per-invoke limits: 8 concurrent orchestrator lanes, a
30,000 ms invoke deadline, 500 collection entries / 2 MiB request, 2 MiB
response.

---

## 3. Import lore (N-C1 write ops)

### Wire shapes

| Op | Payload (wire) | Effect |
|---|---|---|
| `upsert` | `{ "extensions": { "nexus": { "peer_id": "..." } }, "knowledge_entries": [ { ... entry with "extensions": { "nexus": { "world_id": "wld_..." } } } ] }` | Create/update KnowledgeEntries (OCC via the entry's `revision` — see below) |
| `promote` | `{ "extensions": { "nexus": { "peer_id": "..." } }, "candidate": { ... entry with "extensions": { "nexus": { "world_id": "wld_..." } } } }` | Transition a `provisional` entry to `confirmed` |
| `relate` | `{ "extensions": { "nexus": { "peer_id": "..." } }, "relation": { "schema_version": 1, "relation_id": "...", "relation_type": "...", "from_id": "...", "to_id": "...", "extensions": { "nexus": { "world_id": "wld_..." } } } }` | Create/update typed relations between entries (OCC) |

Every entry/relation must carry `extensions.nexus.world_id`; a payload missing
a world id, claiming a world outside the peer's `world_scope`, or replaying a
stored world mismatch is denied **in full** with zero side effects. `relate`
additionally requires both endpoints to exist in the claimed world.

A worked `upsert` (the extraction drafts from the game-narrative templates):

```json
{
  "extensions": { "nexus": { "peer_id": "12D3KooW..." } },
  "knowledge_entries": [
    {
      "schema_version": 1,
      "entry_id": "ent_lin_xia",
      "entry_type": "character",
      "canonical_name": "Lin Xia",
      "status": "provisional",
      "body": {
        "summary": "Resourceful smuggler with a debt to the Ashguard",
        "attributes": { "role": "smuggler", "faction": "Ashguard", "age": 24 },
        "tags": ["character_sheet", "protagonist"]
      },
      "extensions": { "nexus": { "world_id": "wld_abc123" } }
    }
  ]
}
```

**OCC — optimistic concurrency.** Writes are CAS-guarded on the stored
revision (and, since the world-aware CAS, on the stored world). Send the
last-known `revision` on the entry/relation when updating. Rejects map to the
`ErrorEnvelope`:

| `ErrorEnvelope.code` | Meaning | Retry-safe? |
|---|---|---|
| `knowledge_entry_already_exists` | Create path hit an existing id (re-read, update) | yes |
| `stored_revision_stale` | Stored revision is newer than your base (re-read, retry) | yes |
| `revision_conflict` | Your base is ahead of the store (re-read, retry) | yes |
| `world_conflict` | Stored row lives in another world than the one you claimed (re-read) | yes |
| `internal_error` | Server fault (details carry the reject) | no |

`reject.message` flows into `ErrorEnvelope.message`; `reject.details` (when
present) into `ErrorEnvelope.details`.

The import pattern is `upsert` provisional drafts → `promote` verified drafts
to `confirmed` → `relate` the characters. The extraction templates
(`game-narrative/templates/import-worldview.md`,
`game-narrative/templates/import-character-sheet.md`) define the exact
KnowledgeEntry draft shape (schema_version, entry_id, entry_type,
canonical_name, status, body, source_anchor) and the SDK-side import pattern
(including the `entry_type` vocabulary to use).

### Verify the writes on the host

```bash
nexus42 creator world kb list wld_abc123
# id / canonical_name / block_type / status
```

---

## 4. Reason: `check` / `assemble` (N-C2 read half)

The read ops ride the same fail-closed world scoping as writes: the request's
`scope.scope_id` is the world id, and the peer's `world_scope` must contain it.

### `check` — run checker(s) over the world

```json
{
  "scope": { "scope_id": "wld_abc123" },
  "extensions": { "nexus": { "peer_id": "12D3KooW..." } }
}
```

`CheckRequest` takes the `scope` (required) plus optional `rule_refs` /
`rules` / `checker_kinds` filters. The response is the findings list:

```json
{
  "findings": []
}
```

The shipped baseline checker evaluates no rules, so a clean world returns an
empty findings list; rules referenced by the request are still resolved inside
the orchestrator. A quality-loop evaluator is a documented follow-on.

### `assemble` — assemble a context packet

```json
{
  "scope": { "scope_id": "wld_abc123" },
  "max_entries": 50,
  "extensions": { "nexus": { "peer_id": "12D3KooW..." } }
}
```

`AssembleRequest` takes the `scope` (required) plus an optional `max_entries`
hint. The response is the assembled packet — slim context entries
(`entry_id` / `entry_type` / `canonical_name` / optional `snippet`) for your
context window:

```json
{
  "packet": {
    "schema_version": 1,
    "packet_id": "assemble:wld_abc123",
    "entries": [
      { "entry_id": "ent_lin_xia", "entry_type": "character", "canonical_name": "Lin Xia" }
    ],
    "extensions": {}
  }
}
```

Use `assemble` to build the turn context before the LLM step in your
orchestration loop, and `check` to surface knowledge-quality findings.

---

## 5. Compute: `basic-combat` (N-C2 compute half)

The compute op invokes a **host-local** WASM module over Connect. Module
bytes are never peer-supplied: the peer names only a module the operator
installed under `~/.nexus42/modules/`, and the resolved module must be in
the peer's `module_scope` allowlist. Over the shipped Connect surface
compute is **read-only** — see the read-only lock below.

### Step 1 — install the module on the host

User-installed modules live under:

```
~/.nexus42/modules/<id>/<id>.wasm
~/.nexus42/modules/<id>/manifest.json
```

(`<id>` must match the directory name and the manifest's `module_id`.)
The authoritative authoring reference — ABI at a glance, marshalling,
`manifest.json` (incl. `wasm_sha256` pairing), `module_scope`, operator
install, read-only compute — is
[`docs/module-authoring.md`](../docs/module-authoring.md). The normative ABI
contract is
[`.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md)
(exports `memory` / `alloc` / `compute` / optional `init`, the host-import
whitelist, the marshalling convention, and the `manifest.json` contract), or
copy the reference implementation:
[`modules/basic-combat/`](../modules/basic-combat/) (`manifest.json`,
`Cargo.toml`, `src/lib.rs`) — its `alloc`/`compute` marshalling is the
pattern to reuse.

Build with `cargo build --release --target wasm32-unknown-unknown` (install
the target first: `rustup target add wasm32-unknown-unknown`), then copy the
artifacts into place. `modules/basic-combat` is a **standalone crate** (not a
workspace member), so the build runs from inside that directory:

```bash
# run from the repo root (this walkthrough assumes the repo root unless a
# block says otherwise)
cd modules/basic-combat
rustup target add wasm32-unknown-unknown      # once per toolchain
cargo build --release --target wasm32-unknown-unknown

# operator install: place the module into the runtime's host-local store
NEXUS_HOME="${NEXUS42_HOME:-$HOME}/.nexus42"
mkdir -p "$NEXUS_HOME/modules/basic-combat"
cp target/wasm32-unknown-unknown/release/basic_combat.wasm "$NEXUS_HOME/modules/basic-combat/basic-combat.wasm"
cp manifest.json "$NEXUS_HOME/modules/basic-combat/"

# Align the manifest hash to the LOCAL build — the repo manifest's
# wasm_sha256 is pinned to the embedded artifact, so copying it as-is
# fails verification. Set it to the installed bytes' hash, or delete the
# field to fall back to the stat fence (docs/module-authoring.md):
HASH=$(shasum -a 256 "$NEXUS_HOME/modules/basic-combat/basic-combat.wasm" | cut -d' ' -f1)
# write "wasm_sha256": "$HASH" into the installed manifest.json

cd ../..    # back to the repo root
```

> On the Connect surface the embedded module set is **not** reachable: the
> runtime serves only operator-installed modules under `~/.nexus42/modules/`.
> An absent/incomplete `<id>.wasm` + `manifest.json` pair is
> `module_not_found`.

### Step 2 — allowlist the module

The peer invoking compute needs `module_scope` to contain the module id (see
[Allowlist your peer](#allowlist-your-peer-on-the-host)). Missing or empty
`module_scope` denies **all** compute with `module_not_scoped` before any WASM
execution.

### Step 3 — seed the combatants and stage the compute session

`basic-combat` resolves one attack between two character entries: it reads
`base_atk` / `base_def` / `max_hp` from the entries' `body.attributes`
(per `manifest.json` `schemas.key_block_attributes.character`). Import two
combatants with the write path first:

```json
{
  "extensions": { "nexus": { "peer_id": "12D3KooW..." } },
  "knowledge_entries": [
    {
      "schema_version": 1,
      "entry_id": "kb_atk",
      "entry_type": "character",
      "canonical_name": "Lin Xia",
      "status": "confirmed",
      "body": { "summary": "Smuggler with a debt to the Ashguard", "attributes": { "max_hp": 30, "base_atk": 20, "base_def": 10, "level": 1 } },
      "extensions": { "nexus": { "world_id": "wld_abc123" } }
    },
    {
      "schema_version": 1,
      "entry_id": "kb_def",
      "entry_type": "character",
      "canonical_name": "Ashguard Enforcer",
      "status": "confirmed",
      "body": { "summary": "Gate enforcer of the Ashguard", "attributes": { "max_hp": 30, "base_atk": 5, "base_def": 5, "level": 1 } },
      "extensions": { "nexus": { "world_id": "wld_abc123" } }
    }
  ]
}
```

Then stage the compute session. The `ComputeRequest` wire carries
`session_id` / `entry_id` / `computable` / `settle` — no world carrier (the
world is the **stored entry's** `extensions.nexus.world_id`) and no module
carrier by default. The module id resolves from the **staged compute session
state** first, then the entry's `body.computable.module_id` (documented
precedence). The `project` op is not served over Connect, so a session row is
staged out-of-band on the host (workspace SQLite `compute_sessions` table —
`session_id`, `entry_id`, `state_json` with `module_id`, `created_at`).
Discover the creator id + workspace slug the runtime resolved, then insert.
The staging step uses the `sqlite3` CLI — ensure it is on `PATH` (macOS ships
it; elsewhere install it via your package manager, e.g. `brew install
sqlite3` / `apt-get install sqlite3`):

```bash
# the home the runtime booted with (HOME from §1, else the default)
NEXUS_HOME="${NEXUS42_HOME:-$HOME}/.nexus42"
cat "$NEXUS_HOME/config.toml"      # active_creator_id = "ctr_..." + active_workspace_slug_by_creator
ls "$NEXUS_HOME/creators/"         # the creator_id directory (e.g. ctr_...)

CREATOR_ID="$(sed -n 's/^active_creator_id = "\([^"]*\)"/\1/p' "$NEXUS_HOME/config.toml")"
# slug lives in the [active_workspace_slug_by_creator] section only — other
# per-creator tables (paths, urls) must never leak into it
SLUG="$(awk -v cid="$CREATOR_ID" '
  /^\[/ { in_section = ($0 == "[active_workspace_slug_by_creator]"); next }
  in_section { key = $1; gsub(/^"|"$/, "", key);
               if (key == cid) { value = $3; gsub(/^"|"$/, "", value); print value; exit } }
' "$NEXUS_HOME/config.toml")"
DB="$NEXUS_HOME/creators/$CREATOR_ID/workspaces/$SLUG/state.db"

sqlite3 "$DB" \
  "INSERT INTO compute_sessions (session_id, entry_id, state_json, created_at) \
   VALUES ('ses_combat', 'kb_atk', \
           '{\"module_id\":\"basic-combat\",\"attacker_id\":\"kb_atk\",\"defender_id\":\"kb_def\",\"character\":{\"current_hp\":30,\"max_hp\":30}}', \
           '$(date -u +%Y-%m-%dT%H:%M:%SZ)');"
```

The workspace slug defaults to `default` unless you renamed it with
`--workspace-slug` during `nexus42 creator workspace init workspace`.

(The session targets `kb_atk` and names `kb_def` as the defender; both
combatant entries are the ones seeded above. `basic-combat` reads the
combatant attributes from the inline key-block snapshot the host bundles,
selected via `attacker_id` / `defender_id`.)

### Step 4 — invoke `compute`

```json
{
  "extensions": { "nexus": { "peer_id": "12D3KooW..." } },
  "session_id": "ses_combat",
  "entry_id": "kb_atk",
  "computable": { "attacker_id": "kb_atk", "defender_id": "kb_def" },
  "settle": false
}
```

The module deterministically resolves the combat (ATK 20 − DEF 5 = 15 damage)
and the response carries the merged computable state — the confirmed receipt:

```json
{
  "session_id": "ses_combat",
  "entry_id": "kb_atk",
  "computable": {
    "module_id": "basic-combat",
    "attacker_id": "kb_atk",
    "defender_id": "kb_def",
    "character": { "current_hp": 15, "max_hp": 30 }
  }
}
```

`computable` is the session state merged with your request's dynamic
`computable` map and the module's `state_delta` (`character.current_hp`
30 → 15 — the defender's HP after the attack). The module also produces a
battle report (`kind: "combat"`, `damage`, `defender_hp_before/after`) and a
timeline event inside the WASM output; over Connect the response surfaces the
merged state view, and your backend persists the receipt it needs from that
view through its own ledger + write path.

**Read-only compute.** `settle: true` is rejected with the defined
`settle_not_enabled` envelope — the module never commits state itself, and a
`settle: false` response carries no settled `state` map. Committing the
confirmed result is the caller's job: persist the receipt in your turn ledger
and apply world-state changes through the write path (`upsert` with
world-aware CAS / structured-failure rules — never a forced overwrite).

**Compute denials** (all before WASM execution, zero side effects):

| `ErrorEnvelope.code` | Meaning |
|---|---|
| `module_not_found` | No module identity (staged state or `body.computable`), an unsafe module id shape (the id-safety check runs first in the store gate), or the module is not installed under `~/.nexus42/modules/<id>/` |
| `module_not_scoped` | The resolved module is not in the peer's `module_scope` (or the scope is missing/empty) |
| `settle_not_enabled` | `settle: true` on a read-only compute surface |
| `invalid_input` | Missing target entry or malformed payload |
| `op_unsupported` | `project` and unknown ops (zero side effects) |

This op is the settlement step the TRPG turn strategies reference — see
[`react-trpg-turn/`](./react-trpg-turn/) for the settle → receipt → narrate
discipline.

---

## 6. Fork the strategy samples and validate

Two forkable bundles ship under `strategy-samples/`:

- **`game-narrative/`** — lore import lanes (trigger + scheduled extraction of
  worldview / character sheets into KnowledgeEntry drafts + Relation hints).
- **`react-trpg-turn/`** — the TRPG turn loop: a mechanical-op lane
  (settle-first: client-supplied `operationId` + params settle immediately via
  the `compute` op, AI narrates from the confirmed receipt only) and a
  natural-language-turn lane (AI parses intent → proposes an op request
  without pre-announcing outcomes → settle → narrate the confirmed receipt →
  stop at the player response point), plus the browse-guard contract (pure UI
  ops ⇒ no AI call, no world-time advance, no state mutation) and the
  turn idempotency/completion contract (`turnId` per turn, `operationId` per
  rule op, no double-settle, caller-side ledger + world-aware CAS). Its
  README carries the distillation mapping to the partner's turn contract.

Copy the bundle you want anywhere you own (it is a plain directory; nothing
under `strategy-samples/` is embedded or required at build time):

```bash
cp -R strategy-samples/react-trpg-turn my-strategy
```

**One invariant:** the validator enforces that the bundle directory name
equals `preset.id` in the manifest (`check_bundle_id_vs_directory`). Either
keep the directory named `react-trpg-turn`, or edit `preset.id:
my-strategy` (and the `description`) in `my-strategy/preset.yaml` after
copying — validation fails otherwise.

The bundle is the standard preset format (`preset.yaml` + `templates/`). Edit
prompts, triggers, and cadence freely; keep the manifest schema shape intact
so the validator stays green. The authoritative authoring reference — manifest
format, trigger/scheduled lanes, prompt templates, validator semantics, fork
flow — is [`docs/strategy-authoring.md`](../docs/strategy-authoring.md).

### Validate

The validator runs the **real validator core** (semantic + assets + path
safety) with no daemon required:

```bash
./strategy-samples/validate.sh                             # validates the bundled game-narrative sample
./strategy-samples/validate.sh strategy-samples/react-trpg-turn   # validates the TRPG turn sample
./strategy-samples/validate.sh my-strategy                 # validates your fork
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
nexus42 system preset validate --offline --json strategy-samples/react-trpg-turn
# {"errors":[],"id":"react-trpg-turn","state_count":5,"valid":true,"version":1}
```

Without `--offline`, the command delegates to the creator daemon
(`POST /v1/daemon/presets:validate`) — for that mode a running `nexus42`
daemon is required. See [Validation behavior notes](#validation-behavior-notes)
for the differences.

**What the validator proves and what it does not.** The preset/loader
validates schema, graph shape, and referenced assets. Idempotency guarantees
(uniqueness of `turnId` / `operationId`, retry dedup, atomic commit, client
unlock) belong to the client/runtime boundary: the caller generates stable
ids, persists raw input + receipts + final output in its own turn ledger, and
applies the world-aware CAS rules when committing. The sample README and
templates document these obligations — YAML alone does not provide a global
idempotency ledger.

---

## 7. References

- Docs index: [`../docs/README.md`](../docs/README.md)
- Strategy bundles: [`game-narrative/`](./game-narrative/),
  [`react-trpg-turn/`](./react-trpg-turn/)
- Strategy authoring guide (manifest format, lanes, prompt templates,
  validator, fork flow): [`../docs/strategy-authoring.md`](../docs/strategy-authoring.md)
- Runtime usage guide: [`../docs/nexus-runtime.md`](../docs/nexus-runtime.md)
- Validator wrapper: [`validate.sh`](./validate.sh)
- WASM compute ABI: [`../.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md)
- Module authoring guide (ABI at a glance, `manifest.json` incl. `wasm_sha256`, `module_scope`, operator install, read-only compute): [`../docs/module-authoring.md`](../docs/module-authoring.md)
- Module authoring walkthrough: [`../modules/README.md`](../modules/README.md)
- Reference module: [`../modules/basic-combat/`](../modules/basic-combat/)
- Headless runtime spec: [`../.mstar/specs/daemon-runtime.md`](../.mstar/specs/daemon-runtime.md) §4.6
- Connect invoke surface (N-C2 E2): `apps/nexus42/src/commands/connect/invoke.rs`
- Connect SDK + wire family: `@42ch/spoke-connect@0.9.2` on npm
- SPOKE connect-demo (runnable mock host + third-party RemoteAdapter client):
  `../../spoke/examples/connect-demo` — the TS-side story: a `BaselinePorts`
  adapter served by a spec-faithful `ConnectHost` over WebSocket, dialed by
  `connectRemoteAdapter` from `@42ch/spoke-connect/remote` over a consumer
  `Transport`
- RemoteAdapter how-to + step-by-step integration tutorial (English and 简体中文):
  `../../spoke/docs/how-to/connect-remote-adapter.md`,
  `../../spoke/docs/tutorials/integrate-remote-adapter.md`

> The spoke paths above assume the `spoke` repository is checked out as a
> sibling of this repository (e.g. `…/42ch/spoke` next to `…/42ch/nexus`);
> if your checkout differs, use the equivalents from your own clone.
- In-repo reference peers for the runtime (Rust, `spoke-connect`):
  `apps/nexus42/examples/runtime_smoke_probe.rs` (dials a running runtime,
  prints the manifest's served ops) and `apps/nexus42/examples/connect_dialer.rs`

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

## Where the strategy runs

The preset lanes execute on the creator-facing daemon's orchestration engine
via its schedule API — `nexus-runtime` intentionally ships without the daemon
HTTP router and without schedule supervision. In the E2 loop your backend
drives the strategy side (its own timer/event loop + LLM step using the
templates) and writes results into the World over Connect; see the run-payload
contract in `game-narrative/preset.yaml` for the schedule shape when you do
use the daemon API. The TRPG turn sample shows the same division for a turn
loop: the backend orchestrates, the host-local module settles, the AI
narrates confirmed receipts only.

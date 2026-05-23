# Reference Store Layout

| Attribute | Value |
| --- | --- |
| **Status** | Active — normative V1.26 design for local reference registry + body storage |
| **Scope** | User-scoped reference registration metadata in SQLite; canonical reference body text on disk under `~/.nexus42` |
| **Related** | [v1.26 delivery compass §2](../../iterations/v1.26-local-persistence-delivery-compass-v1.md#2-reference-storage-layout-normative-draft), [local-db-schema.md](local-db-schema.md), [entity-scope-model.md](entity-scope-model.md) |

## 1. Decision

Reference sources use a two-layer local storage model:

1. `reference_sources` in workspace `state.db` is the registry unit. It stores identity, workspace binding, import locator, source type, mutability, scan state, tags, content hash, timestamps, and a pointer to the body file.
2. `body.md` on disk is the canonical body text. SQLite does not own canonical body content for new rows.

This keeps list/search metadata cheap to query while avoiding large inline text blobs in the shared local SQLite database.

## 2. Path layout

Reference bodies are partitioned by active Creator under the local Nexus home:

```text
~/.nexus42/
  creators/<creator_id>/
    references/
      units/<reference_source_id>/
        body.md
    workspaces/<workspace_slug>/
      state.db
```

The registry row stores `content_path` as a path relative to the Creator root:

```text
references/units/<reference_source_id>/body.md
```

The absolute path is resolved by `nexus-home-layout` helpers in later implementation batches. Callers must validate `creator_id` and `reference_source_id` before joining filesystem paths.

## 3. Registry contract

`reference_sources` remains a workspace `state.db` table because registration, scan state, and listing are workspace-local operations. Each row represents one reference unit.

| Field | Role |
| --- | --- |
| `reference_source_id` | Stable registry primary key and disk unit directory name. |
| `workspace_id` | Workspace binding for list/query operations. |
| `source_type` | Contract enum value such as `file`, `url`, `pdf`, or `note`. |
| `source_mutability` | Mutability policy: `static` or `refreshable`. Defaults to `static`. |
| `uri` | Logical locator; either `nexus42://references/units/<id>` for local note/body-first units or the original import URI for imported material. |
| `title`, `tags` | Human/search metadata. |
| `scan_status` | Scan lifecycle status. |
| `content_hash` | Hash of the canonical `body.md` content when available. |
| `content_path` | Relative pointer from Creator root to canonical `body.md`. |
| `content` | Deprecated legacy inline body column; must be `NULL` for new rows. |
| `created_at`, `updated_at` | Registry timestamps. |

## 4. Mutability enum

`source_mutability` is a local registry enum with exactly two V1.26 values:

| Value | Meaning |
| --- | --- |
| `static` | Body is fixed after registration except for explicit user edits/re-registration. This is the default for local files, PDFs, and notes. |
| `refreshable` | Body may be refreshed by a future scan/import pipeline, which can update `body.md` and `content_hash` while preserving the registry identity. |

Refresh pipelines are out of scope for V1.26. The enum is added now so the registry can represent future URL/PDF refresh semantics without changing the storage split.

## 5. URI scheme

The `uri` column is a logical locator, not the body path. It uses one of two forms:

- `nexus42://references/units/<reference_source_id>` for body-first local units where Nexus owns the logical source.
- The original import URI for imported material, for example `file:///...`, `/absolute/path`, or `https://...`.

The canonical body path remains `content_path`; callers must not infer filesystem location from `uri`.

## 6. Contract/type alignment notes

As of R1, `crates/nexus-knowledge/src/reference_source.rs` and `crates/nexus-contracts/src/local/domain/reference_source.rs` do not yet expose `content_path` or `source_mutability`. R2/R3 implementation should add those fields to the local/domain representation before repository and handler wiring, or introduce an explicit DB DTO that carries them without duplicating wire-contract ownership.

Generated files under `crates/nexus-contracts/src/generated/` currently contain only the shared `ReferenceSourceType` and `ScanStatus` enums; no generated `ReferenceSource` DTO needs hand edits.

## 7. Migration policy

Pre-1.0 local persistence may be wiped rather than migrated. The V1.26 migration should add `content_path TEXT` and `source_mutability TEXT NOT NULL DEFAULT 'static'`; legacy inline `content` should be retained only for compatibility and set to `NULL` for new rows.

## 8. CLI surface

Reference sources are managed via `nexus42 creator reference` subcommands (V1.26 R4):

### `nexus42 creator reference register`

Registers a new reference source: creates the registry row in `state.db` and writes `body.md` to disk.

```bash
nexus42 creator reference register \
  --source <uri-or-path> \
  --title "My Reference" \
  [--source-type note|file|url|pdf] \
  [--mutability static|refreshable] \
  [--tags "tag1,tag2"] \
  (--file <body-file> | --body "inline text")
```

- `--source` (required): logical URI or path identifying the source material.
- `--title` (required): human-readable title.
- `--source-type` (default `note`): contract enum value.
- `--mutability` (default `static`): mutability policy.
- `--file`: path to a body text file (`-` for stdin). Mutually exclusive with `--body`.
- `--body`: inline body text. Mutually exclusive with `--file`.

### `nexus42 creator reference list`

Lists all registered references (metadata only, no body loading).

```bash
nexus42 creator reference list
```

Output columns: ID, TYPE, MUTABILITY, TITLE, CREATED_AT.

### `nexus42 creator reference show <reference_id>`

Shows a single reference including all metadata and body path.

```bash
nexus42 creator reference show ref_abc123def456
```

Displays: ID, title, type, mutability, URI, workspace, scan status, timestamps, tags, content hash, and body path.

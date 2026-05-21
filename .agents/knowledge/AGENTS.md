# Knowledge — AGENTS.md

Harness knowledge directory for the Nexus OSS repo.

## Two subtrees

| Subtree | Path | Use for |
| --- | --- | --- |
| **Specs** | [`specs/`](specs/README.md) | **Functional / normative** documents: CLI, daemon runtime, ACP, orchestration, sync feature contracts (flat under `specs/`) |
| **Knowledge (root)** | This directory (files directly under `knowledge/`, not under `specs/`) | **Rules and reference**: dependency conventions, schema↔platform boundary, cross-version trackers, maintenance indexes |

**Not here:** iteration compasses → [`.agents/iterations/`](../iterations/README.md). End-user docs → `docs/`.

**Index:** specs in [`specs/README.md`](specs/README.md); knowledge-root docs in [`README.md`](README.md). Archived: [`.agents/archived/knowledge/`](../archived/knowledge/README.md).

## Where to add new documents

| Document kind | Location | Naming |
| --- | --- | --- |
| CLI / daemon / ACP / local DB **normative** | `specs/` | kebab-case `.md` (no version suffix in filename) |
| Feature or subsystem **architecture / contract** | `specs/` | `<topic>-<qualifier>.md` (kebab-case) |
| Workspace dependency / codegen boundary / trackers | `knowledge/` (root) | same kebab-case pattern |

Put `Status`, `Supersedes`, or `Revision` in the document header — not in the directory name.

## Reading during implementation

1. Read `status.json` `metadata.wave_0_spec` / `metadata.spec_refs` (paths may point to `knowledge/specs/` or `iterations/`).
2. For OSS runtime behavior, start with **`specs/`** (e.g. `cli-spec.md`, `daemon-runtime.md`); platform ADRs live under **`nexus-platform`** `v1-spec/adr/` when needed.
3. Use **knowledge-root** docs for schema boundary and crate policy only.
4. Do not silently diverge; escalate via plan residual or spec update.

## Archiving

When a spec is superseded:

1. `git mv` to `.agents/archived/knowledge/`.
2. Update [`specs/README.md`](specs/README.md) or [`README.md`](README.md) indexes.
3. Fix in-repo links in plans and other specs.

## OSS local normative SSOT (2026-05-20)

OSS local normative specs live **flat** under **`specs/`** (SSOT for this repo). Platform `v1-spec/local/` was removed; see nexus-platform `v1-spec/adr/adr-029-oss-local-specs-in-nexus-knowledge-v1.md` for migration context.

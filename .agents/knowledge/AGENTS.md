# Knowledge — AGENTS.md

Harness knowledge directory for the Nexus OSS repo.

## Two subtrees

| Subtree | Path | Use for |
| --- | --- | --- |
| **Specs** | [`specs/`](specs/README.md) | **Functional / normative** documents: CLI, daemon, ACP, orchestration, sync feature contracts, frozen `v1` local module (flat under `specs/`) |
| **Knowledge (root)** | This directory (files directly under `knowledge/`, not under `specs/`) | **Rules and reference**: dependency conventions, schema↔platform boundary, cross-version trackers, maintenance indexes |

**Not here:** iteration compasses → [`.agents/iterations/`](../iterations/README.md). End-user docs → `docs/`.

**Index:** specs in [`specs/README.md`](specs/README.md); knowledge-root docs in [`README.md`](README.md). Archived: [`.agents/archived/knowledge/`](../archived/knowledge/README.md).

## Where to add new documents

| Document kind | Location | Naming |
| --- | --- | --- |
| CLI / daemon / ACP / local DB **normative** v1 | `specs/` | `*-v1.md` (frozen module convention) |
| Feature or subsystem **architecture / contract** | `specs/` | `<topic>-<qualifier>.md` (kebab-case) |
| Workspace dependency / codegen boundary / trackers | `knowledge/` (root) | same kebab-case pattern |

Put `Status`, `Supersedes`, or `Revision` in the document header — not in the directory name.

## Reading during implementation

1. Read `status.json` `metadata.wave_0_spec` / `metadata.spec_refs` (paths may point to `knowledge/specs/` or `iterations/`).
2. For OSS runtime behavior, start with **`specs/*-v1.md`** when a frozen v1 doc exists; then related **`specs/*.md`**.
3. Use **knowledge-root** docs for schema boundary and crate policy only.
4. Do not silently diverge; escalate via plan residual or spec update.

## Archiving

When a spec is superseded:

1. `git mv` to `.agents/archived/knowledge/`.
2. Update [`specs/README.md`](specs/README.md) or [`README.md`](README.md) indexes.
3. Fix in-repo links in plans and other specs.

## OSS local normative SSOT (2026-05-20)

Former `nexus-platform` `.agents/designs/v1-spec/local/*.md` live **flat** under **`specs/`**. Edit OSS local normative text **here** only. Platform removed that tree per **ADR-029**; platform `v1-spec/README.md` §2 links back to this directory.

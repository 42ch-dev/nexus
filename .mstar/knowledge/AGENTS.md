# Knowledge — AGENTS.md

Harness knowledge directory for the Nexus OSS repo.

## Two subtrees

| Subtree | Path | Use for |
| --- | --- | --- |
| **Specs** | [`specs/`](specs/README.md) | **Functional / normative** documents: CLI, daemon runtime, ACP, orchestration, sync feature contracts (flat under `specs/`) |
| **Knowledge (root)** | This directory (files directly under `knowledge/`, not under `specs/`) | **Rules and reference**: dependency conventions, schema↔platform boundary, cross-version trackers, maintenance indexes |

**Not here:** iteration compasses → [`.mstar/iterations/`](../iterations/README.md). End-user docs → `docs/`.

**Index:** specs in [`specs/README.md`](specs/README.md); knowledge-root docs in [`README.md`](README.md). Archived implementation knowledge: [`.mstar/archived/knowledge/`](../archived/knowledge/README.md). Shipped feature tracker archive: [`.mstar/archived/shipped-features-tracker.md`](../archived/shipped-features-tracker.md).

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

## Cross-version deferred feature trackers

Two linked documents; **do not** merge into one file.

| Document | Path | Role |
| --- | --- | --- |
| **Active tracker** | [`deferred-features-cross-version-tracker.md`](deferred-features-cross-version-tracker.md) | **Open** DF/BL rows, PD-* decisions, FL-* product lines, backlog, residual mirror. Scope authority for planning is still the active iteration compass. |
| **Shipped archive** | [`.mstar/archived/shipped-features-tracker.md`](../archived/shipped-features-tracker.md) | **Append-only** closed rows (shipped / cancelled / superseded) and per-version delivery snapshots. Top-level under `.mstar/archived/` — not `archived/knowledge/`. |

### Maintenance rules (active tracker)

1. **Open only** — §3 tables list items not yet closed. Remove shipped rows from the active file; do not leave strikethrough “✅ Shipped” rows in §3.3 long-term.
2. **Closing an item** — Delete the row from §3.3 (or relevant open table). Append the same ID to [shipped-features-tracker.md](../archived/shipped-features-tracker.md) §1 with `Shipped in`, plan-id, and a brief note.
3. **Iteration close** — Add a V1.* delivery snapshot to archive §2. Update the active tracker Quick status line and PD/FL rows. Spec/tracker hygiene plans (e.g. P4) merge **last** after implementation.
4. **Re-defer** — Keep the row in §3.3; update `Target` and `Deferral history`. Do not move to archive until shipped or cancelled.
5. **Conflicts** — Active delivery compass wins over tracker targets. `status.json` `residual_findings` wins over §3.5 mirror for machine-state residuals.

### Maintenance rules (shipped archive)

1. **Append-only** — Never delete historical closed rows or snapshots.
2. **§1 Closed items** — One table row per closed DF/BL/residual tracker id; include version and plan reference.
3. **§2 Per-version snapshots** — One subsection per shipped iteration (compass link, plans, key tracker ids closed).
4. **No open backlog** — Do not add new open/deferred rows here; use the active tracker.

When a **spec** (under `specs/`) is superseded, follow §Archiving below — that path is separate from the feature tracker pair.

## Archiving

When a spec is superseded:

1. `git mv` to `.mstar/archived/knowledge/`.
2. Update [`specs/README.md`](specs/README.md) or [`README.md`](README.md) indexes.
3. Fix in-repo links in plans and other specs.

## OSS local normative SSOT (2026-05-20)

OSS local normative specs live **flat** under **`specs/`** (SSOT for this repo). Platform `v1-spec/local/` was removed; see nexus-platform `v1-spec/adr/adr-029-oss-local-specs-in-nexus-knowledge-v1.md` for migration context.

# Knowledge — Implementation SSOT

Implementation-detail specs and reusable technical design artifacts for the Nexus OSS repo.

**Not here:** iteration compasses (`*-delivery-compass-*`, legacy `v1.*` program docs) → [`.agents/iterations/`](../iterations/README.md). **Not here:** end-user docs → `docs/`. Harness boundaries: [`.agents/AGENTS.md`](../AGENTS.md).

**Index:** active documents in [README.md](README.md); archived documents in [`.agents/archived/knowledge/README.md`](archived/knowledge/README.md).

## File naming

Use **`<topic>-<qualifier>.md`** (kebab-case, no version suffix in the filename).

Examples: `daemon-api-workspace-write-architecture.md`, `novel-writing-sync-contract.md`, `crate-selection-best-practices.md`.

Put revision or status in the document header (frontmatter or a short metadata block), for example `Status`, `Supersedes`, or `Revision`. Do not encode version numbers in the path; rename the file only when the topic or qualifier itself changes.

Legacy files may still carry a `-v1` suffix; treat that as historical naming, not a pattern for new documents.

## Adding a document

1. Pick a descriptive `<topic>-<qualifier>.md` name.
2. Follow reachability rules in [`.agents/AGENTS.md`](../AGENTS.md) (*Documentation & Plans*) — no references to paths outside this repository.
3. Add a row to the **Index (active)** in [README.md](README.md) (source plan, description, status).
4. If the document is a plan output, record the path in `status.json` under that plan's `metadata` (e.g. `wave_0_spec`, `spec_refs`).

## Reading during implementation

Before writing implementation code for a plan:

1. Read `status.json` for `metadata.wave_0_spec` or `metadata.spec_refs` (may point to `knowledge/` or `iterations/`).
2. Read every referenced document in scope.
3. Do not silently diverge from in-scope knowledge/iteration specs; escalate conflicts via plan residual or spec update.

## Archiving

When a document is fully consumed by implementation or merged into a newer SSOT:

1. `git mv` the file to `.agents/archived/knowledge/` (do not delete).
2. Remove its row from [README.md](README.md) **Index (active)**; add a row to [`.agents/archived/knowledge/README.md`](archived/knowledge/README.md).
3. Update all in-repo paths (plan `.md`, archived plan JSON, cross-links in other specs).
4. Keep `archived/plans/`, `archived/residuals/`, and `archived/knowledge/` as separate subtrees.

# Specs

The **tracked normative specification corpus** for the Nexus OSS repo — the citation layer for implement and review. `.gitignore` re-includes `.mstar/specs/**`, so every file here is committed, including `archived/` (the archive of record; the harness-local archive directory is gitignored process material, not an archive).

**Layout rule:** `specs/{active|proposed|archived}/{class}/<file>.md`. The **lifecycle** directory states implement authority — `active/` is current authority, `proposed/` is not yet in effect, `archived/` is frozen history — and the **class** directory is the domain that owns the spec (`agents/`, `architecture/`, `cli/`, `compute/`, `contracts/`, `creator/`, `novel-writing/`, `orchestration/`, `runtime/`, `surfaces/`).

**Discovery is by path and search, not by an index.** The path states lifecycle + class; each spec's own header states its `Status`, `Document class`, and `Coordinates with` / `Related`. Open the class directory you need, or search the corpus for the topic.

**No central index — by design.** This file is a directory note, not a document list. A maintained list duplicates metadata the path and each header already carry, and it makes every unrelated spec change contend on one file with a standing risk of drifting from disk. Where an index would have carried authority, that authority is durable elsewhere: layout, lifecycle, class, status, conflict, and maintenance rules in [`AGENTS.md`](AGENTS.md); per-topic ownership in the owning spec's header and body; and a folded or retired spec records its own supersession in the merge-target doc (`Supersedes:` / provenance line).

**Rules:** [`AGENTS.md`](AGENTS.md). **Sibling tree:** engineering knowledge — compound distillations and reusable patterns — is [`../knowledge/`](../knowledge/README.md); specs are the normative layer.

**Former names** (a rename leaves no index row behind; the old basename resolves to the frozen record): `nexus42-single-binary-daemon-runtime-architecture.md` → [`archived/runtime/2026-09-21-daemon-runtime.md`](archived/runtime/2026-09-21-daemon-runtime.md); `local-platform-isolation-and-crate-architecture.md` → [`archived/architecture/2026-09-21-local-cloud-crate-architecture.md`](archived/architecture/2026-09-21-local-cloud-crate-architecture.md).

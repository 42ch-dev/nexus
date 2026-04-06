# Knowledge Base

Dev-process knowledge for the Nexus project. These documents are **inputs to** or **outputs from** specific plans — they serve as context for agent handoff and cross-session continuity, but are not intended for external consumers.

For the distinction between this directory and `docs/`, see [`AGENTS.md`](../../../AGENTS.md) §"Content Boundary: `docs/` vs `.agents/plans/knowledge/`".

## Index

| Document | Source Plan | Description | Status |
|----------|-------------|-------------|--------|
| [revised-domain-models-spec-v1.md](revised-domain-models-spec-v1.md) | `2025-04-05-domain-models` | Field-by-field revision of all 15 domain aggregates, aligned with JSON Schema truth source. Resolves 6 P1 critical gaps (G1–G6) found in architecture review. | Superseded by implementation (Wave 1 complete) |
| [restructured-context-assembly-v1.md](restructured-context-assembly-v1.md) | `2025-04-05-context-assembly` | Restructured spec narrowing scope to CLI-side only (summary generation + Local API call + bundle metadata). Removes 5 critical deviations from frozen specs. | Input for Wave 2+ implementation |
| [acp-client-tech-spec-v1.md](acp-client-tech-spec-v1.md) | `2025-04-05-acp-client` | Complete technical specification for ACP Client integration: SDK selection (agent-client-protocol v0.10.4), architecture, registry caching, CLI commands, capability IDs, schema definitions. Resolves ACP-R1 and ACP-R2. | Input for implementation |

## Maintenance

### Adding a new document

1. Choose a descriptive name: `<topic>-<qualifier>-v<N>.md` (e.g. `cli-auth-model-v1.md`)
2. Write the document following the reachability rules in `AGENTS.md` §"Documentation & plans" — **no references to files outside this repository**
3. Add an entry to the **Index** table above (source plan, description, status)
4. If the document is an output of a plan, record the path in `status.json` under that plan's `metadata` (e.g. `wave_0_spec`, `spec_refs`)

### Reading knowledge during implementation

When an agent starts work on a plan, it should:

1. Check `status.json` for the plan's `metadata.wave_0_spec` or `metadata.spec_refs` fields — these point to knowledge base documents that serve as authoritative input
2. Read the referenced knowledge documents **before** writing implementation code
3. Treat knowledge documents as the **ground truth** for design decisions within their scope — do not silently diverge from them

### Archiving

When a knowledge document is fully consumed by implementation (all its content reflected in committed code):

1. Keep the document in place (do not delete — it preserves the design rationale)
2. Update the **Status** column in the Index to `Superseded by implementation (<plan-id> complete)` or `Archived`
3. Do not move to `../archived/` — that directory is for completed plan files, not knowledge artifacts

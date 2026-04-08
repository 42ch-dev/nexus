# Knowledge Base

Dev-process knowledge for the Nexus project. These documents are **inputs to** or **outputs from** specific plans — they serve as context for agent handoff and cross-session continuity, but are not intended for external consumers.

For the distinction between this directory and `docs/`, see [`AGENTS.md`](../../../AGENTS.md) §"Content Boundary: `docs/` vs `.agents/plans/knowledge/`".

## Index

| Document | Source Plan | Description | Status |
|----------|-------------|-------------|--------|
| [revised-domain-models-spec-v1.md](revised-domain-models-spec-v1.md) | `2025-04-05-domain-models` | Field-by-field revision of all 15 domain aggregates, aligned with JSON Schema truth source. Resolves 6 P1 critical gaps (G1–G6) found in architecture review. | Superseded by implementation (Wave 1 complete) |
| [restructured-context-assembly-v1.md](restructured-context-assembly-v1.md) | `2025-04-05-context-assembly` | Authoritative SSOT for Context Assembly design decisions. Includes: V1.0 frozen spec (goals, data input surface, technical freeze, non-goals, open decisions), responsibility split, strict boundaries, request/response schemas, bundle metadata integration, V1.1 enhancement constraints (file size limit, path traversal validation, UTF-8 truncation safety). | Active — authoritative SSOT for Context Assembly design decisions (responsibility split, strict boundaries, request/response schemas, V1.1 enhancement constraints). Plan 11 references this document. |
| [acp-client-tech-spec-v1.md](acp-client-tech-spec-v1.md) | `2025-04-05-acp-client` | Complete technical specification for ACP Client integration: SDK selection (agent-client-protocol v0.10.4), architecture, registry caching, CLI commands, capability IDs, schema definitions. Resolves ACP-R1 and ACP-R2. | Active — authoritative input for Plan 12 (`2026-04-09-v1.1-acp-ux-permissions.md`) |
| [phase1-architecture-review-v1.md](phase1-architecture-review-v1.md) | V1.0-phase1 Retrospective | Comprehensive architecture review of all Phase 0 + V1.0-phase1 crates and schemas. 36 findings (3 critical, 7 high, 14 medium, 12 low). | Archived — key findings summarized in `v1.1-overview-v1.md` §2.3; retained for detailed reference |
| [phase1-product-review-v1.md](phase1-product-review-v1.md) | V1.0-phase1 Retrospective | Comprehensive product review of all Phase 0 + V1.0-phase1 user-facing features. 32 CLI commands analyzed (24% functional, 43% skeleton, 11% not implemented). | Archived — command analysis summarized in `v1.1-overview-v1.md` §2.1; retained for detailed reference |
| [phase2-product-plan-v1.md](phase2-product-plan-v1.md) | V1.0-phase2 | Product plan for V1.1 Beta. P0/P1/P2 features (18), user stories, command-by-command status plan, competitive analysis. **Merged into** `v1.1-overview-v1.md`. | Superseded — merged into v1.1-overview-v1.md |
| [phase2-architecture-plan-v1.md](phase2-architecture-plan-v1.md) | V1.0-phase2 | Architecture plan for V1.1. 5 implementation plans (A–E), residual integration matrix (38 items), dependency graph, technical decisions, risk mitigation. **Merged into** `v1.1-overview-v1.md`. | Superseded — merged into v1.1-overview-v1.md |
| [2026-04-06-pre-phase2.md](../2026-04-06-pre-phase2.md) | V1.0-phase2 | **Consolidated pre-Phase 2 implementation plan.** Merges findings from architecture review (36 findings) and product review (37 feature analysis) into 5 implementation plans (A–E) with task-level checklists. | Superseded — content absorbed into v1.1-overview-v1.md |
| [v1.1-overview-v1.md](v1.1-overview-v1.md) | V1.1 | **Authoritative V1.1 development overview.** Consolidates product plan (user stories, feature priorities, competitive analysis), architecture plan (technical decisions, Plans A–E history, residual matrix, risk mitigation), and PM coordination (3 active nexus plans: 09/11/12, cross-repo platform plans 09–15 alignment, deferred Plans 13/14, GA criteria). | Active — single source of truth for V1.1 |
| [architecture-alignment-review-v1.md](architecture-alignment-review-v1.md) | 2026-04-08 | Comprehensive architecture alignment review of `crates/` vs v1-spec. 6-dimension analysis (architecture, domain model, CLI/daemon, ACP integration, sync contract, technical debt). Identifies 3 critical gaps (TD-1/2/3), 3 high-priority gaps (TD-4/5/6), and provides priority roadmap. References v1.1-overview-v1.md for execution. | Active — authoritative architecture review for V1.1 planning |

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

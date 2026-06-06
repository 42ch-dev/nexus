# Knowledge Base

Engineering reference and **spec index** for the Nexus OSS repo.

| Subtree | Role |
| --- | --- |
| **[`specs/`](specs/README.md)** | Functional & normative specifications (CLI, daemon, ACP, orchestration, **`specs/`** v1 frozen module) |
| **This directory (root files)** | Cross-cutting rules and trackers — not feature specs |

Iteration compasses: [`.mstar/iterations/`](../iterations/README.md). Conventions: [AGENTS.md](AGENTS.md). Harness: [`.mstar/AGENTS.md`](../AGENTS.md).

## Index (knowledge root — rules & reference)

| Document | Description | Status |
| --- | --- | --- |
| [crate-selection-best-practices.md](crate-selection-best-practices.md) | Rust workspace dependency conventions | Active |
| [schemas-wire-platform-sync-boundary.md](schemas-wire-platform-sync-boundary.md) | Which `schemas/` types ship to contracts vs local-only Rust | Active |
| [specs/schemas-directory-layout.md](specs/schemas-directory-layout.md) | `schemas/` folder tree, cloud vs local, rename policy | Active |
| [deferred-features-cross-version-tracker.md](deferred-features-cross-version-tracker.md) | Open/backlog deferred-feature lifecycle (active tracker) | Active |

Specs index includes [work-experience-model.md](specs/work-experience-model.md) (V1.33), [creator-workflow-fl-e.md](specs/creator-workflow-fl-e.md) and [agent-nexus-tool-bridge.md](specs/agent-nexus-tool-bridge.md) (V1.34), [cli-command-ia.md](specs/cli-command-ia.md), [creator-centric-entry-model.md](specs/creator-centric-entry-model.md), and [preset-conditional-routing-fl-d.md](specs/preset-conditional-routing-fl-d.md) (V1.35). V1.35 CLI audit evidence: [v1.35 compass Appendix A](../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md#appendix-a-cli-usability-audit-v135).

## Specs

All feature and local normative documents: **[`specs/README.md`](specs/README.md)**.

## Archived

- Implementation knowledge supersession: [`.mstar/archived/knowledge/`](../archived/knowledge/README.md)
- Shipped / closed feature tracker: [`.mstar/archived/shipped-features-tracker.md`](../archived/shipped-features-tracker.md)

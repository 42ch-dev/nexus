# V1.129 Compound Round Summary

> Filled at iteration-close (Phase 3 §3.2).

## Iteration package inventory

| Path | Type | Disposition |
|------|------|-------------|
| `iterations/v1.129/specs/profile-create-reliability.md` | P0 spec | **Promote to `{KNOWLEDGE_DIR}/architecture-patterns/transport-error-classification.md`** — codifies the `TransportErrorKind` classifier pattern (reusable beyond V1.129) |
| `iterations/v1.129/specs/transport-error-ux.md` | P1 spec | **Promote to `{KNOWLEDGE_DIR}/architecture-patterns/studio-first-primitive-promotion.md`** — codifies the Studio-first → `@42ch/nexus-ui` promotion workflow (with V1.129 P1 process-exception learning) |
| `iterations/v1.129/specs/dogfood-nit-closeout.md` | P2 spec | Keep in iteration package (one-shot closeout; not reusable) |
| `iterations/v1.129/delivery-compass.md` | compass | Stays (iteration SSOT) |
| `iterations/v1.129/README.md` | index | Stays |

## Knowledge crystallization

Two knowledge docs to crystallize in a follow-up compound pass (post-PR-merge; not blocking Phase 4):

1. **`{KNOWLEDGE_DIR}/architecture-patterns/transport-error-classification.md`** — pattern: classify transport failures into a small enum (`network`/`tls`/`timeout`/`http_fallback`/`daemon_down`/`unknown`), surface per-kind copy + CTA. Source: P0 spec + P0/P1 implementation.
2. **`{KNOWLEDGE_DIR}/architecture-patterns/studio-first-primitive-promotion.md`** — pattern: Studio fixture → `@42ch/nexus-ui` primitive → app surfaces, with the caller-owned-copy contract (primitive is presentational only; localized copy flows in via props). Source: P1 spec + V1.129 P1 process-exception learning.

These are non-blocking — the iteration package specs under `.mstar/iterations/v1.129/specs/` remain readable and referenceable until a follow-up compound pass moves them. The PM records the intent here so a future `mstar-compound` trigger knows what to do.

## CONCEPTS.md

No new domain terms introduced. `TransportErrorKind` is a code-level concern, not a domain concept.

## Deferred knowledge work

- Compound-promotion to `{KNOWLEDGE_DIR}/architecture-patterns/`: deferred to a post-V1.129 follow-up (PM or writing-specialist triggers `mstar-compound`).
- `DF-V1127-NIT-CLOSEOUT` row in deferred-features tracker: archive the two anchors (R-V1126P0-T2-001, R-P1-001) at iteration-close; remaining nits stay open with target `post-V1.129`.

## Trigger compound-refresh?

**No.** V1.129 did not surface stale knowledge docs. No overlap to merge, no doc to retire.

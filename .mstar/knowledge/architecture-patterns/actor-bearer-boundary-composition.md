---
module: actor-runtime
date: 2026-09-05
problem_type: architecture_pattern
category: architecture-patterns
severity: high
applies_when:
  - adding-actor-kinds
  - extending-character-scope
  - changing-session-reuse
  - adding-mind-data
tags:
  - actor
  - character
  - binding
  - knowledge-view
  - session-isolation
  - memory-bearer
  - theory-of-mind
related_components:
  - nexus-local-db
  - nexus-daemon-runtime
  - nexus-moment-context-assembly
  - nexus-creator-memory
  - nexus-spoke-adapter
---

# Actor bearer boundary composition

## Context

A Character crosses identity, World membership, knowledge, execution, memory, and mental-state storage. Isolation is sound only when every layer derives scope from stored rows and passes an admitted capability forward. Rechecking only the route payload, revision, or provider session id leaves cross-Actor seams.

Normative product semantics live in [Actor Product Model](../../specs/actor-product-model.md). This document captures the reusable implementation pattern validated by the v1.184 package in [`iterations/v1.184/specs/`](../../iterations/v1.184/specs/).

## Guidance

### 1. Keep identity and viewpoint separate

Use a closed `ActorRef` for who acts and a subordinate viewpoint for World, binding, branch, and event. A Character binding is an authorization join, not identity. Do not encode Actor identity in provider session ids, orchestration roles, or a copied World KB.

Creation and destructive lifecycle changes are write-serialized transactions. Character creation inserts its first active `ActorWorldBinding` atomically. Binding removal checks, in fixed order, last-active-binding, binding-owned knowledge, and binding-local memory before deleting exactly one binding. Every rejection mutates nothing.

### 2. Model knowledge ownership once

Keep one `KnowledgeEntryRecord` with a closed owner union:

- World owner: World-local; `creator_only` is valid only here.
- Character owner: explicitly shared across all active bindings of that Character.
- Binding owner: private to one World life.

The database must enforce exactly one matching owner column. View services compose these scopes after stored admission and return all-or-error bounded pages. Avoid mount tables, copied World KBs, Rust-side merges of unbounded component lists, and payload-claimed ownership.

### 3. Admit before every side effect

One admission service resolves the active Creator, Character, World, binding, and complete bounded KnowledgeView from storage. Reuse it at session creation and again before every prompt. Invalid or stale bindings must fail before MCA, registry mutation, session creation, filesystem access, or provider calls.

Session reuse keys include every history-shaping dimension: provider, canonical workspace, model/mode, Actor, World, binding, branch, and event. Serialize creation per key; reuse only Ready sessions, reject Busy, and retire stale sessions. Legacy Creator requests remain outside the Actor registry path.

### 4. Add bearer arms without duplicating engines

Represent memory storage with a closed bearer sum, such as `MemoryBearerRef::{Creator, Character}`. Shared extraction, review, promotion, aggregation, synthesis, and rendering operate on the bearer; only repositories and canonical paths dispatch by arm.

Character rows and files remain separate from Creator storage. Binding-local pending data stays local through review. Only an explicit revision-checked promotion clears binding provenance on the same fragment id. Projection treats true absence as empty but propagates metadata, permission, parse, and database errors before host execution.

Capacity is part of authorization: reserve deterministic room for selected-binding data so global Character memory cannot crowd it out.

### 5. Separate ToM carrier from subject

`MindState.holder_entry_id` is the carrier KnowledgeEntry id. The epistemic subject belongs in `modules.belief[*].holder`; a Character id must never be written into the carrier FK.

Record ToM by CAS-patching the authoritative carrier belief array and inserting the derivative MindState in one transaction through the existing SPOKE validator. Revalidate live status and admitted owner scope inside the CAS transaction; revision alone is not an authorization boundary.

Queries materialize only the exact probe-admitted carrier-id snapshot. Bound carrier count and JSON array length before Rust deserialization, preserve invalid stored JSON rather than overwriting it, and use physical array ordinals in cursors. Fill L1 and L2 with independent bounded order-specific queries so an L1-heavy corpus cannot starve L2.

ToM record, query, and projection call no provider. A subsequent admitted Character run performs the normal single Host prompt operation.

## Why this matters

Each local rule is insufficient by itself. Database FKs do not enforce product deletion precedence; optimistic concurrency does not prove scope; handler admission can become stale before commit; a bounded response can still do unbounded work; and a shared provider session can mix otherwise isolated storage. The pattern composes storage constraints, stored admission, transactional revalidation, exact session keys, bounded materialization, and one existing runtime path.

## When to apply

Apply this pattern when adding an Actor kind, widening World/binding visibility, changing session reuse, adding bearer-specific memory, or projecting new mind data. Re-run the full two-Actor/two-World fixture whenever any boundary changes.

## Examples

- [Actor identity and binding](../../iterations/v1.184/specs/actor-identity-binding.md)
- [Actor knowledge ownership and view](../../iterations/v1.184/specs/actor-knowledge-view.md)
- [Actor execution and session isolation](../../iterations/v1.184/specs/actor-execution.md)
- [Character SOUL and Memory](../../iterations/v1.184/specs/character-memory.md)
- [Character ToM L1/L2](../../iterations/v1.184/specs/character-tom.md)

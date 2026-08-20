---
module: nexus-narrative
date: 2026-08-14
problem_type: knowledge
category: engineering
severity: low
tags: [testing, json-assertions, key-order, btreesemap, typed-seams, spoke, serde]
last_updated: 2026-08-14
applies_when: Writing test assertions that compare serialized JSON strings across a typed seam (spoke generated types, serde maps); any test asserting exact string equality on state_json/body_json/modules_json style columns
---

# Order-Insensitive JSON Assertions on Typed Seams

## Context

`n_c2_compute_request_module_override_denied` (nexus42 connect interop) failed only after the spoke 0.10.0 upgrade:

```
left:  Some("{\"module_id\":...,\"attacker_id\":...,\"defender_id\":...}")
right: Some("{\"attacker_id\":...,\"defender_id\":...,\"module_id\":...}")
```

The test asserted **exact string equality** on a serialized JSON column. spoke 0.10.0's generated `ModuleMap` is a `BTreeMap`-backed typed map → serde serializes keys **alphabetically**, re-ordering a payload the old test had only passed by coincidence of insertion order. The JSON was semantically identical.

## Guidance

Never assert raw-string equality on JSON produced across a typed seam. Compare parsed values:

```rust
assert_eq!(
    actual.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
    expected.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
    "... (order-insensitive)"
);
```

Single-key objects are order-safe by construction; anything with ≥2 keys is one upstream type change away from breaking.

## Why This Matters

The failure looks like a **data corruption bug** (stored state "differs" from expected) when it is actually a representation change — expensive to RCA if you chase the storage layer first.

## When to Apply

- Tests comparing `*_json` columns or serialized envelope fields.
- Any upgrade that swaps a struct/map representation behind serde (spoke codegen bumps are the recurring case).

## Examples

Fixed in 2c1f63a4 (V1.164 P1 QA blocker): parse-both-sides compare; suite 1201/1201. Sibling audit found exactly one other string-equality assertion (single-key — legitimately safe).

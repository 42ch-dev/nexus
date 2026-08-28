---
module: wire-contracts
date: 2026-08-27
problem_type: architecture_pattern
category: conventions
severity: medium
applies_when:
  - "Adding an additive optional field to a Rust contract type that crosses the daemon API without a JSON Schema carrier"
  - "Filling the plan frontmatter `wire_contracts_changed` for a preset/orchestration-facing change"
  - "Deciding whether `pnpm run codegen` / check-wire-drift must run for a change"
  - "Reviewing a plan whose GC says 'wire changes ONLY these fields' and the schema diff is empty"
tags:
  - wire-contracts
  - json-schema
  - codegen
  - typify
  - presets
  - raw-yaml
  - additive-fields
last_updated: 2026-08-27
created: 2026-08-27
status: active
---

# Wire-invisible additive fields — `wire_contracts_changed: true` with an empty schema diff is a legitimate outcome (when the carrier is raw YAML)

## Context

House rule: **wire contracts are truth** — JSON Schema is the single source of truth for cross-language types, and plan frontmatter declares `wire_contracts_changed`. The trap: two different carriers wear the "wire" label.

1. **Schema-carried surfaces** (`schemas/**` → typify → generated Rust + TS DTOs): additive Rust fields with no schema change = drift → `check-wire-drift.sh` fails → the plan's wire claim is wrong.
2. **Raw-carrier surfaces** (creator presets cross the daemon API as **raw YAML**, stored and re-served verbatim; `preset-management` schemas are envelopes only): contract structs like `StateDefinition` parse the YAML in Rust, but **no JSON Schema describes them**. An additive `Option<u64>` field with `#[serde(default)]` + `skip_serializing_if` changes the Rust parse surface while the schema surface is *empirically empty*.

V1.179 P2 (DR-06) shipped `timeout_ms`/`on_timeout` on `StateDefinition` under `wire_contracts_changed: true`. `pnpm run codegen` was an empirical **no-op** (zero generated diff), `check-wire-drift.sh` exit 0, zero TS changes, no npm bump — and that is the *correct* outcome, not a skipped step.

## Guidance

When a plan touches a raw-carrier contract type:

1. Keep `wire_contracts_changed: true` if the field is observable across a process boundary (here: preset authors write the fields in YAML) — the *contract* changed even though the *schema corpus* did not.
2. Still run the gate suite (`pnpm run codegen` + `check-wire-drift.sh`) and record **why** it is a no-op: "no `schemas/` file references `StateDefinition`; presets cross the daemon API as raw YAML; preset-management schemas are envelopes only."
3. The durable spec home for the field semantics is the **normative spec doc** (e.g. `preset-conditional-routing.md` §3.3.3 field table), not `schemas/`.
4. Distinguish from schema-carried surfaces: if a `schemas/**` file *does* reference the type, an empty schema diff is drift — fix the schema, not the claim.

## Why This Matters

Over-claiming forces pointless schema churn (inventing a schema for a raw-carrier type); under-claiming hides a cross-process contract change from the release notes and review gates. The `wire_contracts_changed` flag means "cross-process contract surface changed," not "schemas/ directory changed" — but proving the distinction requires the recorded no-op evidence, not silence.

## When to Apply

- Adding optional fields to preset/orchestration YAML-facing structs (`StateDefinition`, `ConvergeConfig`-adjacent types)
- Filling plan frontmatter for changes touching `nexus-contracts` types without schema counterparts
- Reviewing a wire_contracts_changed:true plan whose codegen diff is empty (verify the raw-carrier argument, then accept)

## Examples

V1.179 P2 T2: `timeout_ms` + `on_timeout` on `StateDefinition`; loader fail-closed rules; normative table in `preset-conditional-routing.md` §3.3.3; codegen no-op recorded in the implementer report + QC seat 1 judgment ("GC#1 wire scope acceptable without escalation"). Contrast: `schemas/generated` DTO additions (e.g. memory-review `ReviewResponse` in V1.80) DID regenerate + bump `@42ch/nexus-contracts`.

## Prevention

When locking a plan with wire changes, name the **carrier** (schema vs raw YAML) next to `wire_contracts_changed` in the Clarify section, and pre-write the expected codegen outcome (regen+drift vs recorded no-op) so the implementer cannot skip gates silently and QC cannot demand phantom diffs.

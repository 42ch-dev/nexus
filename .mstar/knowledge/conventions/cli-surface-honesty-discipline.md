---
module: apps/nexus42 (CLI leaves), nexus-daemon-runtime (error contract)
date: 2026-08-26
problem_type: convention
category: conventions
severity: medium
plan_id: 2026-08-25-v1.175-p1-developer-cli-parity
tags:
  - cli
  - error-code
  - error-details
  - clap
  - valueenum
  - truncation
  - has-more
  - dto
applies_when:
  - "Authoring a CLI leaf that consumes a daemon route"
  - "Documenting error surfaces in cli-spec rows or --help text"
  - "Rendering paginated or capped daemon responses on a human surface"
  - "Defining a clap ValueEnum flag whose tokens must match wire enums"
---

# CLI surface honesty discipline — public error codes, enum tokens, truncation

## Context

V1.175 P1 shipped 8 §5-remainder groups as thin `nexus42` CLI leaves over
existing daemon routes. Implementer rounds and plan QC converged on three
honesty rules that keep CLI help, output, and test pins aligned with what
the daemon actually delivers — not with what the docs guessed it delivers.

## Guidance

### 1. Error-code honesty — document and test the ACTUAL public code

The daemon error envelope is two-tier (`crates/nexus-daemon-runtime/src/api/errors.rs`):

- `error_code()` returns a **coarse public code** from an allowlist
  (`bad_request`, `not_found`, `invalid_input`, `strategy_conflict`,
  `outline_conflict`, `world_kb_conflict`, `strategy_validation_failed`,
  `outline_validation_failed`, `world_kb_validation_failed`,
  `invalid_transition`, `conflict`, …). `BadRequest` inner codes **outside
  the allowlist are remapped** to the generic `bad_request` — e.g. the
  strategy handlers raise `strategy_invalid` (400) internally, but the wire
  delivers `bad_request`. `Internal.code` is never the wire code.
- CLI docs (cli-spec rows, `--help`) and tests must state the **public**
  code, not the inner one. Listing a remapped code teaches consumers a
  contract the wire never delivers.
- **Render exactly the `error_details()` keys per family**: strategy /
  outline 409 → `current_revision` + `node_id` + `conflicting_path` +
  `recovery_hint`; world-kb 409 → `current_version` + `entity_id` +
  `conflicting_path` + `recovery_hint` (NOT `current_revision` — verified
  against the daemon before documenting); 422 validation family → render
  each `details.validation_summary.errors` entry as
  `(validation: <rule>)`, never swallow the generic top-level message.
- **Tests pin `[code]` + structured fields, never OR-matched status
  strings.** Asserting stderr contains "409" / "not found" accepts
  fabricated text and misses remap regressions. Pin `[strategy_conflict]`
  + `current_revision=…`, `[outline_validation_failed]` + the rule text,
  `[world_kb_conflict]` + `current_version`.
- 204 deletes (`progress clear`, `annotation remove`) print **empty stdout
  under `--json`** — no DTO exists to emit; never fabricate `{"ok": true}`.

### 2. clap ValueEnum tokens must be pinned to wire snake_case

clap's `ValueEnum` derive defaults to **kebab-case** tokens
(`--op move-chapter`); wire enums are **snake_case** (`move_chapter`).
Pin every variant with `#[value(name = "…")]` so `--help`, parsing, and the
wire agree:

```rust
#[derive(ValueEnum)]
enum OutlineOp {
    /// Move a chapter to a different volume.
    #[value(name = "move_chapter")]
    MoveChapter,
    // ...
}
```

- T3 precedent: outline/timeline ops + chapter status enums; T4 extended it
  to the 19-variant `block_type` and findings `VALID_STATUSES` surfaces.
- Without the pin, tokens silently serialize kebab-case to the daemon and
  every call 400s with a wire error that looks nothing like a typo.

### 3. Truncation honesty on paginated / capped surfaces

- The **human default must render a truncation note** whenever the DTO
  signals a bound: `creator works findings list` prints a
  `has_more` note when `pagination.has_more`; `creator world kb graph`
  documents the daemon's 500-entity / 1000-relationship cap in `--help`
  and renders a cap note when the cap is hit — and says plainly that no
  wire `truncated` flag exists yet.
- **`--json` stays DTO-verbatim** — the `has_more` flag is already in the
  DTO; never synthesize fields, never expand, never "fix" the DTO for
  humans.
- Document bounded derivations too: the fork parent-branch derivation
  reads only the first page (100 canon + 100 provisional events) — cli-spec
  rows state the bound so users know why an old fork-point requires
  `--parent-branch`.

## Why This Matters

The CLI is the developer surface for the author/orchestration loop. A doc
or test that names `strategy_invalid` teaches a contract the wire remaps;
an OR-matched status assertion can stay green while the code regresses to
fabricated output; a human default that hides `has_more` presents data
loss as completeness. Each rule is cheap at authoring time and expensive
as a downstream debugging session.

## When to Apply

- New CLI leaf consuming a daemon route: check `error_code()` and
  `error_details()` for the exact public surface before writing cli-spec
  rows, `--help`, or tests.
- New clap enum flag whose values cross the wire: pin `#[value(name)]`.
- New paginated/capped read surface with a human default: decide the note
  text up front, keep `--json` verbatim.

## Examples

```text
# test pin shape (never OR-match status strings):
#   stderr contains [strategy_conflict] and current_revision=2
#   stderr contains [world_kb_conflict] and current_version=1
#   stderr contains [outline_validation_failed] and (validation: kebab-case rule)
```

```rust
// clap ValueEnum pin (snake_case wire token):
#[value(name = "in_review")]
InReview,
```

- Implemented in: `apps/nexus42/src/commands/preset/patch.rs`,
  `creator/works/outline.rs` (ValueEnum pins + route-family guard),
  `creator/world/kb/daemon.rs` (`block_type`), `creator/world/fork.rs`,
  `creator/works/mod.rs` (`VALID_STATUSES`), shared
  `DaemonClient::parse_error_response` (CAS 409 family + validation-summary
  rendering); tests `tests/strategy_patch_cli.rs`, `tests/outline_cli.rs`,
  `tests/world_kb_daemon_cli.rs`, `tests/findings_triage_cli.rs`.
- Related: `api-design/field-level-error-envelope-for-generated-dtos.md`
  (daemon-side envelope the CLI consumes),
  `architecture-patterns/pagination-cursor-without-total-count-labels.md`
  (has_more lower-bound labels on web surfaces),
  `architecture-patterns/shared-validation-core-migration.md` (byte-identical
  error text across CLI/daemon re-exports).

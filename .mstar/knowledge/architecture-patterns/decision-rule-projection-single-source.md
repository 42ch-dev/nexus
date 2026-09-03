---
module: nexus-orchestration / nexus42 CLI
date: 2026-09-04
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-09-03-v1.182-p1-bl04-checkpoint-resume-ux
tags: [decision-rules, single-source, cli-projection, drift-risk, shared-projection, verdict]
applies_when:
  - a CLI/GUI/inspection surface must restate engine decision rules (resume eligibility, gate outcomes, admission verdicts)
  - a second consumer needs the same rule outcome as the engine but cannot invoke the engine
  - QC flags "rule duplication drift risk" between an engine module and a projection surface
---

# Decision-rule projection: single source, many surfaces

## Context

The v1.182 BL-04 inspect CLI had to answer "can this checkpointed run resume?" outside the daemon. The authoritative rules live in `resume_driven_sessions` (`crates/nexus-daemon-runtime/src/preset_run.rs`, rules 1–3 decide; rule 4 — engine/runner availability — is boot-time only). The first implementation copied those rules into `apps/nexus42/src/commands/ops.rs`, and plan QC (seat 1) flagged the drift risk; seat 3 then caught the projection omitting rule 1 (terminal/cancelled rows kept live join keys → CLI said `resumable: yes` while boot's recovery filter would never re-drive them). The fix extracted `crates/nexus-orchestration/src/resume_rules.rs` as the single projection source consumed by BOTH the daemon resume path and the CLI detail/list surfaces.

## Guidance

1. **Project rules next to their source, not in the consumer.** Put the pure projection function in the crate that owns the state machine (`resume_rules.rs` in `nexus-orchestration`), taking the stored row as input and returning a typed verdict. Engine path and CLI path both call it — there is exactly one place where the rules are written.
2. **Split decidable verdict from environment caveat.** Rules that read stored state (rules 1–3) produce the verdict; rules that depend on runtime context the caller cannot see (rule 4, "is a runner attached?") become a separate caveat field (`runner_check: "boot_time" | "not_applicable"`). Never merge them into one boolean — the CLI would have to guess, and a guess is a lie (contract §4: `unknown` is reserved for unreadable context).
3. **The stored status column is not a rule.** `SqliteSessionStorage::save` always writes `status="running"`; the projection must classify from the durable fields (`_run_status`, join keys, context), never from the bookkeeping column.
4. **Pin equivalence with tests on both sides.** Unit tests on the projection (each rule → verdict class) + a CLI integration test asserting the same row classifies identically through the binary + an E2E that exercises inspect-then-resume ≡ resume-without-inspect. The V1.163-style compile gate catches structural breaks; these tests catch semantic drift.

## Why This Matters

A copied decision rule desyncs silently: each surface compiles and passes its own tests while answering differently to the same input. The failure surfaces as an honesty bug (operator told "resumable" for a row the engine will never re-drive), which is worse than a crash. Extraction cost is low when done at first flag (the rules are already written — they move); post-drift cost is a correctness incident plus a contract amendment.

## When to Apply

- New inspection/reporting surfaces over engine state (CLI subcommands, run reports, dashboards).
- Any QC finding of the shape "rules duplicated between X and Y".
- Contract-first CLI work: write the contract against the shared projection's types, not against prose restatements.

## Examples

- `crates/nexus-orchestration/src/resume_rules.rs` (v1.182): single projection consumed by `preset_run.rs` (boot re-drive) and `apps/nexus42/src/commands/ops.rs` (ops inspect detail + list), with `ResumeDecision`-mirroring enum and unit-tested rule→verdict mapping.
- Anti-example (pre-fix): ops.rs restating rules 1–3 inline — rule 1 was dropped and a cancelled session reported resumable.

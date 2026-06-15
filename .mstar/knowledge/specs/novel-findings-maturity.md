# Novel Findings Maturity — Draft Overlay v1 (V1.48)

**Status**: Draft (V1.48)  
**Document class**: Draft overlay  
**Created**: 2026-06-16  
**Last updated**: 2026-06-16  
**Scope**: V1.48 iteration — findings producer quality, consumer injection, rules runtime, data hygiene, serial cross-refs  
**Merge target (P-last)**: Fold sections into [novel-quality-loop.md](novel-quality-loop.md); amend [novel-workflow-profile.md](novel-workflow-profile.md) §5.5.2 / §5.5.4  
**Coordinates with**:

- [novel-quality-loop.md](novel-quality-loop.md) — Shipped (V1.47) base
- [novel-workflow-profile.md](novel-workflow-profile.md) — §5.5.2, §5.5.4, §4.5.7
- [novel-author-experience.md](novel-author-experience.md) — CLI narrative
- [creator-run-preset-entry.md](creator-run-preset-entry.md) — CLI IA for `rules reset`

**Iteration compass**: [v1.48-novel-quality-loop-completion-delivery-compass-v1.md](../../iterations/v1.48-novel-quality-loop-completion-delivery-compass-v1.md)

> **Authority**: While Status = Draft (V1.48) and the V1.48 compass is active, this overlay wins over conflicting sections in Shipped Masters for **delivery batching**. At P-last, normative text merges into Masters and this file is archived with a superseded stub.

---

## 1. Producer — `review-report.md` parsing (P0)

**Implements**: R-V147P0-01

### 1.1 Input artifact

Path (per Work):

```text
Works/<work_ref>/Logs/review/review-report.md
```

Written by `novel-chapter-review` preset terminal (V1.47). P0 **reads** this file when persisting findings via the supervisor `from-review` path.

### 1.2 Parse contract

When the file exists and parses successfully, each finding persisted MUST map:

| Field | Source in report | Fallback |
| --- | --- | --- |
| `kind` | Report section or frontmatter `kind` | `craft` |
| `severity` | Report `severity` | `info` |
| `body` | Report finding body text | Truncated excerpt from report |
| `rule_suggestion` | Optional per-finding suggestion block | `NULL` |
| `target_executor` | Report routing hint when present | `write` if craft/continuity; else `brainstorm` per §5.5.2 table |

Supported `kind` values remain aligned with [novel-quality-loop.md](novel-quality-loop.md): `craft`, `continuity`, `plot_hole`, `world_inconsistency`, …

Supported `severity` values: `info`, `minor`, `major`, `blocker`.

### 1.3 Fallback behavior

If the file is **missing** or **parse fails**:

1. Persist ≥1 finding using the V1.47 placeholder shape.
2. Emit `tracing::warn!` with `work_id`, `chapter`, `schedule_id`, and parse error summary.
3. Do **not** fail the review terminal solely due to parse failure.

### 1.4 Preset id SSOT

The FL-E review preset id (`novel-chapter-review`) MUST be defined in exactly one orchestration module and imported by: auto-chain hook, `STAGE_PRESET_ALLOWLIST`, supervisor guard (R-V147P0-06).

---

## 2. Consumer — findings → `novel-writing` prompts (P1)

**Implements**: [novel-workflow-profile.md §5.5.2](novel-workflow-profile.md) deferred consumer

### 2.1 Query scope

For `novel-writing` runs with selected chapter `N`:

- Include open findings where `work_id` matches AND (`chapter = N` OR `chapter IS NULL`).
- Order: `severity` DESC (blocker first), then `created_at` ASC.

### 2.2 Prompt injection

Inject template variable `{{ open_findings_block }}` in outline and draft prompt assembly:

```markdown
## Open findings (chapter {{ chapter_label }})

- [{{ severity }}/{{ kind }}] {{ title }}: {{ body_truncated }}
```

**Limits**:

| Limit | Value |
| --- | --- |
| Max findings listed | 8 |
| Max `body` chars per finding | 400 |
| Max total block chars | 3200 |

When no qualifying findings exist, `open_findings_block` MUST be empty string (omit section heading).

### 2.3 Non-goals (P1)

- Do not auto-resolve findings from produce stage.
- Do not inject into finalize `llm_judge` prompt unless a follow-up plan explicitly extends scope.

---

## 3. Rules runtime — Layer 2 `AGENTS.md` (P2)

**Implements**: R-V147P0-04; [novel-workflow-profile.md §5.5.4](novel-workflow-profile.md)

### 3.1 Read path

`read_rules_layers` (or successor) MUST:

1. Read Layer 1 from user override or embedded `writing-craft.md` (unchanged).
2. Read Layer 2 from `Works/<work_ref>/AGENTS.md` when the file exists.
3. If `AGENTS.md` absent, MAY read legacy `Works/<work_ref>/Rules/novel-rules.md` **read-only** for backward compatibility.
4. New scaffolds (`novel-project-init`, bootstrap) MUST create `AGENTS.md` stub, not `Rules/novel-rules.md`.

### 3.2 Accept `rule_suggestion`

Command surface (exact subcommand locked in P2 plan; normative intent):

```bash
nexus42 creator works findings accept <finding_id>
```

Behavior:

1. Load finding; require non-empty `rule_suggestion`.
2. Append a dated markdown section to `Works/<work_ref>/AGENTS.md` under `## Accepted rule suggestions`.
3. Optionally mark finding `status=resolved` (default: resolve on accept).

### 3.3 No bulk migration

Existing Works with only `Rules/novel-rules.md` continue to work via fallback read. No daemon migration job in V1.48.

---

## 4. Rules CLI — reset Layer 2 (P2)

```bash
nexus42 creator run rules-reset <work_id>
```

(or equivalent per [creator-run-preset-entry.md](creator-run-preset-entry.md) IA review in P2)

**Behavior**: Replace `Works/<work_ref>/AGENTS.md` with the embedded default scaffold template for novel Works. MUST NOT delete the Work or chapter artifacts.

---

## 5. Data hygiene — retention and API (P3)

**Implements**: R-V147P0-02, R-V147P0-03

### 5.1 Retention policy

| Rule | Default |
| --- | --- |
| Purge `resolved` / `wont_fix` rows older than | 90 days |
| Never purge | `open` rows |
| Trigger | Daemon daily task OR `creator works findings prune --dry-run` (implementer picks one in P3 T0) |

### 5.2 `rule_suggestion` NULL clear

`FindingPatch` MUST distinguish **omit field** (no change) from **explicit null** (clear column). Document in DAO rustdoc.

---

## 6. Serial cross-ref (P4)

Normative tests and behavior remain in [novel-workflow-profile.md §4.5.7](novel-workflow-profile.md):

| Test | Description | Plan |
| --- | --- | --- |
| #4 | Resume draft row without duplicate | P4 |
| #5 | `reconcile-chapters` filesystem ↔ DB | P4 |

Optional R-V147P1-01 (intake re-trigger on existing Work) documented in P4 plan; not normative in this overlay unless implemented.

---

## 7. Explicit OUT (V1.48)

- Four-state findings lifecycle (`triaged`, `in_review`, …) → V1.49
- Foreshadowing / event-index depth → V1.49
- New wire JSON schemas in `schemas/`
- Platform publish (DF-59)

---

## 8. P-last merge checklist

- [ ] §1 → `novel-quality-loop.md` new § (Producer parsing)
- [ ] §2 → `novel-workflow-profile.md` §5.5.2 (consumer normative)
- [ ] §3–§4 → `novel-workflow-profile.md` §5.5.4 + `novel-author-experience.md` §3
- [ ] §5 → `novel-quality-loop.md` retention §
- [ ] Archive this overlay to `.mstar/archived/knowledge/` with stub pointer

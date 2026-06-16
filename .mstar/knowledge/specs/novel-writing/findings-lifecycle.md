# Novel Findings Lifecycle — Draft Overlay v1.49

**Status**: Draft (V1.49)  
**Document class**: Draft overlay  
**Created**: 2026-06-17  
**Last updated**: 2026-06-17  
**Scope**: Extended findings status lifecycle (F6) — supersedes three-state minimum in [quality-loop.md](quality-loop.md) §2 for V1.49 implement wave  
**Coordinates with**:

- [quality-loop.md](quality-loop.md) — merge target §2 at P-last
- [author-experience.md](author-experience.md) — §4 visibility surfaces

**Iteration compass**: [v1.49-novel-narrative-maturity-and-author-desk-delivery-compass-v1.md](../../iterations/v1.49-novel-narrative-maturity-and-author-desk-delivery-compass-v1.md)

---

## 1. Purpose

V1.39 shipped a minimal three-state findings model (`open` / `resolved` / `wont_fix`). V1.48 closed the producer→consumer chain. V1.49 extends status to support author/reviewer triage without losing V1.48 consumer behavior.

---

## 2. Status enum (normative V1.49)

| Status | Meaning |
| --- | --- |
| `open` | New finding; not yet triaged |
| `triaged` | Reviewed; actionable for write/brainstorm routing |
| `in_review` | Under master review (`novel-review-master` active) |
| `resolved` | Addressed; eligible for retention prune |
| `wont_fix` | Explicitly waived; never pruned by retention DAO |
| `duplicate` | Superseded by another finding; terminal |

### 2.1 Allowed transitions

```text
open → triaged | in_review | resolved | wont_fix | duplicate
triaged → in_review | resolved | wont_fix | duplicate
in_review → resolved | wont_fix | duplicate
resolved → (terminal; may be pruned by retention policy)
wont_fix → (terminal)
duplicate → (terminal)
```

Invalid transitions return `422` with stable error code on Local API.

### 2.2 Actionable set for prompt consumer

Findings with status ∈ `{ open, triaged }` are included in `open_findings_block` (V1.48 naming preserved). Status `in_review` is **excluded** from produce prompts unless a future spec amends.

### 2.3 Migration

Existing rows remain valid. No automatic status rewrite on migration. Default for new rows: `open`.

---

## 3. API / CLI (minimum)

| Surface | Requirement |
| --- | --- |
| PATCH finding | Accept new status values; validate transitions |
| `creator works status --json` | Expose new status strings verbatim |
| Optional | `creator works findings triage <id>` convenience wrapper — may defer to PATCH if scope tight |

---

## 4. P-last merge

Fold §2–§3 into [quality-loop.md](quality-loop.md) §2. Archive this overlay with `Superseded by: quality-loop.md §2 (V1.49)`.

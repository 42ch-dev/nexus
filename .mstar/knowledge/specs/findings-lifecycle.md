# Findings Lifecycle — Cross-Profile Master

**Status**: Normative — V1.77 Phase 2b (promoted from `novel-writing/findings-lifecycle.md` stub)  
**Document class**: Master  
**Scope**: Cross-profile findings lifecycle — 6-state status machine, `target_executor` routing semantics, and the UI remediation surface consuming the Local API PATCH route. Quality-loop produce side (review verdicts, finding creation, orchestration hooks) is owned by [novel-writing/quality-loop.md](novel-writing/quality-loop.md) §2 and is not duplicated here.  
**Coordinates with**:
- [novel-writing/quality-loop.md](novel-writing/quality-loop.md) — producer side (review verdicts, finding creation, orchestration hooks, retention/prune)
- [local-api-surface-conventions.md](local-api-surface-conventions.md) — findings PATCH surface conventions
- [web-ui.md](web-ui.md) — Control-Room findings view and V1.77 remediation surface
- [`crates/nexus-local-db/src/findings.rs`](../../crates/nexus-local-db/src/findings.rs) — DAO enforcement (canonical `is_valid_transition`, `VALID_STATUSES`, `enforce_status_transition`)
- [`crates/nexus-daemon-runtime/src/api/handlers/findings.rs`](../../crates/nexus-daemon-runtime/src/api/handlers/findings.rs) — PATCH handler error mapping

**Supersedes**: [`novel-writing/findings-lifecycle.md`](novel-writing/findings-lifecycle.md) (pointer stub — full text archived at `../../archived/knowledge/novel-writing/findings-lifecycle.md`).

---

## 1. Purpose

The findings lifecycle is a cross-profile 6-state status machine that all Work profiles consume. The backend (V1.49+) enforces the lifecycle server-side on every `PATCH /v1/local/works/{work_id}/findings/{finding_id}`, rejecting illegal transitions with HTTP 422 `INVALID_TRANSITION`. The V1.77 UI promotion adds a remediation authoring surface that consumes this existing PATCH route.

This Master documents the lifecycle adjacency rules, `target_executor` routing semantics, and the UI remediation surface. The produce side — how findings are created (review verdicts, `from-review` hooks, orchestration-synthesized findings) — is owned by the quality-loop spec ([novel-writing/quality-loop.md](novel-writing/quality-loop.md) §2) and is not repeated here.

---

## 2. Status lifecycle

### 2.1 Status values (6-state)

| Status | Meaning | Actionable for produce? |
|--------|---------|------------------------|
| `open` | New finding; not yet triaged | Yes |
| `triaged` | Reviewed; actionable for write/brainstorm routing | Yes |
| `in_review` | Under master review (`novel-review-master` active) | No (excluded from produce prompts) |
| `resolved` | Addressed; eligible for retention prune (90-day default) | No (terminal) |
| `wont_fix` | Explicitly waived; never pruned by retention DAO | No (terminal) |
| `duplicate` | Superseded by another finding; terminal | No (terminal) |

The actionable set for produce-prompt consumers is `{ open, triaged }` (`crates/nexus-local-db/src/findings.rs:132` — `ACTIONABLE_FINDING_STATUSES`). `in_review` findings are excluded by default (the master-review preset owns that surface). Terminal statuses (`resolved`, `wont_fix`, `duplicate`) are excluded.

### 2.2 Lifecycle adjacency (SERVER-SIDE ENFORCED)

The DAO enforces the following transition table on every `status` PATCH. Attempts to transition outside this table are rejected with `LocalDbError::IllegalTransition` → HTTP 422 `INVALID_TRANSITION`.

```text
open       → triaged | in_review | resolved | wont_fix | duplicate
triaged    → in_review | resolved | wont_fix | duplicate
in_review  → resolved | wont_fix | duplicate
resolved   → (terminal; may be pruned by retention policy)
wont_fix   → (terminal)
duplicate  → (terminal)
```

**Self-loop rejection**: `status: "<current>"` (e.g. `open → open`) is rejected as `INVALID_TRANSITION`. Callers that only want to refresh `updated_at` must omit `status` from the patch body entirely.

**Enforcement site**: `crates/nexus-local-db/src/findings.rs:172-189` (`is_valid_transition()`) + `findings.rs:863-895` (`enforce_status_transition()`) + `findings.rs:974-976` (called from `update_finding()`). Handler mapping: `handlers/findings.rs:402-421`.

### 2.3 Enum membership validation

In addition to transition guards, the DAO validates that any patched value is a member of the allowed enum sets:

- **Status**: `VALID_STATUSES` — `open`, `triaged`, `in_review`, `resolved`, `wont_fix`, `duplicate` (`findings.rs:116-123`)
- **Severity**: `VALID_SEVERITIES` — `info`, `minor`, `major`, `blocker` (`findings.rs:104`)
- **Target executor**: `VALID_TARGET_EXECUTORS` — `write`, `brainstorm`, `none`, `master` (`findings.rs:192`)

Invalid enum values → `LocalDbError::InvalidEnum` → HTTP 422 `INVALID_INPUT` (handler:394-441).

---

## 3. `target_executor` routing

| Value | Preset / action | Meaning |
|-------|----------------|---------|
| `write` | `novel-writing` (`produce`) | Re-run or continue the writing preset |
| `brainstorm` | `novel-brainstorm` | Re-run brainstorming |
| `master` | `novel-review-master` | Re-run the master review preset |
| `none` | Manual resolution | Author resolves manually; no auto-routing |

`target_executor` is an **assignment hint** — a routing suggestion for the author or the auto-chain consumer. It does not automatically trigger a re-run. Re-running a preset from a finding remains a deliberate canvas/CLI action (the canvas is the intended steering surface for re-runs; `target_executor` is metadata, not an auto-trigger).

---

## 4. UI remediation surface (V1.77)

### 4.1 Route consumed

`PATCH /v1/local/works/{work_id}/findings/{finding_id}` — the update-finding route.

Request body fields (all optional on the wire):

| Field | Type | PATCH semantics |
|-------|------|----------------|
| `severity` | string | Replace severity (must be in `VALID_SEVERITIES`) |
| `status` | string | Transition status (server-enforced adjacency §2.2) |
| `title` | string | Replace title |
| `description` | string | Replace description |
| `target_executor` | string | Replace routing hint (must be in `VALID_TARGET_EXECUTORS`) |
| `kind` | string | Replace finding category |
| `rule_suggestion` | string | Replace or clear rule suggestion (string on the wire — `update-finding-request.schema.json` `"type": "string"`; an empty string clears, omitting leaves it unchanged) |

### 4.2 Three remediation affordances

1. **Status transitions** — inline status dropdown or action buttons driving the 6-state machine (§2.2). Invalid transitions are disabled client-side (defense-in-depth + UX); the server rejects any that bypass the UI.
2. **`target_executor` assignment** — dropdown/selector routing to `brainstorm`/`write`/`master`/`none` (§3).
3. **Inline edit** — title/description/severity/kind/rule_suggestion editable in a finding detail/inspector panel.

### 4.3 UX layout

**Detail-panel + row-action hybrid.** The findings page is a Control-Room table (not a canvas graph), so a detail/inspector panel with the three remediation affordances + row-level status/severity badges is the default, reusing existing `Table` + `StatusBadge` components.

### 4.4 OCC / conflict policy

**Last-writer-wins.** The findings table has no `revision` column, no OCC version field, and no conflict detection. The PATCH is a simple `UPDATE findings SET ... WHERE creator_id = ? AND finding_id = ?` with no compare-and-swap predicate (`crates/nexus-local-db/src/findings.rs:1037-1039`). The transition-guard read-before-write is best-effort under SQLite's serialized writer. No conflict modal is needed — the quality loop is single-author-triage (the author triages their own findings; the producer writes, the author triages, no concurrent-author conflict scenario).

---

## 5. Non-goals (V1.77)

- **No one-click orchestration re-trigger from a finding** — re-running a preset stays a deliberate canvas/CLI action. `target_executor` is an assignment hint, not an auto-trigger.
- **No findings producer changes** — this Master documents the *consumer* side (triage/remediation). Finding creation, review verdicts, and orchestration hooks are owned by [novel-writing/quality-loop.md](novel-writing/quality-loop.md).
- **No create/delete from the UI** — the V1.77 lead surface is update-only. `createFinding`/`deleteFinding` remain CLI/producer-only for now; the web app can add them in a follow-up if UX demands in-UI finding creation.

# Daemon API Surface Conventions — Amendment History — Historical Notes

**Provenance**: frozen from [`specs/active/runtime/daemon-api-surface-conventions.md`](../../active/runtime/daemon-api-surface-conventions.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans the V1.63 audit → V1.64/V1.65/V1.66/V1.67/V1.71–V1.77 amendment chain up to the v1.198 World-rules amendment, but no single calendar date is uncontested, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 5 (Status — amendment ledger)

```markdown
| **Status** | Normative — V1.77 amendment (§11 findings PATCH route as a non-OCC resource PATCH; last-writer-wins, no revision/version, no conflict modal; cross-profile consumption). Prior: V1.74 amendment (§7.6 World KB relationship patch route extending the V1.73 World KB route pattern; additive relationship DTOs; per-row OCC with `expected_version`/`version` against `kb_relationships.revision`), V1.73 amendment (§7 World KB canvas structured patch/read routes extending the V1.71/V1.72 patch-route convention; additive World KB DTOs; per-row OCC with `expected_version`/`version`), V1.72 amendment (§7 outline/timeline structured patch routes extending the V1.71 patch-route convention; additive outline DTOs; `@42ch/nexus-contracts` 0.7.0 → 0.8.0 by default), ...
```

### at pre-freeze line 7 (Scope)

```markdown
| **Scope** | Cross-resource Daemon API response/query conventions for schemas under `schemas/daemon-api/` and the serving route families in `apps/nexus-service` over `nexus-core` (pre-v1.193 references to `nexus-daemon-runtime` handlers below are historical) |
```

### at pre-freeze line 56 (§2.2 Legacy pattern — offset/limit)

```markdown
Offset/limit plus `total` is legacy. It exists because Works was the first promoted Daemon API list surface. V1.64 migrates Works pagination to cursor semantics (F-P1). New endpoints MUST NOT introduce offset/limit pagination.
```

### at pre-freeze lines 72–74 / 93–95 (§3.1 Error envelope)

```markdown
**wrapped envelope** — today `apps/nexus-service` (`src/errors.ts`,
`mapNativeError`); the pre-v1.193 emitter was `ApiErrorResponse` in
`crates/nexus-daemon-runtime/src/api/errors.rs` (deleted with the crate):

- `error.request_id`: correlation ID injected by the request-tracing middleware
  when active (today the `apps/nexus-service` error mapping in `src/errors.ts`;
  the pre-v1.193 injector was `crates/nexus-daemon-runtime/src/api/middleware.rs`);
```

### at pre-freeze lines 108–115 (§3.1 Implementation note)

```markdown
> **Implementation note:** transport adapters and consumers MUST read
> `code`/`message`/`details` from `body.error`, not from the top level. V1.67
> P0 (FE1-ORCH) **resolves** the ad-hoc `(StatusCode, String)` bodies that
> remained in orchestration handlers (`schedules.rs` / `sessions.rs` /
> `presets.rs`): they are swept onto this canonical envelope via a shared
> error-mapping helper. After V1.67, ad-hoc tuple bodies are not an allowed
> Daemon API surface; clients no longer need the `http_<status>` fallback for
> orchestration routes.
```

### at pre-freeze line 137 (§3.2 Error-code naming ratification)

```markdown
> **V1.67 ratification (`@42ch/nexus-contracts` 0.5.0 → 0.6.0):** the canonical `NexusApiError::error_code()` module is aligned to lowercase `snake_case` **globally** — it previously emitted `UPPER_SNAKE_CASE`, contradicting this section. The ~18 canonical codes (e.g. `UNINITIALIZED`→`uninitialized`, `INVALID_INPUT`→`invalid_input`, `INTERNAL`→`internal`, `AUTH_REQUIRED`→`auth_required`, `NOT_FOUND`→`not_found`) change in the 0.6.0 bump. This is a global error-module change, not an orchestration-handler-local one.
```

### at pre-freeze lines 145–156 (§4 List-array naming)

```markdown
> **V1.67 ratification (F-P3, `@42ch/nexus-contracts` 0.5.0 → 0.6.0):** the prior legacy allowance for schema-backed plain lists is **removed**. V1.67 P0 renames the four remaining schema-backed per-resource keys to `items`:

| Key | Resource | V1.67 status |
| --- | --- | --- |
| `works` | Works list | **Renamed → `items`** (`GET /v1/daemon/works`). |
| `schedules` | Schedule list | **Renamed → `items`** (`GET /v1/daemon/orchestration/schedules`). |
| `sessions` | Orchestration sessions list | **Renamed → `items`** (`GET /v1/daemon/orchestration/sessions`). |
| `capabilities` | Capability registry list | **Renamed → `items`** (`GET /v1/daemon/orchestration/capabilities`). |
...

Pre-1.0 → no compatibility shim. Hand-written / local-only plain lists not yet schema-backed (`references`, `worlds`, `fragments`, `pool entries`, orchestration `presets` DTO) are **out of the 0.6.0 bump**; they adopt `items` at their next schema-promotion or feature touch.
```

### at pre-freeze line 164 (§5 Sort parameters)

```markdown
> **V1.67 lock (F-F1):** the prior `sort_by` + `sort_order` two-parameter sketch is **superseded** by a single optional `sort` query parameter.
```

### at pre-freeze line 192 (§5)

```markdown
V1.67 implements server-side sort on: works (`updated_at`/`title`/`status`/`intake_status`), schedules (`created_at`/`updated_at`/`status`/`preset_id`/`label`), sessions (`session_id`/`creator_id`/`preset_id`/`status`), capabilities (`name`). Chapters document/accept default `volume,chapter` only (cursor-keyed). Other lists adopt on next schema-promotion.
```

### at pre-freeze lines 198–200 / 223–237 (§6 File-backed chapter-content routes)

```markdown
V1.65 introduces the chapter-content Daemon API surface under
`/v1/daemon/works/{work_id}/chapters/*`; detailed field contracts live in

### 6.2 Outline-prose `set.content` persistence is atomic file write + DB metadata update (V1.75)

The V1.65 `PUT /v1/daemon/works/{work_id}/chapters/{n}/outline` whole-document
write route is **removed in V1.75** (canvas-pivot). Outline prose writes now go
through the canvas patch route
`POST /v1/daemon/works/{work_id}/chapters/{chapter_id}/patch` with `set.content`
+ `base_revision` (`outline_revision` CAS) — see §7 (V1.75 amendment) and
[canvas-strategy-surface.md](../../active/surfaces/canvas-strategy-surface.md) §3.5. The atomic-write
invariants that this section historically attached to the removed PUT are still
normative; they are re-anchored to the PATCH content path below and enforced by
`crates/nexus-core/src/outline.rs::apply_chapter_patch`
(the `atomic_write_outline` call: sibling temp + rename + file fsync + directory
fsync, run while the caller holds a `RuntimeLockGuard`). The pre-v1.193 enforcer
was `crates/nexus-daemon-runtime/src/api/handlers/outline.rs`, deleted with the
crate; the route is served today by `apps/nexus-service` (`content.ts`).
```

### at pre-freeze lines 270–293 (§6.3 / §6.4 V1.65 wording)

```markdown
`PATCH /v1/daemon/works/{work_id}/chapters/{n}` is the V1.65 structure-edit route
...
- The only normal UI-driven status progression in V1.65 is
  `not_started → outlined`.
...
- `published` structural edits are hard-blocked in V1.65 unless a future
  publish-retraction design changes the policy.
- Chapter deletion is out of scope for V1.65; any future delete route MUST
  hard-block `finalized` and `published` chapters.

### 6.4 Body markdown is read-only in V1.65
...
optional parsed frontmatter for rendering, but V1.65 MUST NOT add a body write
route. The orchestration host-tool path remains the body writer until V1.66
designs a per-chapter edit lock and conflict policy.
```

### at pre-freeze line 310 and 315 (§6.5 / §6.6)

```markdown
`host_tool_handlers.rs` for body reads and applies it to outline writes (V1.75:
the `outline.patch_chapter set.content` path — see §6.2).

V1.65 has no hard per-chapter edit lock. Outline writes are last-write-wins at
```

### at pre-freeze lines 328–451 (§7 patch-route narration)

```markdown
## 7. Structured patch routes for canvas-like surfaces (V1.71)

V1.71 promotes the Strategy canvas write-boundary from paper contract to Daemon API convention. New node/edge edit surfaces ...

> **V1.75 amendment — outline `content` field.** ... The V1.65 `PUT /chapters/{n}/outline` whole-document write route is removed in the same bump; the canvas patch route is now the sole outline write path.

V1.71 Strategy uses a `revision:` key in the preset YAML header; ...
V1.72 Outline+Timeline canvas surfaces reuse this convention ...
V1.72 adds 3 outline/timeline patch routes following the V1.71 pattern:
Error envelope (mirrors V1.71 Strategy convention): ... mirroring V1.71's `StrategyValidationError`.
V1.73 World KB canvas surface reuses this convention ...
... like the V1.65 chapter-content routes in §6.
### 7.5 World KB canvas β routes (V1.73)
V1.73 adds the World KB entities + candidates surface to the structured canvas convention. It intentionally ships **2 mutating patch routes** plus **2 read projection routes**; ...
V1.73 graph reads expose source-anchor provenance edges and reserve `relationships` as an empty array until the V1.74 relationship surface.
### 7.6 World KB relationship patch route (V1.74)
V1.74 extends the World KB canvas β routes with a first-class relationship patch route and with populated relationship projections on the existing graph read.
**V1.76 `needs_review` + `source` (extraction suggestions):** ...
```

### at pre-freeze lines 509–544 (§9 Evidence and V1.64 decisions / §10 Daemon port discovery)

```markdown
## 9. Evidence and V1.64 decisions
The V1.63 Daemon API surface audit (historical name) identified:
...
V1.64 closes F-P1, F-P2, and F-E1 for the Web UI data-layer baseline, while documenting F-P3 and F-F1 as future conventions with adapter coverage for MVP.

## 10. Daemon port discovery (V1.66 desktop shell)
Daemon API clients that connect over loopback HTTP use a **resolved daemon base URL**, not a schema-defined discovery endpoint. (Compass: v1.66 §5 #3 LOCKED.)
V1.66 desktop-shell convention:
6. **V1.66 does not introduce a dynamic port handshake endpoint or daemon-lifecycle Daemon API schema** (`wire_contracts_changed: false`).
```

### at pre-freeze lines 548–595 (§11 Findings PATCH route)

```markdown
## 11. Findings PATCH route (V1.77 — non-OCC resource PATCH)
... It is consumed by the V1.77 findings-remediation UI surface ...
- **Schema on disk**: `schemas/daemon-api/findings/update-finding-request.schema.json` (unchanged since V1.49).
... the pre-v1.193 handler was `update_finding_handler` in `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:380-451` (deleted with the crate).
- **`wire_contracts_changed`**: `FALSE` for V1.77 — the route, schema, codegen, and handler are unchanged. Only consumer-side consumption is additive.
```

### at pre-freeze lines 598–682 (§12)

```markdown
## 12. V1.114 compute registry and KB state read routes
V1.114 adds two read-only surfaces to the Daemon API inventory.
### 12.3 V1.147 compute direct lane and World timeline events routes
V1.147 adds the **direct Control Room compute lane** — invoke/accept/discard of installed WASM compute modules against owned Worlds over HTTP — plus a per-World timeline-events read. ...
**Error-code family (V1.147 compute lane)** — ...
... misleading 500 (V1.147 P3 F2 reclassification).
```

### at pre-freeze line 686 (§13)

```markdown
**Normative amendment (v1.198; accepted target, implementation pending).**
```

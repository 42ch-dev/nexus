---
module: crates/nexus-daemon-runtime
date: 2026-07-05
problem_type: api_design
category: api-design
severity: low
applies_when:
  - Adding a bulk helper endpoint that mutates multiple rows owned by the same creator
  - The underlying single-row DAO already enforces authz, enum validation, and lifecycle transitions
  - A cap is needed to keep local SQLite latency bounded
  - Partial success is acceptable for the first shipped version
tags:
  - daemon-api
  - findings
  - batch-patch
  - partial-success
  - additive-endpoint
  - sqlite
---

# Additive Batch PATCH Helper with Partial Success

Add a single bulk mutation endpoint that reuses the existing audited single-row DAO, caps the request size, and returns per-ID outcome arrays instead of failing the whole batch on validation errors.

## Context

The single-finding remediation UI worked, but authors with dozens of findings paid a high click cost. The V1.91 companion feature added multi-select batch status transition and executor assignment. The constraints were:

- No change to the findings state machine, enum values, or transition rules.
- No new backend export job; CSV export stayed client-side from the filtered list.
- The endpoint must be additive: existing single-finding PATCH remains untouched.
- Local SQLite is single-writer, so a giant multi-row transaction was not obviously better than a bounded loop.

## Guidance

### 1. Reuse the existing single-row DAO

The bulk handler should not reimplement authz, enum validation, or lifecycle-transition enforcement. Delegate each ID to the same DAO used by the single-finding endpoint:

```rust
for finding_id in &body.finding_ids {
    match findings::update_finding(state.pool(), &creator_id, finding_id, &finding_patch, now).await {
        Ok(true) => updated += 1,
        Ok(false) => not_found.push(finding_id.clone()),
        Err(LocalDbError::IllegalTransition { .. }) => conflict.push(finding_id.clone()),
        Err(other) => {
            tracing::warn!(...);
            return Err(other.into());
        }
    }
}
```

Reusing the DAO guarantees that bulk and single-finding behavior cannot drift.

### 2. Enforce a hard cap before touching the database

Return a typed 422 if the request exceeds the cap so the client can chunk or warn:

```rust
const BATCH_CAP: usize = 100;

if body.finding_ids.len() > BATCH_CAP {
    return Err(NexusApiError::BadRequest {
        code: "too_many_findings".to_string(),
        message: format!("batch update is capped at {BATCH_CAP} findings; received {}", body.finding_ids.len()),
    });
}
```

This keeps worst-case latency predictable on the local SQLite pool.

### 3. Return partial-success arrays, not HTTP errors, for expected failure classes

For `not_found` and `conflict` (illegal transition), collect IDs and return them in the success response. The client can decide whether to toast, retry, or surface a persistent banner:

```rust
Ok(Json(BatchUpdateFindingsResponse {
    updated,
    not_found: if not_found.is_empty() { None } else { Some(not_found) },
    conflict: if conflict.is_empty() { None } else { Some(conflict) },
}))
```

Unexpected internal errors still abort and return 5xx, because they indicate a problem the client cannot reconcile locally.

### 4. Keep the patch shape additive and versioned

The request body should accept only the fields the bulk action supports. In V1.91 this was `status` and `target_executor`; other finding fields stayed on the single-finding PATCH. If the generated contract widens the patch sub-object (for example, codegen emits `serde_json::Value`), add a small runtime deserializer that enforces the concrete shape:

```rust
let patch: BatchFindingPatch = serde_json::from_value(body.patch)
    .map_err(|e| NexusApiError::BadRequest { code: "invalid_input".to_string(), message: ... })?;
```

Document the codegen gap and plan to replace the hand-rolled struct with a generated type once the schema/codegen pipeline supports the shape.

### 5. Wire the UI for idempotent retry

On the client, keep `selectedIds` selected when the mutation errors so the user can retry with the same selection. On success, clear the selection. This pairs naturally with the idempotent transition rules enforced by the DAO.

## Why This Matters

A bulk helper is easy to over-engineer: full transactions, complex filter DSL, server-side export jobs, or new state-machine edges. The partial-success loop keeps the scope surgical (one handler, one UI bar, one client CSV helper) while still giving power users a real triage workflow. It also avoids duplicating validation logic, which is the most common source of bulk/single drift.

## When to Apply

- The single-row DAO already captures the business rules you need.
- Each row is independently owned and scoped (for example, by `creator_id`).
- Partial success for validation conflicts is acceptable; only unexpected internal errors abort the batch.
- The dataset is bounded enough that a capped sequential loop is not a perf defect.

Do not use this pattern when the batch must be atomic, when conflicts require a complex reconciliation UI, or when cross-row constraints exist.

## Examples

### Request

```json
PATCH /v1/daemon/findings/batch
{
  "finding_ids": ["find_01", "find_02", "find_03"],
  "patch": { "status": "triaged" }
}
```

### Response (mixed result)

```json
{
  "updated": 1,
  "not_found": ["find_03"],
  "conflict": ["find_02"]
}
```

### Client-side CSV export from the filtered list

Because the filtered list is already in memory, export it directly without calling the server:

```ts
function downloadFindingsCsv(rows: FindingListItem[]) {
  const lines = [
    CSV_COLUMNS.join(','),
    ...rows.map((row) => CSV_COLUMNS.map((col) => csvField(row[col])).join(',')),
  ];
  const blob = new Blob([lines.join('\n')], { type: 'text/csv' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `findings-${Date.now()}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}
```

Keep CSV helpers in a dedicated module so the page stays under the project module-size cap.

## Related

- `crates/nexus-daemon-runtime/src/api/handlers/findings.rs` — `batch_update_findings_handler`
- `crates/nexus-daemon-runtime/tests/findings_api.rs`
- `schemas/daemon-api/findings/batch-update-findings-request.schema.json`
- `schemas/daemon-api/findings/batch-update-findings-response.schema.json`
- `apps/web/src/pages/findings-page.tsx`

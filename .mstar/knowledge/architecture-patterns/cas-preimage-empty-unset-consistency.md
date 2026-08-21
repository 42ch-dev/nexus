---
module: nexus-daemon-runtime + nexus-local-db + apps/web
date: 2026-08-21
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
applies_when:
  - "Designing a CAS (compare-and-swap) write endpoint whose pre-image is a raw stored blob"
  - "A stored column treats NULL and empty string as the same 'unset' state"
  - "A client reconstructs the CAS pre-image from a parsed/derived value instead of the raw stored bytes"
tags: [cas, optimistic-concurrency, pre-image, empty-string, unset, is-default, 409, sqlite]
---

# CAS Pre-Image Consistency: Empty ≡ Unset, Client Reconstructs From Raw

## Context

V1.171 P2 (AR-29) added `PUT /v1/daemon/works/{work_id}/cron` writing
`works.schedule_json` through `set_schedule_json_tx` (CAS). The stored column
treats **NULL and empty string as the same "unset" state** (the DAO
`COALESCE(schedule_json,'')` normalizes them). The GET endpoint returned an
`is_default` marker computed as `stored.is_none()` — which contradicted the
invariant for a stored empty string — and the web client reconstructed the CAS
pre-image from the **parsed** response (`JSON.stringify({tz, roles})`), not
from the raw stored bytes. Two coordinated bugs resulted (QC3 W-2):

1. Stored `""` → GET said `is_default: false` → client sent a canonical JSON
   pre-image → pre-check 409'd before the tx CAS (which would have accepted
   `''` vs `""`).
2. Malformed non-empty stored blob → GET resolved defaults but `is_default:
   false` → client pre-image could never byte-match the stored garbage → a
   permanent 409 loop with no escape.

## Guidance

1. **Derive the marker from the same invariant the CAS uses.** If the storage
   layer treats `NULL ≡ ""` as unset, the marker must too:
   `is_default = stored.as_deref().is_none_or(|s| s.is_empty())`.
2. **One authority for the CAS.** Either drop the handler-side pre-check and
   let the transaction-level CAS be the single authority (it distinguishes
   missing-work → 404 from mismatch → 409 via the re-read), or align the
   pre-check with the DAO's `COALESCE` normalization. Two layers of CAS logic
   that disagree is the bug.
3. **A client that reconstructs a pre-image from parsed fields cannot
   byte-match arbitrary stored bytes.** Either (a) return the raw stored blob
   in GET so the client round-trips it verbatim, or (b) reject unparseable
   stored blobs with a stable 400 code (`E_CRON_INVALID_STORED`) so the UI
   shows "stored config unreadable — repair via CLI" instead of a 409 loop.
   Never let the client fabricate a pre-image from derived values.
4. **Test the empty-string round-trip explicitly**: stored `""` → GET
   `is_default: true`; PUT with `expected_current_json: ""` against `Some("")`
   succeeds. Also test a serde-reconstructed pre-image (JSON.stringify
   equivalent) to guard key-order drift.

```ts
// web dialog — pre-image from the GET response
const preimage = isDefault ? '' : JSON.stringify({ tz, roles }); // ← only safe if
// is_default uses the same empty≡unset invariant AND stored blobs are canonical
```

## Why This Matters

- **The marker and the CAS must agree on what "unset" means** — a mismatch
  turns a benign empty blob into an unrecoverable 409 loop (the client can
  never produce a matching pre-image).
- **CAS is byte-exact by design** — any client-side reconstruction from parsed
  data is a latent mismatch. The raw blob (or a stable rejection) is the only
  honest contract.
- This is the same class as `verify-stored-row-scope-before-cas-write.md`:
  the CAS protects against *stale* writes, never against *wrong* pre-images —
  the pre-image contract must be designed, not assumed.

## When to Apply

- Any CAS endpoint where the pre-image is a raw JSON/text blob stored in a
  column with NULL/empty normalization.
- Any GET+PUT pair where the client must echo a stored value back as a
  precondition.
- Reviewing a 409-conflict flow for an escape hatch (can the client ever
  produce a matching pre-image?).

## Examples

### Before
```rust
// get_work_cron
let is_default = stored.is_none(); // "" stored → false, contradicts invariant
// handler pre-check
Some(_) if stored.is_some() => 409  // "" stored + "" pre-image → 409 before tx CAS
```

### After
```rust
// WorkSchedule::resolve returns (schedule, is_default) with empty≡unset
let (schedule, is_default) = WorkSchedule::resolve(stored.as_deref());
// handler pre-check dropped — tx-level CAS (COALESCE) is the single authority
// malformed non-empty stored → 400 E_CRON_INVALID_STORED (honest, recoverable)
```

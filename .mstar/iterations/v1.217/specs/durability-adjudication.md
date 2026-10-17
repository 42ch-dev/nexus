# Durability adjudication — I-000009 / I-000010 (v1.217)

Report-only adjudication of two low-severity, trigger-gated leftover obligations.
No production code is changed by this document's plan.

## Method

Read both subjects and their consumers:

- `scripts/public-first-workflow.mjs` — `artifactIdentity` (`:1442`), `sameArtifact`
  (`:1458`), `ARTIFACT_HASH_MAX_BYTES` (`:383`), `localArtifactIdentities` (`:1478`),
  `assertDeterministicReceipt` (`:1504`), live authorization call site (`:3836`–`:3840`),
  `readAttemptDir` (`:1174`), `readSpentToken` (`:1299`), `assertGuardAttempt` (`:1356`).
- `scripts/public-first-workflow-request-guard.mjs` — `writeEvent` (`:404`),
  `writeMarker` (`:430`), `spend` (`:502`), admitted dispatch (`:606`).

The threat surface was established from the call sites, not assumed.

---

## I-000009 — Large artifact identity checks size and mtime without content digest

Stored acceptance: *"Before a future expansion of live-request qualification beyond
the frozen installed runtime"*. Revision 1, low, `review-obligation`, open.

### Threat surface (who can replace the artifact)

`artifactIdentity` (`public-first-workflow.mjs:1442`–`:1457`) hashes a file only when
`stat.size <= ARTIFACT_HASH_MAX_BYTES` (8 MiB, `:383`); above that it records
`path` + `bytes` + floored `mtime_ms` and sets `sha256: null` (`:1453`). `sameArtifact`
(`:1458`–`:1468`) compares path, presence, `bytes`, `sha256`, and — only for the
un-hashed (large) case — `mtime_ms` (`:1466`).

The identities cover the prepared inputs `driver, guard, fixture, service_entry, cli,
dsh, dsh_transport` (`localArtifactIdentities`, `:1478`–`:1492`). The only routinely
oversized member is `cli` — the `nexus42` CLI binary (the source comment names
"~175 MB", `:1436`–`:1441`), resolved from `NEXUS42_BIN` or `PATH` (`:3820`–`:3822`).

Consumption: the live mode re-checks the deterministic receipt's recorded identities
against what is on disk NOW — `assertDeterministicReceipt(..., { current: artifacts })`
(`:3838`–`:3840`), mismatch filter at `:1542`. So the identity is used to answer one
question: "do the artifacts on disk still match the ones the deterministic receipt was
produced against?" The actor who could exploit the gap is therefore someone who can
write the binary in place AND preserve both byte size and mtime (e.g. `utimes`/`rsync
--times`). That is deliberate local tampering with operator-controlled prepared
artifacts — not a remote or incidental path.

### Trigger definition

The acceptance fires on any change that expands live-request qualification beyond the
frozen installed runtime — concretely, any of:

1. `NEXUS42_BIN`/`DSH_RUNTIME_BIN` resolution (`:3820`–`:3830`) is widened to accept a
   non-frozen, operator-supplied, or PATH-resolved binary as a *qualified* artifact
   without the deterministic→live same-artifact re-check;
2. `facts.artifacts` (or `artifactIdentity` output) is consumed by any decision other
   than the `sameArtifact` re-check of the same frozen pair — i.e. the identity is
   promoted from "staleness detector" to "integrity claim";
3. `ARTIFACT_HASH_MAX_BYTES` (`:383`) is raised beyond 8 MiB, or the large-file branch
   is otherwise made to feed a qualification path, so un-hashed artifacts become
   load-bearing;
4. a spec/doc begins asserting the receipt proves *content* integrity of large artifacts.

While the qualification stays confined to re-checking the frozen installed pair against
its own deterministic receipt, this is a staleness check, not an integrity guarantee,
and the trigger has not fired.

### Fix-cost estimate

Files: `scripts/public-first-workflow.mjs` (only `artifactIdentity` `:1442`–`:1457`,
`sameArtifact` `:1458`–`:1468`, and optionally `ARTIFACT_HASH_MAX_BYTES` `:383`), plus
`scripts/public-first-workflow.test.mjs`.

Approach (content digest): stream `sha256` for all sizes and always populate `sha256`,
then make `sameArtifact` compare `sha256` unconditionally (drop the `mtime` fallback at
`:1466`). Small diff (~10 lines) and no receipt-format change (`sha256` is already a
receipt field). The dominant cost is not the code but the runtime: each qualification
would re-hash a ~175 MB binary in both deterministic and live modes. Alternative if
perf matters: keep the size threshold but add a streaming digest for large files, or
add cheap provenance (path + size + creation/inode) instead of mtime.

### Verdict

**keep-defer.**

Rationale: the acceptance trigger ("expansion of live-request qualification beyond the
frozen installed runtime") has not fired. Qualification today is the frozen-pair
re-check (`:3838`–`:3840`) against the receipt (`assertDeterministicReceipt` `:1504`,
mismatch filter `:1542`), and the un-hashed member is the operator-prepared CLI binary.
The residual exposure requires an actor who can rewrite that binary while preserving
size and mtime; against that actor the mtime check was never the intended control. The
fix is small in code but adds a full re-hash of a ~175 MB file to every live
qualification — a real cost not justified by the current, un-fired exposure. Re-open,
do not close: the trigger conditions in §Trigger are precise and cheap to re-test.

---

## I-000010 — One-request spent token lacks power-loss durability guarantee

Stored acceptance: *"Before future power-loss-safe request budgeting is claimed"*.
Revision 1, low, `review-obligation`, open.

### Threat surface (which budgeting decisions consume the spend record)

The guard acquires the single attempt slot with an exclusive create: `writeFileSync(
spentPath, payload, { flag: 'wx', mode: 0o600 })` (`public-first-workflow-request-guard.mjs:511`,
in `spend` `:502`–`:518`). No `fsync` of the file or its directory is performed
anywhere in the guard; the only file write that offers crash atomicity is the `wx`
exclusive create itself (so documented at `spend` `:495`–`:501`). `writeEvent` (`:417`)
and `writeMarker` (`:441`) likewise write with `wx` and no `fsync`.

Consumers of the spend record — the budgeting decisions it drives:

1. In-process gate: `spend` returning non-null aborts before dispatch and denies with
   `alreadySpent` (`:601`–`:606`); the dispatch itself (`:606`) is unreachable without a
   persisted slot.
2. Post-run qualification: the driver reads the attempt directory after every owned
   child exits — `readAttemptDir(attemptDir, { final: true })` (`public-first-workflow.mjs:4566`)
   and `readSpentToken` (`:1299`) — then `assertGuardAttempt` (`:1356`) requires
   exactly one admitted request, zero denials, and the `spent` token; `spent_present`
   is derived from the file's existence (`readAttemptDir` `:1183`). This is the single
   authority for "how many upstream requests happened" (`:4560`–`:4565`).

Residual: if the machine loses power after the `wx` create returns but before the
metadata is durable, the `spent` token (and any event record) can be lost, so a later
run could take a fresh slot. Nothing in the code claims otherwise.

### Trigger definition

The acceptance fires on any change that claims or relies on power-loss-safe request
budgeting — concretely, any of:

1. a spec, doc, receipt field, or CLI message asserting that the one-request budget
   survives a machine power loss / unclean shutdown;
2. a consumer that treats an absent `spent` token as proof that no request was ever
   dispatched *across a power cycle* (today, absence is instead treated, at
   `readSpentToken` `:1303`–`:1310`, as "the intended request never happened" — valid
   only without power-loss assumptions);
3. a change to `spend` (`:502`), `writeEvent` (`:404`), or `writeMarker` (`:430`) that
   introduces `fsync`/`dirsync` and thereby starts advertising durability;
4. any expansion of the local single-attempt dev flow into an environment where the
   budget must hold across unexpected reboots.

Until such a claim or consumer exists, the guard's contract is crash *atomicity*
(exclusive create) and not power-loss *durability*, and the trigger has not fired.

### Fix-cost estimate

Files: `scripts/public-first-workflow-request-guard.mjs` (only `spend` `:502`–`:518`,
`writeEvent` `:404`–`:420`, `writeMarker` `:430`–`:450`; add `fsyncSync`/`openSync` to
the `node:fs` import at `:87`) plus `scripts/public-first-workflow-request-guard.test.mjs`.

Approach (fsync + dirsync): after each `wx` create, `openSync` the file, `fsyncSync`
its fd, close it, then `openSync` the parent directory and `fsyncSync` that fd so the
new directory entry is durable. ~15 lines including error paths; no schema change; must
preserve the fail-closed taint path (`persistEvent` `:489`–`:493`, `taint` `:461`) so a
failed fsync denies the dispatch rather than silently continuing. Cost is a syscall per
write and a directory fsync per attempt — negligible for a single-attempt run.

### Verdict

**keep-defer.**

Rationale: the acceptance condition ("future power-loss-safe request budgeting is
claimed") is nowhere claimed. The guard documents only the exclusive create's
cross-process atomicity (`spend` `:495`–`:501`), and the durable budget authority is the
on-disk attempt directory read *after* the run (`readAttemptDir` `:1174`,
`readSpentToken` `:1299`, `assertGuardAttempt` `:1356`). A power loss around a local,
operator-run, single-attempt development flow is an operator-local hazard, not a
guarantee the project makes; and losing the token does not silently multiply requests —
a re-run still regenerates and re-authorizes deterministically. Re-open, do not close.

This is the cheaper of the two to fix (local to one file, no perf concern), so it is
the natural first candidate if any future iteration touches the guard or starts making
a durability claim; it is not escalated now because no trigger has fired and low
severity does not justify pulling it forward on its own.

---

## Disposition summary

| Issue | Severity | Trigger fired? | Verdict | Next action |
| --- | --- | --- | --- | --- |
| I-000009 | low | no | keep-defer | occurrence note; re-open when qualification extends beyond the frozen installed runtime |
| I-000010 | low | no | keep-defer | occurrence note; re-open when power-loss-safe budgeting is claimed or the guard is next touched |

Neither issue is closed or waived: both describe real, narrow residuals whose trigger
conditions are precise and re-testable, so the obligations stay open and verifiable.

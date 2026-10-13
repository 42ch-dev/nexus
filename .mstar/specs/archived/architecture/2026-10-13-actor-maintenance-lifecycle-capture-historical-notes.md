# actor maintenance lifecycle capture — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/actor-maintenance-lifecycle-capture.md`](../../../knowledge/architecture-patterns/actor-maintenance-lifecycle-capture.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 30

```markdown
> **Current scope.** The durable mechanisms in §1–§3 and §5–§6 (per-resource revision CAS, lifecycle epoch, per-Character activity/transition fence, the operation phase machine, and the receipt-keyed dedup rows) are live. The **run-to-memory capture writer** described in §4 and §7 is **historical (v1.185)** and is not reachable from the current Host: Character `remember:true` is refused before any effect, so no drain/digest writer runs. That historical writer and its example symbols are retained only as the design record.
```

### at pre-freeze line 34

```markdown
The v1.185 developer loop added material mutation and reversible freeze to the shipped Actor vertical: identity edit, archive/restore, WorldSheet maintenance, bounded KE content maintenance, and an opted-in run-to-memory capture. Three well-known concurrency mechanisms are individually insufficient here: SQLite row CAS does not cover Host sessions or file effects, a provider session can outlive a DB transaction, and a server-owned stream ends on the Host's schedule, not the request's. The pattern combines **per-resource revision CAS**, an **internal lifecycle epoch**, and a **per-Character activity fence**; the run-to-memory capture (a v1.185 **historical** drain-authoritative capture with a receipt-keyed immutable dedup, see §4/§7) is currently disabled.
```

### at pre-freeze line 36

```markdown
Non-normative product semantics: [Actor Product Model §11](../../active/architecture/actor-product-model.md). The current Host completion limit for capture is stated at §6.1 there: the Host has a reservation but no complete run-capture writer, so Character `remember:true` is refused. The sibling composition doc covers the shipped v1.184 admission/bearer boundaries: [actor-bearer-boundary-composition.md](../../../knowledge/architecture-patterns/actor-bearer-boundary-composition.md).
```

### at pre-freeze line 65

```markdown
- **Activity admission** (writes, runs, and historical capture): an owned shared lease via `CoreService::acquire_actor_activity` (`actors.rs:297`). Owner-check first (no fence state leaks existence), then acquire the read lock and **re-read status and epoch under the fence** — the pair is exact for the lease's lifetime.
```

### at pre-freeze line 82

```markdown
### 4. Drain-authoritative capture (historical — v1.185)

> **Historical.** This section records the v1.185 run-capture design. The writer it described (`drain_and_finalize_character_operation`, `try_persist_capture`, `DrainAccumulator`, `build_capture_digest` in the former `crates/nexus-core/src/execution/lifecycle.rs`) does not exist at HEAD. Capture is currently refused: the Host has a reservation but no complete run-capture writer, so Character `remember:true` is refused before any effect (`not_supported` in core, `crates/nexus-core/src/host.rs:1066-1075`; HTTP `501 route_not_migrated`, `apps/nexus-service/src/provider.ts:456-462`); non-Character `remember:true` stays `422 invalid_input`.

The only capture authority was the **server-owned drain of the stream returned by `HostFacade::exec`** — not CLI SSE, not a broadcast subscriber, not model-written metadata or a caller-supplied digest. The admission snapshot (owner Creator, Character, binding, World, epoch, server-minted session and operation ids) was transferred into the drain along with the activity lease.

- The drain was filtered to the exact stored session + operation; only `MessageDelta` text accumulated into the candidate digest (never thought/tool/plan events) and only `OpFinished(end_turn)` counted as a successful terminal. `MaxTokens` / `MaxTurnRequests` / `Refusal` were `Incomplete` — no candidate.
- The lease epoch was re-checked at persist time against the snapshot epoch; a mismatch (or a gone binding) failed capture with `CaptureScopeChanged` (now `ActorContractConflict::RunCaptureScopeChanged`, `crates/nexus-local-db/src/error.rs`) **without** rewriting the successful run. Run success and capture failure were distinct outcomes.
- `remember:true` required a stored admitted Character session with an exact binding; a foreign actor/session still fails validation before Host execution. No caller-supplied actor, binding, digest, or operation id claimed capture authority.

```

### at pre-freeze line 109

```markdown
> **Reachability note.** These rows and their writer still exist for the memory owner, but no current Host path produces them: Character `remember:true` is refused before any effect (§4). Reintroducing capture requires a separately accepted complete integration of the memory owner, not a second pipeline (Actor Product Model §6.1).

### 7. Bounded digest accumulation (historical — v1.185)

> **Historical.** Part of the retired §4 capture writer.

The candidate digest is `Prompt:\n<raw prompt>\n\nResponse:\n<accumulated message text>`, capped at `MAX_CAPTURE_DIGEST_BYTES = 65_536`. Accumulation used **checked arithmetic** (`checked_add` on static overhead + prompt + buffered text + incoming delta); any overflow or over-cap flips `digest_too_large` before a delta is copied and yields `CaptureTooLarge` — never an oversized candidate. An empty (`trim()`-empty) response is `CaptureEmptyOutput`; it is never a candidate.
```

### at pre-freeze line 119

```markdown
Each mechanism covers what the others cannot. Row CAS protects the SQLite truth; the epoch protects Host session identity across a freeze; the fence makes archive/restore refuse instead of racing provider or file effects; the receipt makes capture idempotent and immutable. Omitting any of them produces a concrete failure class: a `500` where a `409` is expected (unmapped constraint), a stale pre-archive session resuming after restore (unfenced epoch key), an archive that "succeeds" mid-activity (lease dropped early), a duplicate candidate after review (no receipt), or an oversized/overflowed digest (unchecked accumulation — historical).
```

### at pre-freeze line 132

```markdown
- Historical (v1.185, retired): drain/digest/persist writer — the former `crates/nexus-core/src/execution/lifecycle.rs` (`drain_and_finalize_character_operation`, `try_persist_capture`, `DrainAccumulator`, `build_capture_digest`); stale-session rejection for the prompt path — the former `prepare_prompt` in `crates/nexus-core/src/host.rs`. The current stale-session refusal is `actor_session_stale` (`crates/nexus-core/src/host.rs:1082-1125`).
```

### at pre-freeze line 134

```markdown
- Published slice — the v1.185 iteration package is a local process artifact (not tracked at HEAD); the durable authority is [§11](../../active/architecture/actor-product-model.md) plus the sources above.
```

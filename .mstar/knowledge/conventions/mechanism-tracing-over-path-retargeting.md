---
module: knowledge-corpus
date: 2026-10-12
problem_type: convention
category: conventions
severity: medium
tags:
  - documentation-hygiene
  - retargeting
  - citations
  - sweeps
  - historical
  - rca
---

# Mechanism Tracing over Path Retargeting

When a document cites an implementation that has moved or been deleted, rewriting the path to the new owner while keeping the old body manufactures **false current facts**. Trace the mechanism at HEAD first; only then rewrite the reference. Distilled from the 2026-10-12 knowledge/specs hygiene round, where three QC rounds blocked mechanically retargeted references.

## Context

The round retargeted documents whose cited implementation was the Rust `nexus-daemon-runtime` host (retired in v1.193; migrated to the TypeScript service `apps/nexus-service`, or deleted outright). The fast sweep was "old path -> new owner prefix": `crates/nexus-daemon-runtime/...` -> `apps/nexus-service/...`. Every retargeted path resolved, which made the result look verified — but the sentences hanging off those paths still described the retired implementation. A resolved path reads as a current-fact claim, so a reader (human or agent) follows instructions the new owner cannot satisfy and derives a wrong edit or a wrong diagnosis.

## Guidance

Trace the current mechanism first, then write.

1. **Confirm the mechanism exists at HEAD before rewriting the reference.** `rg` for the symbols, file paths, and line anchors the doc asserts. Two distinct failures must both be caught: a reference that resolves **nowhere**, and a symbol that resolves somewhere but **belongs to another mechanism** — `NEXUS_DAEMON_REMOTE_BIND` still exists, but it gates the peer-tools connect bind, not the HTTP service (Example 4). Existence is necessary, not sufficient.
2. **Mechanism alive -> rewrite instructions and examples against the new owner's real API and behavior.** Keep the lesson; change the mechanism it is told through (Example 1: the retained router's capture rule; Example 2: the retained listener stack).
3. **Mechanism dead -> keep the reusable lesson but label it explicitly historical and name the retired iteration, and delete symbols and line numbers that can no longer resolve.** A function name or `path:line` that no longer exists is a false citation, not history (Example 3). State current behavior separately and in its own terms.
4. **Re-sync the index row whenever the doc body changes.** The README row's title and summary are part of the claim surface; a row describing the pre-rewrite mechanism is a second false current fact. (Two QC rounds bounced on row/body drift during this round.)
5. **Disambiguate shared response codes when writing RCA instructions.** The same code can be produced on more than one path — `501 route_not_migrated` is emitted both when no route pattern matches (`apps/nexus-service/src/server.ts:371-375`) and when a matched handler deliberately refuses an unsupported operation (`executeProviderOperation`, `apps/nexus-service/src/provider.ts:452,461-462`), both rendering through `routeNotMigrated` (`apps/nexus-service/src/errors.ts:67-73`) and its status mapping (`apps/nexus-service/src/errors.ts:58-64`). An RCA step must establish which path applies before prescribing a registration fix: a matched handler's deliberate refusal means route registration is not the defect.

## Why This Matters

- A dead path is an obvious error; a **live path with a dead mechanism** is camouflaged. Reviewers and agents skip the "does this still exist?" check because the path resolves, then build on a claim that was never true.
- The failure is systematic under sweep pressure: one prefix rewrite is cheap, but the body it lands on was never re-verified, so the error multiplies across a corpus at the speed of search-and-replace.
- The cost is measured in review rounds: this round's retargets were blocked three times by QC, and each unresolvable symbol or wrong mechanism becomes a dead end or a confidently wrong implementation for the next reader.

## When to Apply

Any reconciliation of documents or comments after an implementation moved or was deleted — especially batch sweeps across many files, where a mechanical prefix swap is tempting. Also apply to the index rows and summaries that accompany such docs, and to any RCA guidance that cites a response code shared by multiple emission paths.

## Examples

Five retargets blocked by this round's QC, each with the current mechanism at HEAD:

1. **Colon-capture teaching retargeted to the TypeScript service.** The body still taught the retired Axum/matchit `:param` template convention; the retained router matches anchored RegExp patterns whose capture groups are positional params (`DomainRoute.pattern`, `apps/nexus-service/src/routes.ts:96-113`; `matchRoute`, `apps/nexus-service/src/routes.ts:143-163`). A literal `:id` in a pattern matches those characters, so the request matches no retained route and the service answers `501 route_not_migrated` (`apps/nexus-service/src/server.ts:371-375`).
2. **TLS notes kept the Rust stack.** After the retarget, the notes still presented `axum-server`/`rustls` and auto-generated certificates under `~/.nexus42/tls/` as current startup instructions. The service selects `createHttpsServer` from `node:https` when `config.tlsCert` and `config.tlsKey` are both set (`apps/nexus-service/src/server.ts:9-10,461-467`) and consumes operator-supplied PEM; no workspace manifest lists `axum-server` at HEAD.
3. **Symbols introduced by the migration that never existed in the new owner.** `DrainAccumulator`, `try_persist_capture`, `parse_delete_expected_revision` — absent from the implementation at HEAD (zero `rg` hits in code, manifests, and tests; only explicit historical markers keep the retired names). Current truth instead: Character run capture is refused before any effect (`not_supported`, `crates/nexus-core/src/host.rs:1066-1075`; HTTP `501` at `apps/nexus-service/src/provider.ts:456-463`), the knowledge-delete handler parses `expected_revision` itself (`apps/nexus-service/src/actors.ts:240-253`), and the live sibling mechanism is the actor fence (`try_acquire_activity` / `try_acquire_transition`, `crates/nexus-core/src/actor_fence.rs:223,234`).
4. **A retired boot-gate env var presented as the HTTP service's remote-access contract.** `NEXUS42_DAEMON_API_KEY` + `NEXUS_DAEMON_REMOTE_BIND=1` was the Rust daemon's gate; the service fail-closes a non-loopback bind on `--allow-remote` + TLS PEM + `NEXUS42_DAEMON_API_KEY` (`apps/nexus-service/src/bind.ts:36-48`; flag and environment read at `apps/nexus-service/src/config.ts:373,439`). The env var itself still exists — as the peer-tools connect lane's gate (`crates/nexus-core/src/connect/accept.rs:204-228`) — which is exactly why a resolving symbol does not make a citation true.
5. **A same-named symbol cited at the wrong layer.** The service method was cited as the DAO. `CoreService::update_finding` (`crates/nexus-core/src/findings.rs:478`) delegates to the row-level DAO `update_finding` in `nexus-local-db` (`crates/nexus-local-db/src/findings.rs:927`). Same name, two layers — cite the layer whose contract you mean.

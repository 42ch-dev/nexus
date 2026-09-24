---
module: scripts/public-first-workflow (public proof driver + request guard)
date: 2026-09-24
problem_type: workflow_issue
category: workflow-patterns
severity: high
plan_id: 2026-09-22-v1.195-p3-public-first-workflow
applies_when:
  - "An authorized, externally-billed or rate-limited action must run at most once per attempt (one live model request, one submission, one publish)"
  - "A deterministic proof must be separated from a live qualification so neither is reported as the other"
  - "A proof driver must isolate its child processes without reading, copying or logging credentials"
  - "Reviewing a harness that claims a hard ceiling on outbound calls"
related_components:
  - scripts
  - apps/nexus42
  - apps/nexus-service
  - nexus-agent-host
tags:
  - request-budget
  - exclusive-create
  - live-qualification
  - credential-hygiene
  - deterministic-fault-controls
  - honest-blocker
status: active
---

# One authorized live action per attempt — a preload guard with an atomic spent slot

## Context

Some verification is inherently one-shot: one real upstream model request, one publish, one submission under a "no retry" grant. The grant is only meaningful if the ceiling is enforced by something other than good intentions, and it has to hold against the ways a "single" call becomes several: a title-generation side call, an automatic retry, a redirect, a second child process, a second launch of the same script, a crash mid-request and a re-run.

The verified shape is a Node **preload** injected into every owned child of the run (`NODE_OPTIONS=--import=<absolute guard path>`), wrapping the runtime's own global transport before the runtime imports it. It is a guard, not an imitation runtime and not a proxy.

## Guidance

### 1. Spend the slot with an exclusive create, before dispatch

The slot is a file taken with an exclusive create (`wx`) **before** the original transport is called, so it is atomic across children and processes and it is consumed even when DNS, TLS or the HTTP exchange then fails. "After dispatch" accounting cannot bound a crash or a redirect chain; a token that can be reset after failure is not a ceiling. The official action must be admitted only through that slot. A second launch cannot reset, delete or recreate the evidence directory or the token.

### 2. Allowlist one exact target, deny everything else before the network

The guard accepts exactly one preselected HTTPS origin plus the model path and the POST method (in deterministic mode, exactly the configured loopback test origin). Everything else — another host, another path (`/models`, telemetry), another method, a redirect target, a request from an unexpected runtime — is denied **before** any network call, and the denial is recorded. Redirect policy is forced to `error` so a redirect answer cannot become a second request. There is no wildcard or suffix host matching.

### 3. Forward opaque, inspect nothing

The original transport receives the caller's own request object with only `redirect` overridden: headers, body and signal are the same references, uninspected and unmodified. There is no credential proxy, no copied key, no alternate secret file and no credential introspection. The guard's own evidence records are credential-free (no URL, no header, no body, no environment dump), and one regression asserts exactly that.

### 4. Fail closed on the guard's own failure

If the ceiling cannot be locked, if a denial or admission cannot be persisted, if the guard's environment is unusable, or if the transport is not the supported global fetch, the child is denied (or terminated) — an unrecordable denial is **terminal for the attempt**, never a silent zero-denial pass. An attempt that cannot prove its bookkeeping fails instead of running unguarded.

### 5. Prove the ceiling deterministically before spending it

Deterministic fault controls are material preconditions, not extras: two concurrent calls in one process, two child processes racing on one attempt, a second launch, a 302, a 429, a connection reset, an unexpected endpoint and a non-POST method on the allowed endpoint must all leave the served count ≤ 1 per fresh fixture attempt (and the slot spent where a request was actually dispatched). The supported runtime's actual behaviour is confirmed by the handshake record, not by reading its source once.

### 6. Keep deterministic proof and live qualification distinct

A deterministic pass is not live proof, and live success is not required for the deterministic claims. The honest statement of a live step is one recorded admission with its outcome category and cleanup status; a transport or auth failure **after** admission consumes the authorization, and only a new explicit grant may retry — so it must never be reported as "no credentials".

### 7. Report a missing credential as a blocker, without looking at one

Credential availability is observed as "the inherited environment names a credential", never by reading, copying, printing or persisting a value. If no eligible inherited credential is available, the run stops with zero admissions and a named blocker; nobody discovers keys in a real home or copies one into the isolated proof. The blocker is distinct from a runtime, policy, replay or recovery defect.

### 8. Isolate the children, and strip credential-shaped variables case-insensitively

Each child gets its own `HOME`, `DSH_HOME`, workspace and evidence directories, and the driver **removes** credential-shaped inherited variables from the child environment (a named list plus a case-insensitive pattern over `API_KEY|ACCESS_KEY_ID|TOKEN|SECRET|PASSWORD|CREDENTIAL(S)`). Name matching must be case-insensitive: a mixed-case variant of a listed name survived the first, case-sensitive filter and reached a child until the review caught it. Stripping by name never reads a value.

### 9. Runtime drift is a STOP, not a fallback

The guard bounds the *known* transport of the *supported* runtime version. If the installed runtime stops using the guarded primitive (or the preload handshake disappears), the live step stops and reports the mismatch — the guard's coverage claim is not extended to an unexamined transport, and no unguarded call is used as a fallback.

## Why This Matters

A one-shot grant has no second chance to be correct: an admitted failure that is misclassified as "missing credentials" invites an unplanned second attempt, a side call (title, probe, telemetry) silently consumes the budget, and a retry after a failure turns one authorized action into unbounded spend. The exclusive-create slot is the only accounting that survives a crash, and separating deterministic fault proof from live qualification is what keeps "the guard works" from being reported as "the live path works".

## When to Apply

- Any task that runs a billed, rate-limited or externally visible action under an explicit one-shot grant.
- Any harness that must bound outbound calls from a program it does not own (a vendor CLI, an agent runtime, a model SDK).
- Reviewing a guard whose own bookkeeping could fail open (unrecordable denial, missing lock, unusable environment).
- Writing a proof driver that must not touch the operator's real homes or credentials.

## Examples

- Preload contract in the file header of `scripts/public-first-workflow-request-guard.mjs`: one request per attempt, `wx` slot before the original fetch, opaque forwarding with `redirect: 'error'`, denial categories, credential-free evidence.
- Live invocation shape: `node scripts/public-first-workflow.mjs --mode live --deterministic-receipt <receipt> --attempt-dir <fresh-owned-dir>` — only after the deterministic mode has passed on the same artifacts.

## Evidence

- Guard implementation and its contract header — `scripts/public-first-workflow-request-guard.mjs` (spent slot via `wx`, allowlisted URL validation with named problems, `DENY` categories including `unsupported_transport` and `package_child_runtime`, foreign-runtime refusal, credential-free records).
- Guard regressions — `scripts/public-first-workflow-request-guard.test.mjs`: exactly one dsh POST admitted and forwarded opaque; a second request denied and never dispatched; two concurrent in-process calls and two racing child processes admit exactly one; a second launch cannot reset a spent attempt; a fresh attempt directory is a fresh budget; 302 not followed; 429 reported as itself and never retried with the slot still spent; connection reset spends the authorization; unexpected endpoint denied before any network and spends nothing; non-POST denied; foreign runtime denied; a symlinked launch of the recorded entry is still the guarded runtime; a missing global fetch is `unsupported_transport`, not a bypass; an unrecordable denial is terminal; evidence carries no secret and only contract fields.
- Driver isolation and credential-name stripping — `scripts/public-first-workflow.mjs` (per-child isolated `HOME`/`DSH_HOME`/workspace/evidence directories, `CREDENTIAL_ENV_KEYS` + case-insensitive `CREDENTIAL_ENV_PATTERN`, `credentials_unavailable` blocker with zero admissions, no credential inspection).

## Coverage gaps

- The guard bounds the supported runtime's known global-fetch transport. It is **not** an OS-wide network sandbox: hostile arbitrary code, model tools and custom runtime layers are excluded by the sealed recipe, not by the guard.
- Durability of the spent slot across power loss is not proven (recorded low residual); process-level exclusivity and post-failure persistence are.
- The live step was qualified once, on a specific installed runtime version with a specific guard/driver digest pair; a changed runtime or guard digest invalidates that evidence and requires a new explicit grant rather than reuse.

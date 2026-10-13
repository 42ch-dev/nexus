# Registry Integration — Original Unshipped Design Historical Notes

**Provenance**: frozen from [`specs/active/agents/registry-integration.md`](../../active/agents/registry-integration.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — the removed text names v1.193 P2 (2026-09-21, commit `e3df40f1`) as the daemon-deletion supersession; the remaining blocks are the file's own "original design (not shipped)" records, frozen as the design exploration they were. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## §0 owner-paragraph tail (removed sentence)

"The old daemon-owned supervision/restart/backoff model is **historical**: `nexus-daemon-runtime` and the integrated daemon were deleted in v1.193 P2."

## §0.1 original-design bullet (removed; reworded as a present-tense "not implemented" note)

- **Original design (not shipped as a source-selection policy):** workspace overrides / enterprise mirrors, pinned index versions or ETags, and explicit upstream `FORMAT.md` / schema-revision compatibility remain design considerations, not current fetch guarantees.

## §2.1 historical design scope paragraph (removed)

**Historical design scope:** generalized ACP-version/capability/platform filtering, creator-registration probing, and daemon-owned process supervision/restart/backoff were original integration goals. They must not be inferred from registry membership or treated as current daemon responsibilities.

## §3.1 original source-priority design (removed)

**Original source-priority design (not shipped):**

1. Remote canonical Registry (HTTPS)
2. Workspace override
3. User cache
4. Local static mirror

## §3.2 original fetch design (removed)

**Original fetch design (not shipped guarantees; daemon wording historical):**

- Use ETag / If-Modified-Since when available.
- Failures must not crash daemon; enter degraded mode.
- Verify manifest signatures/checksums when Registry provides them.

## §3.3 original offline design (removed)

**Original offline design (superseded, not current behavior):**

- If remote fetch fails, runtime uses last good cached manifest if within TTL.
- If no cache exists, runtime requires local override or manual agent configuration.

---

## §4 historical design boundary + §§4.1–4.4 (removed)

**Historical design boundary (§§4.1–4.4):** the artifact matrix, invalidation triggers, proposed `registry refresh` CLI spelling and `cache/registry/` location below are the original design, **not the shipped cache contract**.

### 4.1 What is cached

- Registry manifest files
- Agent package metadata
- Small artifacts such as icons/descriptions

### 4.2 TTL and freshness

| Artifact | Default TTL | Notes |
| --- | --- | --- |
| Manifest index | 24h | refresh in background |
| Agent version list | 24h | pin overrides TTL |
| Downloaded agent package | explicit | only if Registry supports binary distribution |

Runtime may serve stale cache immediately while async refresh proceeds.

### 4.3 Cache invalidation triggers

- User runs `nexus42 acp registry refresh`
- TTL expiry + next online event
- Workspace config changes
- Doctor detects checksum mismatch vs pinned agent

### 4.4 Storage location

- Under `$HOME/.nexus42/cache/registry/` (or a workspace-keyed subtree still under `$HOME/.nexus42/cache/`)


## §5 historical design boundary + §§5.1–5.4 (removed)

**Historical design boundary (§§5.1–5.4):** the compatibility filters, scoring order, explanatory inspect output and fallback policy below are original design targets, **not shipped selection guarantees**. Explicit provider configuration and Host readiness admission are separate from this registry lookup.

### 5.1 Hard filters

- ACP protocol version outside supported window
- Transport unsupported for current OS / runtime mode
- Missing required Nexus capabilities for selected profile
- Platform policy flags

### 5.2 Soft scoring

Prefer, in order:

1. User pinned agent
2. Workspace default
3. Highest compatible version within same major line
4. Local stdio agents over remote agents
5. Recent successful agent

### 5.3 Explicit user binding

- `nexus42 acp agent use` writes pin record under `$HOME/.nexus42/` or workspace-linked config (not ad-hoc under `<workspace>/` without user intent)
- `nexus42 acp registry inspect` shows why an agent passed/failed filters

### 5.4 Fallback path

If Registry resolution fails:

- Allow manual agent command in config
- Doctor prints actionable fix steps

---

## §6 tail sentence (removed)

"Neither composition uses the retired daemon supervisor or establishes a daemon restart/backoff contract."

## §6.1 historical ownership bullet (removed)

- **Historical ownership claim (superseded v1.193 P2):** “Process supervision and restart/backoff are owned by daemon.” Current owners are listed above; no such daemon exists.

## §7 historical CLI recommendation (removed)

Minimum recommended:

**Historical recommendation, not an executable command inventory:** in the current CLI `RegistryCommand` has `list` and `inspect`, not `refresh`; `nexus42 acp probe --registry` invokes `RegistryClient::refresh()`. The list below retains the original recommendation.

- `nexus42 acp registry list`
- `nexus42 acp registry inspect <agent>`
- `nexus42 acp registry refresh`
- `nexus42 acp agent use <agent>`
- `nexus42 acp probe`

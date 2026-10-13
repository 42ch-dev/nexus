# docs/** — de-historicization — Historical Notes

**Provenance**: frozen from `docs/**` on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed units are version/iteration provenance annotations and past-state narration with no single uncontested supersession date, so the freeze date is used).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live `docs/**` files by the v1.216 pass is reproduced below, verbatim, grouped by source file in original order. Still-true statements were kept in the live docs (restated present-tense where the history was woven into the sentence); their superseded wording appears here. Nothing here is current authority.

### `docs/ARCHITECTURE.md`

```markdown
The headless **`nexus-runtime`** binary (Connect-only, `--no-default-features --features connect-host`) and the Electron-owned TS service are the two long-running hosts; neither is launched or statused by the CLI. The integrated `nexus42 daemon` group was retired in v1.193 P2.
```
```markdown
3. **`nexus-knowledge`** owns World-scoped KnowledgeEntries / SourceAnchors (merged from former `nexus-kb` in V1.139) and User-scoped knowledge. Do not conflate either with the CLI **work file index** under work-scope `kb` routes.
```
```markdown
- **Local service host:** the Electron-owned TS service composes the native/TS authority and serves browser/desktop HTTP. The retired Rust host composition (`nexus-daemon-runtime`, v1.193 P2) is gone; `nexus-runtime` is the Connect-only headless cohort.
```

### `docs/CONTRIBUTING.md`

```markdown
A feature track is opened, measured and reclaimed inside the slice that owns it: a feature's cache and worktree are never parked until the end of an iteration. Reclamation is immediate and scoped, and it is never a reason to serialize independent work — in v1.190 six concurrent targets consumed 98 GiB under `/tmp` in a single iteration and degraded the host, so each slice reclaims its own footprint as soon as it is done with it.
```

### `docs/README.md`

```markdown
| [mcp-server.md](../../../../docs/mcp-server.md) | Operators | **Retired in v1.193 P2** — historical record of the removed `nexus42 mcp serve` tools-only MCP bridge (prerequisites, Claude Code / Codex config JSON, documented-only native face PL-10, the deleted ACP stdio factory) plus the retained core peer-admission contract. |
```

### `docs/module-authoring.md`

```markdown
| `nexus_abi_version` | integer | Compute envelope ABI version (`1` for V1.x). Unrecognized versions are rejected. |
```
```markdown
| `schemas` | object | — | V1.62+: inline JSON-Schema fragments — `key_block_attributes`, `key_block_state`, `invocation`, `battle_report` — validated by the host before/after invocation. Omitting the block disables validation (V1.61 modules keep working unchanged). Spec §7.3. |
```

### `docs/release.md`

```markdown
(`v1.19x`, `v1.20x`) are development milestones that never appear in a version
```

### `docs/nexus-runtime.md`

```markdown
carries no SPA bytes at all: the app-only `web-embed` selector (and the Rust
daemon host it embedded) was deleted in v1.193 P2, and the ordinary `cli`
```
```markdown
The retired Rust daemon host is not linked into this binary at all (v1.193 P2
deleted the crate): its HTTP router, embedded SPA, Setup/Canvas/Control Room,
```
```markdown
the manifest's `tools[]` member. They are **not** the V1.172 capabilities
```
```markdown
reads with no authoring surface, and the V1.172 capability registry is
```
```markdown
`DbPool` busy timeout); the per-work `runtime_lock` belonged to the retired
Rust host (v1.193 P2) and the Connect invoke path never acquires it. The
```
```markdown
Two pieces (V1.155 P1): the operator policy file and the issuance command.
```
```markdown
An absent file yields the defaults (empty / false / disabled — pre-V1.155
behavior). A malformed file, unknown fields, or
```

### `docs/strategy-authoring.md`

```markdown
>    Electron/TS service — the CLI has had no `daemon`/schedule group since
>    v1.193 P2); preset lanes then execute on the retained orchestration
```
```markdown
The `input` field maps to `preset.input.*` at schedule start. The retired CLI
schedule leaf (`nexus42 daemon schedule add --preset <id> --creator <id>
--seed <text>`, deleted with the whole `daemon` group in v1.193 P2) only
stored seed text as `core_context` v0 — it did **not** populate
`preset.input.*`, and it has no replacement CLI entry: drive schedules through
the service API above.
```
```markdown
`nexus42 preset validate <path>` (the pre-v1.193 spelling
`nexus42 system preset validate --offline` is deleted, and with the local leaf
becoming the only implementation the `--offline` switch is gone). It mirrors
```
```markdown
retained CLI leaves (`nexus42 preset patch state|transition|prompt`, V1.175 P1
— see [cli-spec §6.2G.4](../../active/cli/cli-spec.md)):
```
```markdown
schedule supervision (the Rust host composition was deleted in v1.193 P2).
```

---

## D2 decisions (no archive record — bare supersession markers)

The remaining removed tokens in `docs/**` were **bare version/iteration provenance annotations wrapping a still-true statement** (D3 exception): only the annotation was dropped, the statement kept present-tense. No independent history content was discarded, so no separate record is required. Examples: `(V1.155 P1)` → "Two pieces:"; `(V1.180 P1, RN-OGA-2)` → statement kept.

## Companion record

The mixed `docs/mcp-server.md` (retired Model A bridge record + retained peer-admission contract) was relocated whole via `git mv` to [`2026-10-13-mcp-server-historical-notes.md`](2026-10-13-mcp-server-historical-notes.md); its live contract is restated present-tense in the retained `docs/mcp-server.md`.

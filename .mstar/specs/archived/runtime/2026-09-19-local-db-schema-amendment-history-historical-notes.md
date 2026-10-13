# Nexus Local DB Schema — Amendment History — Historical Notes

**Provenance**: frozen from [`specs/active/runtime/local-db-schema.md`](../../active/runtime/local-db-schema.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-09-19 — the live spec's own `Last updated` line marked that date as the v1.191 P1 schema 23→24 raise, the last amendment recorded in the removed header. The frozen lines span the V1.26 → V1.40/V1.42/V1.52/V1.55/V1.139/V1.191 amendment ledger; every normative rule they annotated is retained present-tense in the live spec.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

### at pre-freeze lines 5–9 (header amendment block)

```markdown
**V1.40 Shipped amendments:** §4.1.2 `kb_key_blocks` validation intent — application-layer validation in `nexus-kb::validation` module (ValidationMode::Novel enforces `body.attributes.novel_category`); `narrative_worlds` rows via `creator world create` (V1.40 P0); `kb_extract_jobs` artifact locator columns (`source_kind`, `source_locator`, `profile_hint`, `work_id`) added V1.40 P3.

**V1.42 Draft amendment:** `work_chapters` PK migration — composite primary key `(work_id, volume, chapter)` with `volume INTEGER NOT NULL DEFAULT 1`; backfill existing rows `volume = 1`; drop legacy `(work_id, chapter)` PK. Normative detail: [novel-writing/workflow-profile.md §4.5.4](../../active/novel-writing/workflow-profile.md).

**V1.139 architect §5.2 amendment:** §4.1.2 `kb_key_blocks` — `extensions_nexus_json TEXT` column (additive migration) for spoke `extensions.nexus` round-trip preservation. Existing columns (`world_id`, `created_from_command_id`, `source_work_id`, `source_chapter`, `source_provenance_kind`) retained for query efficiency. Full contract: [`spoke-adapter-architecture.md`](../../active/architecture/spoke-adapter-architecture.md) §2.3.

**Last updated**: 2026-09-19 — v1.191 P1: holder registry, native governance columns and knowledge revisions (schema 23 → 24).

**v1.191 P1 amendment (shipped):** [holder-governance.md §§2–6](../../active/architecture/holder-governance.md#2-registry-identity-stability-and-lifecycle) owns the workspace holder registry (`knowledge_holders`), the native KE governance columns (`holder_entry_id` / `disclosure` on `kb_key_blocks`), the World/Character `knowledge_revision` columns, the isolated import quarantine (`knowledge_import_quarantine`), the guarded offline migration and rollback. The ordered migration `20260918000001_holder_registry.sql` raises the schema version 23 → 24; the legacy `creator_only` column and the `extensions.nexus.creator_only` key are removed from every runtime path and refused on input. Actor narrative ownership remains the existing three-container model; no fake World lore ...
```

(The replacement live header is `**Last updated**: 2026-09-19 — schema version 24 (workspace holder registry, native governance columns, and knowledge revisions).`.)

### Inline history units removed (verbatim)

- at pre-freeze line ~106 — `**DF-43 ownership boundary (V1.55 P0)**:` → live `**DF-43 ownership boundary**:`
- at pre-freeze line ~108 — `#### 4.1.2 Narrative + World KB persistence (V1.26 draft)`
- at pre-freeze line ~110 — `... support the V1.26 persistent \`NarrativeGateway\` and \`KbStore\` adapters.`
- at pre-freeze line ~112:

```markdown
**V1.40 P1 migration intent** (): add validation for narrative `block_type` vocabulary (`foundation`, `background`, `character`, `location`, `society`, `rules`, `economy`) on `kb_key_blocks.block_type` — via CHECK constraint, application-layer enum, or companion lookup table. Minimum `body_json` shape per category is enforced in `nexus-kb`, not in raw SQL. **V1.40 P1 shipped**: application-layer validation in `nexus-kb::validation` module; `ValidationMode::Novel` enforces `body.attributes.novel_category` on insert/update. **V1.40 P0** may add `narrative_worlds` rows via `creator world create` without altering `kb_key_blocks` DDL beyond existing indexes.
```

- at pre-freeze line ~184 — `-- V1.52 provenance columns (V1.139 V1.52 amendment: retained for query efficiency)`
- at pre-freeze line ~188 — `-- V1.139: spoke extensions.nexus round-trip preservation (see spoke-adapter-architecture.md §2.3)`
- at pre-freeze line ~218 — `#### 4.1.3 Knowledge entries persistence (V1.27)`
- at pre-freeze line ~259 — `**V1.139 architect §5.2 amendment — \`extensions_nexus_json\` column:**`
- at pre-freeze line ~261 — `The \`extensions_nexus_json TEXT\` column is additive (V1.139 migration adds it via \`ALTER TABLE\`). It stores ...`
- at pre-freeze line ~263 — `#### 4.1.4 \`works\` lifecycle lock columns (V1.41 Draft — DF-60)`
- at pre-freeze line ~269 — `| \`novel_completion_status\` | \`TEXT\` | no | \`completed\` when §6 criteria met (V1.38 residual) |`
- at pre-freeze line ~275 — `#### 4.1.5 Novel work pool tables (V1.41 Draft — DF-61)`

### at pre-freeze line ~311 (retired `outbox` table row — removed as history)

```markdown
| `outbox`（已删除） | 同步命令队列（V1.163 由 `20260812_drop_legacy_outbox.sql` 删除；schema 15→16） | — |
```

### at pre-freeze line ~318 (outbox zero-consumer audit provenance — removed as history)

```markdown
> **Provenance**：零活跃消费者审计完成于 **V1.59 P1 T3**（commit `c39e4afc`，2026-06-22）。migration `20260812_drop_legacy_outbox.sql` 头注的 "V1.159 T3" 为已知不可变值（migration 文件自应用起不可变：sqlx checksum 锁定，禁止原地修改），以 **V1.59 T3** 为 verified 值。
```

### at pre-freeze line ~316 / ~320 / ~322 (`workspace_sessions`)

```markdown
| `workspace_sessions` | Workspace session persistence for `workspace.open`/`workspace.commit` with file-level OCC content hashes (V1.56 P0) | Daemon |
...
#### 4.2.1 `workspace_sessions` (V1.56 P0)
...
Persists `workspace.open`/`workspace.commit` sessions in `SQLite`. Replaces the V1.55 in-memory `WorkspaceSessionManager`.
```

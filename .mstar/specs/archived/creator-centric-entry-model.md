# Creator-Centric Entry Model — Normative Supplement v1

**Status**: Shipped (V1.35)  
**Document class**: Master (historical V1.35 lock; retained entry-model supplement)  
**Created**: 2026-06-06  
**Shipped**: 2026-06-07 (V1.35 P5 spec-tracker-hygiene)  
**Merged into**: [cli-spec.md](../cli/cli-spec.md) §7 in V1.35 P5; retained as shipped entry-model supplement
**Scope**: Product-level rules for **when** users enter via `creator` vs `platform` vs `system`  
**Supersession (v1.193 P2)**: Current entry semantics are defined by [cli-spec.md](../cli/cli-spec.md) §6.0B and its delivered v1.193 P2 overlay, not this V1.35 onboarding record. `daemon start`, `creator bootstrap`, `creator run`, and all daemon scheduling controls were retired; the steps below that use them are historical, not current entry rules. The direct-core `cli` cohort is now the ordinary default (`apps/nexus42/src/cli.rs`, `apps/nexus42/src/commands/creator/mod.rs`, `apps/nexus42/Cargo.toml`).
**Coordinates with**:

- [cli-command-ia.md](cli-command-ia.md) — top-level IA
- [cli-spec.md](../cli/cli-spec.md) — command detail
- [work-experience-model.md](../creator/work-experience-model.md) — Work journey
- [entity-scope-model.md](../architecture/entity-scope-model.md) — KB / knowledge scopes

---

## 1. Purpose

Nexus OSS serves two overlapping personas:

1. **Creator operator** — acts as or on behalf of an agent identity (`creator_id`); creative work, local assets, orchestration.
2. **Platform user** — authenticated human with User session; cloud sync, explore, publish, pairing.

V1.35 locks **creator as the creative hub**. Platform capabilities are **optional mounts**, not prerequisites for local-first work.

---

## 2. Entry rules (historical V1.35 normative lock)

| User intent | Primary entry | Notes |
| --- | --- | --- |
| Start or continue creative **Work** | `nexus42 creator run ...` (retired v1.193 P2) | Historical default product path (V1.33+ FL-E), not a current CLI entry |
| Register / switch Creator identity | `nexus42 creator register\|use\|list` | Pure local register allowed pre-release |
| Workspace + SOUL + memory + local assets | `nexus42 creator workspace\|soul\|memory\|kb\|knowledge\|reference` | Bound to active `creator_id` |
| Connect external AI agent | `nexus42 acp agent use` | Historical prerequisite: `daemon start` (retired v1.193 P2; no longer required) |
| Run presets / schedules (power user) | `nexus42 daemon schedule ...` (retired v1.193 P2) | Historical advanced path; not a current CLI surface |
| User login, cloud sync, explore, publish | `nexus42 platform ...` | Requires User session when platform integration enabled |
| Doctor, config, preset validate | `nexus42 system ...` | Not creator-scoped maintenance |

---

## 3. Pure local vs platform-mounted

### 3.1 Pure local path (historical V1.35 default while `platform_integration = paused`)

Historical minimum chain to first Work (≤7 steps — V1.35 cli-spec §7.1). Steps 5 and 7 were retired in v1.193 P2; this chain is not a current first-Work recipe, and no replacement CLI runner is implied:

1. `system doctor`
2. `creator register` (or reuse existing)
3. `creator use <ref>`
4. `creator workspace init`
5. `daemon start` — **historical; retired v1.193 P2**
6. `acp agent use <agent>`
7. `creator bootstrap --idea "..."` — **historical; retired v1.193 P2**

No `platform auth login` or sync pull was required; the current canonical sync spelling is `platform sync pull`, not the retired top-level `sync` alias.

### 3.2 Platform-mounted path

When platform integration is enabled:

- Add `platform auth login` before or after Creator registration (User-first vs Creator-first — cli-spec §7.2).
- Structured sync via **`platform sync pull|push`** (V1.35 IA target).
- Pairing via `creator pair` when User owns Creators created on web.

Creator commands **must not** silently require User token when local-only policy is active.

---

## 4. What stays outside `creator`

| Capability | Target group | Rationale |
| --- | --- | --- |
| `sync pull\|push` | `platform sync` | User-scoped cloud boundary (PD-05: not short-term focus but IA clarity) |
| `doctor`, `config`, `preset validate` | `system` | Machine maintenance, not agent identity |
| ACP protocol negotiation | `acp` | Separate capability plane per ADR-025 spirit |
| Daemon lifecycle / schedule control (historical; retired v1.193 P2) | `daemon` (deleted group) | Historical runtime supervisor, not identity; no current CLI control surface |

---

## 5. Invariants

1. **Single active creator** per CLI session (`creator use`); all `creator *` subcommands resolve against it.
2. **Historical V1.35 creative entry** for new users was **`creator run`**, not `daemon schedule`; both CLI entries were removed in v1.193 P2, so this is no longer a help-text or onboarding invariant.
3. **`creator kb`** is never generic “all knowledge”; use qualified terms per entity-scope-model §5.4.
4. Platform group remains in IA even when paused — help must say “requires login; skip for local-only”.

---

## 6. Acceptance (spec-level)

1. cli-spec §7 documents local and platform paths without contradiction.
2. V1.35 compass Appendix A maps dual-entry and KB naming issues to P2/P3 plans; cli-command-ia §3.2 holds rename options.
3. Compass P3 locks rename/help strategy for `kb` vs `knowledge`.

---

*Shipped supplement to cli-command-ia.md for V1.35. Implementation tracked by plans P2–P4 and P5 hygiene.*

# Creator-Centric Entry Model — Normative Supplement v1

**Status**: Shipped (V1.35)  
**Document class**: Master (V1.35 lock effective)  
**Created**: 2026-06-06  
**Shipped**: 2026-06-07 (V1.35 P5 spec-tracker-hygiene)  
**Merge target**: cli-spec.md §7
**Scope**: Product-level rules for **when** users enter via `creator` vs `platform` vs `system`  
**Coordinates with**:

- [cli-command-ia.md](cli-command-ia.md) — top-level IA
- [cli-spec.md](cli-spec.md) — command detail
- [work-experience-model.md](work-experience-model.md) — Work journey
- [entity-scope-model.md](entity-scope-model.md) — KB / knowledge scopes

---

## 1. Purpose

Nexus OSS serves two overlapping personas:

1. **Creator operator** — acts as or on behalf of an agent identity (`creator_id`); creative work, local assets, orchestration.
2. **Platform user** — authenticated human with User session; cloud sync, explore, publish, pairing.

V1.35 locks **creator as the creative hub**. Platform capabilities are **optional mounts**, not prerequisites for local-first work.

---

## 2. Entry rules (normative)

| User intent | Primary entry | Notes |
| --- | --- | --- |
| Start or continue creative **Work** | `nexus42 creator run ...` | Default product path (V1.33+ FL-E) |
| Register / switch Creator identity | `nexus42 creator register\|use\|list` | Pure local register allowed pre-release |
| Workspace + SOUL + memory + local assets | `nexus42 creator workspace\|soul\|memory\|kb\|knowledge\|reference` | Bound to active `creator_id` |
| Connect external AI agent | `nexus42 acp agent use` | After `daemon start` |
| Run presets / schedules (power user) | `nexus42 daemon schedule ...` | Advanced; not first-run default |
| User login, cloud sync, explore, publish | `nexus42 platform ...` | Requires User session when platform integration enabled |
| Doctor, config, preset validate | `nexus42 system ...` | Not creator-scoped maintenance |

---

## 3. Pure local vs platform-mounted

### 3.1 Pure local path (default while `platform_integration = paused`)

Minimum chain to first Work (≤7 steps — see cli-spec §7.1):

1. `system doctor`
2. `creator register` (or reuse existing)
3. `creator use <ref>`
4. `creator workspace init`
5. `daemon start`
6. `acp agent use <agent>`
7. `creator run start --idea "..."`

No `platform auth login` or `sync pull` required.

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
| Daemon lifecycle / schedule control | `daemon` | Runtime supervisor, not identity |

---

## 5. Invariants

1. **Single active creator** per CLI session (`creator use`); all `creator *` subcommands resolve against it.
2. **Creative entry** for new users is **`creator run`**, not `daemon schedule` (help text and docs must agree).
3. **`creator kb`** is never generic “all knowledge”; use qualified terms per entity-scope-model §5.4.
4. Platform group remains in IA even when paused — help must say “requires login; skip for local-only”.

---

## 6. Acceptance (spec-level)

1. cli-spec §7 documents local and platform paths without contradiction.
2. V1.35 compass Appendix A maps dual-entry and KB naming issues to P2/P3 plans; cli-command-ia §3.2 holds rename options.
3. Compass P3 locks rename/help strategy for `kb` vs `knowledge`.

---

*Supplement to cli-command-ia.md for V1.35. Implementation tracked by plans P2–P4.*

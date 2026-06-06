# CLI Command Information Architecture — Normative Specification v1

**Status**: Draft (V1.35)  
**Document class**: Draft overlay  
**Created**: 2026-06-06  
**Supersedes**: [cli-spec.md](cli-spec.md) §6.0B (six-group lock) — effective when merged at iteration P5  
**Merge target**: cli-spec.md §6.0B
**Scope**: Top-level `nexus42` command groups, deprecation rules, creator-centric entry  
**Coordinates with**:

- [cli-spec.md](cli-spec.md) — per-command detail (§6 subsections remain authoritative for flags)
- [creator-centric-entry-model.md](creator-centric-entry-model.md) — entry semantics
- [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) — local vs cloud split

---

## 1. Purpose

V1.16 established a six-group CLI (`daemon`, `acp`, `creator`, `sync`, `platform`, `system`). Post-V1.34 product evidence shows:

- **Dual entry confusion**: `daemon schedule` vs `creator run`
- **Sync misplaced**: cloud sync is User/platform-scoped, not peer to creator identity
- **Local-first path obscured**: first-run spec assumes platform auth

V1.35 revises top-level IA to **five groups** while preserving ADR-025 spirit (ACP-first, creator knowledge plane, daemon/acp separation).

---

## 2. Top-level groups (V1.35 target)

| Group | Role | Primary persona |
| --- | --- | --- |
| **`creator`** | Agent identity hub — Work, workspace, assets, register/use | Creator operator |
| **`daemon`** | Runtime supervisor — start/stop, schedules (power user) | Advanced / automation |
| **`acp`** | ACP capability plane — agents, registry, skills, probe | Integrator |
| **`platform`** | User session — auth, **sync**, explore, context, publish | Platform user |
| **`system`** | Local maintenance — doctor, config, preset list/validate, debug | Operator |

**Removed from top-level (V1.35):** standalone **`sync`** → migrate to **`platform sync`**.

No sixth top-level group in V1.35. Pre-release allows deprecation aliases (see §5).

---

## 3. Creator hub principles

1. **Creative default path**: `creator run` is the user-facing Work lifecycle entry (V1.33 FL-E).
2. **Identity anchor**: All `creator *` commands bind to active `creator_id` from `creator use`.
3. **Optional platform mount**: Creator may operate pure-local; platform commands add cloud capabilities when User is logged in.
4. **Subcommand stability**: Existing `creator` subcommands remain unless P3 locks a rename strategy (§3.2).

### 3.1 Creator subcommand tiers

| Tier | Subcommands | UX |
| --- | --- | --- |
| **Primary** | `run`, `workspace`, `register`, `use` | First-run and daily use |
| **Assets** | `soul`, `memory`, `kb`, `knowledge`, `reference`, `world` | Scoped; help must disambiguate KB terms |
| **Platform bridge** | `pair`, `unpair`, `credentials`, `list` (when User logged in) | Optional |
| **Maintenance** | `demo-seed`, `status`, `logout` | Secondary |

### 3.2 `creator kb` vs `creator knowledge` (P3 lock)

**Problem (KCA-003):** Users conflate `creator kb`, `creator knowledge`, and World KB. Evidence and UX IDs: [V1.35 compass Appendix A](../../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md#appendix-a-cli-usability-audit-v135) UX-004.

**Compass must lock one option before P3 implement:**

| Option | Pros | Cons |
| --- | --- | --- |
| A. Help-only qualified labels | No breaking change | Names still collide |
| B. Alias `creator assets` → work index | Matches cli-spec alias direction | Two names to maintain |
| C. Rename `kb` → `work-index` | Clearest | Breaking; scripts |

**Default if compass silent:** Option A. Option C requires `gitnexus_impact` before rename.

**Related deferral:** DF-42 (Local API KB redesign) — out of V1.35 implement scope.

---

## 4. Group responsibilities

### 4.1 `platform` (includes sync)

| Subcommand area | Examples | Requires User login |
| --- | --- | --- |
| Auth | `platform auth login\|logout\|status` | login flow |
| **Sync** | **`platform sync pull\|push\|status`** | yes (when integration enabled) |
| Context | `platform context assemble-moment` | local path shipped; cloud assemble deferred (DF-55) |
| Explore / publish | `platform explore`, `platform publish` | yes |

**Migration (P2):**

- Implement `platform sync` as canonical surface.
- Top-level `nexus42 sync` → deprecated hidden alias forwarding to `platform sync` for ≥1 iteration.
- Update cli-spec §6.7 boundary table and shell completion.

### 4.2 `daemon`

- Lifecycle: `start`, `stop`, `status`, `logs`, `doctor`
- Orchestration control: `schedule add|edit|...` — **advanced**; document as power-user path
- Must not appear as primary path in root `--long-about`

### 4.3 `acp`

- Unchanged separation from daemon (negotiation vs runtime control)
- Worker entry points remain hidden (`acp-worker`, `daemon-run`)

### 4.4 `system`

- `doctor`, `config`, `completion`, `debug`, **`preset list|validate`**
- Not creator-scoped; safe for CI and support

---

## 5. Deprecation and compatibility

| Legacy | Target | V1.35 rule |
| --- | --- | --- |
| `nexus42 sync *` | `nexus42 platform sync *` | Deprecated alias; stderr warning once per process |
| `daemon schedule` as first-run hint | `creator run` | Help text only; no command removal |
| Top-level `preset` (never shipped) | `system preset`, `creator run` | Document only (DF-52) |

**Hard delete** of `sync` top-level: **Out of V1.35** — earliest V1.36 after alias period.

---

## 6. First-run paths (summary)

Detailed steps: cli-spec §7. Normative split:

| Path | When | Platform auth |
| --- | --- | --- |
| **Local-first** (§7.1) | Default; `platform_integration = paused` | Not required |
| **Platform-mounted** (§7.2) | User wants cloud worlds / sync | Required |

Local-first must reach `creator run start` in ≤7 commands (see creator-centric-entry-model §3.1).

---

## 7. Help and discoverability rules (P2/P3 implement)

1. Root `long_about` mentions **`creator run`** and **`creator workspace init`**, not `daemon schedule`.
2. `creator --help` ordering: surface `run` near top (implementation detail — P3).
3. Every ambiguous term (`kb`, `knowledge`, `KB`) uses qualified phrases in help strings per entity-scope-model §5.4.
4. `platform --help` subtext: "Requires User login; skip entirely for local-only workflows."

---

## 8. Acceptance (spec-level)

1. This document and cli-spec §6 header agree on five groups post-P2.
2. `nexus42 --help` lists five groups; sync appears under platform or as deprecated alias only.
3. Compass Appendix A items UX-001..UX-010 mapped to P2/P3 plans and closed in P5 where addressed.
4. Compass success criteria §1.4 satisfied at iteration close.

---

## 9. Change control

- **Authority**: Active V1.35 compass > this spec > cli-spec §6.0B legacy text until P5 hygiene merge.
- **Platform unpause**: Does not automatically add top-level groups; extends `platform` subcommands only.
- **Impact before rename**: `gitnexus_impact` required for any `creator kb` rename (P3).

---

*Draft for V1.35. Implementation: plans P1 (docs), P2 (sync migration), P3 (creator hub).*

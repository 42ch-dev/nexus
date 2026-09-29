# CLI Command Information Architecture — Normative Specification v1

**Status**: Shipped (V1.35)  
**Document class**: Master (historical V1.35 lock; retained rationale supplement)  
**Created**: 2026-06-06  
**Shipped**: 2026-06-07 (V1.35 P5 spec-tracker-hygiene)  
**Supersedes**: pre-V1.35 [cli-spec.md](../cli/cli-spec.md) §6.0B six-group lock
**Merged into**: [cli-spec.md](../cli/cli-spec.md) §6.0B in V1.35 P5; retained as shipped IA rationale and acceptance supplement
**Scope**: Top-level `nexus42` command groups, deprecation rules, creator-centric entry  
**Supersession (v1.193 P2)**: Current command authority is [cli-spec.md](../cli/cli-spec.md) §6.0B plus its delivered v1.193 P2 overlay, checked against `apps/nexus42/src/cli.rs`. The daemon group (including lifecycle/schedule controls and hidden `daemon-run`), `web-embed`, DaemonClient leaves, `creator run` / `creator bootstrap` and the other incomplete runner entries, and the top-level `sync` alias were removed. `platform sync` is canonical; the direct-core `cli` cohort is the ordinary default (`apps/nexus42/Cargo.toml`). The V1.35 rationale and later pre-retirement amendments below are historical, not current command or onboarding rules.
**Coordinates with**:

- [cli-spec.md](../cli/cli-spec.md) — per-command detail (§6 subsections remain authoritative for flags)
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

## 2. Top-level groups (historical V1.35 target)

| Group | Role | Primary persona |
| --- | --- | --- |
| **`creator`** | Agent identity hub — Work, workspace, assets, register/use | Creator operator |
| **`daemon`** (historical; retired v1.193 P2) | Runtime supervisor — start/stop, schedules (power user) | Advanced / automation |
| **`acp`** | ACP capability plane — agents, registry, skills, probe | Integrator |
| **`platform`** | User session — auth, **sync**, explore, context, publish | Platform user |
| **`system`** | Local maintenance — doctor, config, preset list/validate, debug | Operator |

**Removed from top-level (V1.35):** standalone **`sync`** → migrate to **`platform sync`**.

No sixth top-level group in V1.35. Pre-release allows deprecation aliases (see §5).

---

## 3. Creator hub principles (V1.35 historical lock)

1. **Historical creative default path**: `creator run` was the user-facing Work lifecycle entry (V1.33 FL-E); its CLI entry was removed in v1.193 P2.
2. **Identity anchor**: All `creator *` commands bind to active `creator_id` from `creator use`.
3. **Optional platform mount**: Creator may operate pure-local; platform commands add cloud capabilities when User is logged in.
4. **Subcommand stability**: Existing `creator` subcommands remain unless P3 locks a rename strategy (§3.2).

### 3.1 Creator subcommand tiers

| Tier | Subcommands | UX |
| --- | --- | --- |
| **Primary** | `bootstrap`, `run` (both retired v1.193 P2), `works`, `workspace`, `register`, `use` | Historical first-run and daily-use tier |
| **Assets** | `soul`, `memory`, `kb`, `knowledge`, `reference`, `world` | Scoped; help must disambiguate KB terms |
| **Platform bridge** | `pair`, `unpair`, `credentials`, `list` (when User logged in) | Optional |
| **Maintenance** | `demo-seed`, `status`, `logout` | Secondary |

**Historical `creator run` amendment (V1.45 target — replaced V1.44 bespoke subcommands; runner retired v1.193 P2):**

| Entry | Role |
| --- | --- |
| `creator run <preset_id> [<work_id>]` (retired v1.193 P2) | Historical generic preset dispatch; see [creator-run-preset-entry.md](creator-run-preset-entry.md) |
| `creator bootstrap …` (retired v1.193 P2) | Historical composite Work onboarding (V1.45 generic runner; see creator-run-preset-entry.md) |
| `creator works …` | Atomic Work ops only (`inspire`, `reopen`, `resume-chain`, `reconcile-chapters`, …) |

**Removed in V1.45 (hard delete):** `review-master`, `audit-chapter`, `stage`, `start`, `continue`, `resume`, `reconcile-chapters` under `creator run`.

<!-- V1.44 shipped table (superseded by V1.45):
| `review-master <work_id>` | … | V1.44 P1 |
| `audit-chapter <work_id>` | … | V1.44 P0 |
-->

### 3.2 `creator kb` vs `creator knowledge` (P3 lock)

**Problem (KCA-003):** Users conflate `creator kb`, `creator knowledge`, and World KB. Evidence and UX IDs: V1.35 UX-004.

**Compass must lock one option before P3 implement:**

| Option | Pros | Cons |
| --- | --- | --- |
| A. Help-only qualified labels | No breaking change | Names still collide |
| B. Alias `creator assets` → work index | Matches cli-spec alias direction | Two names to maintain |
| C. Rename `kb` → `work-index` | Clearest | Breaking; scripts |

**Default:** Option A. Option C requires `gitnexus_impact` before rename.

**Related deferral:** DF-42 (Daemon API KB redesign) — out of V1.35 implement scope.

**Durable roadmap:** DR-19 (`creator kb` vs `creator knowledge` disambiguation decision).

---

## 4. Group responsibilities (historical V1.35 IA)

### 4.1 `platform` (includes sync)

| Subcommand area | Examples | Requires User login |
| --- | --- | --- |
| Auth | `platform auth login\|logout\|status` | login flow |
| **Sync** | **`platform sync pull\|push\|status`** | yes (when integration enabled) |
| Context | `platform context assemble-moment` | local path shipped; cloud assemble deferred (DF-55) |
| Explore / publish | `platform explore`, `platform publish` | yes |

**Historical migration (V1.35 P2; alias removed in v1.193 P2):**

- Implement `platform sync` as canonical surface.
- Top-level `nexus42 sync` → deprecated hidden alias forwarding to `platform sync` for ≥1 iteration (historical transition only; the alias is now deleted).
- Update cli-spec §6.7 boundary table and shell completion.

### 4.2 `daemon` (historical — entire group retired v1.193 P2)

- Lifecycle: `start`, `stop`, `status`, `logs`, `doctor`
- Orchestration control: `schedule add|edit|...` — **advanced**; document as power-user path
- Must not appear as primary path in root `--long-about`

### 4.3 `acp`

- Unchanged separation from daemon (negotiation vs runtime control)
- Historical worker entry points were hidden (`acp-worker`, `daemon-run`); `daemon-run` was deleted in v1.193 P2, not retained as a hidden entry.

### 4.4 `system`

- `doctor`, `config`, `completion`, `debug`, **`preset list|validate`**
- Not creator-scoped; safe for CI and support

---

## 5. Deprecation and compatibility (historical V1.35 policy)

The table records the V1.35 transition, not live aliases or runner guidance. The top-level `sync` alias, `daemon` group, and `creator run` entry were deleted in v1.193 P2.

| Legacy | Target | V1.35 rule |
| --- | --- | --- |
| `nexus42 sync *` | `nexus42 platform sync *` | Deprecated alias; stderr warning once per process |
| `daemon schedule` as first-run hint | `creator run` | Help text only; no command removal |
| Top-level `preset` (never shipped) | `system preset`, `creator run` | Document only (DF-52) |

**Historical V1.35 deferral:** hard delete of top-level `sync` was out of V1.35 — earliest V1.36 after the alias period (DR-53). **Delivered v1.193 P2:** the alias is deleted; only `platform sync` remains canonical.

---

## 6. First-run paths (historical V1.35 summary)

Historical detailed steps: cli-spec §7. V1.35 normative split (current entry authority: cli-spec §6.0B + v1.193 P2 overlay):

| Path | When | Platform auth |
| --- | --- | --- |
| **Local-first** (§7.1) | Default; `platform_integration = paused` | Not required |
| **Platform-mounted** (§7.2) | User wants cloud worlds / sync | Required |

Historical V1.35 acceptance required local-first to reach `creator bootstrap` in ≤7 commands (see creator-centric-entry-model §3.1). `creator bootstrap` was removed in v1.193 P2; this is not a current onboarding chain.

---

## 7. Help and discoverability rules (historical V1.35 P2/P3 implement)

The `creator run` / `daemon schedule` help rules below describe the retired entry model, not current help requirements.

1. Root `long_about` mentions **`creator run`** and **`creator workspace init`**, not `daemon schedule`.
2. `creator --help` ordering: surface `run` near top (implementation detail — P3).
3. Every ambiguous term (`kb`, `knowledge`, `KB`) uses qualified phrases in help strings per entity-scope-model §5.4.
4. `platform --help` subtext: "Requires User login; skip entirely for local-only workflows."

---

## 8. Acceptance (historical V1.35 spec-level criteria)

1. This document and cli-spec §6 header agree on five groups post-P2.
2. `nexus42 --help` lists five groups; sync appears under platform or as deprecated alias only.
3. Compass Appendix A items UX-001..UX-010 mapped to P2/P3 plans and closed in P5 where addressed.
4. Compass success criteria §1.4 satisfied at iteration close.

---

## 9. Change control

- **Historical authority**: this spec overrode cli-spec §6.0B legacy text until the V1.35 P5 hygiene merge. Current authority is cli-spec §6.0B + the delivered v1.193 P2 overlay, not this rationale supplement.
- **Platform unpause**: Does not automatically add top-level groups; extends `platform` subcommands only.
- **Impact before rename**: `gitnexus_impact` required for any `creator kb` rename (P3).

---

*Shipped in V1.35. Implementation: plans P1 (docs), P2 (sync migration), P3 (creator hub), P5 (spec/tracker hygiene).*

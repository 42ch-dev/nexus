# Novel Author Experience — Normative Supplement v1

**Status**: **Shipped (V1.46)** — overlay merged into Master; baseline **Shipped (V1.43)** + V1.45 CLI IA amendments  
**Document class**: Feature line (author experience supplement)  
**Created**: 2026-06-12  
**Last updated**: 2026-06-15 (V1.46 P-last — promoted from Draft (V1.46) to Shipped; P0 author desk delta + P1 spec sweep + P2 chapter hints shipped)  
**Scope**: End-user **ongoing serial** happy path — normative CLI surfaces, remediation chains, and author visibility (spec-only SSOT; **no** `docs/novel-writing-quickstart.md` after P1)  
**Coordinates with**:

- [creator-run-preset-entry.md](creator-run-preset-entry.md) — **Shipped Master V1.45** — CLI IA, preset ids, flags (remediation target for runtime copy)
- [creator-centric-entry-model.md](creator-centric-entry-model.md) — §3.1 local bootstrap (≤7 steps)
- [cli-spec.md](cli-spec.md) — §7 first-run UX principles
- [novel-workflow-profile.md](novel-workflow-profile.md) — artifact layout + completion §6
- [novel-quality-loop.md](novel-quality-loop.md) — findings + review visibility
- [creator-workflow.md](creator-workflow.md) — FL-E stage names in narrative

**Iteration compass**: [v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md](../../iterations/v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md)

---

## 1. Purpose

V1.36–V1.45 implemented novel-writing **capabilities** across crates. V1.46 does **not** add a new profile or preset grammar. It:

1. **Embeds the author happy path** (formerly BL-10 quickstart) in this spec §3 — compact ~80 lines.
2. **Closes author-desk deltas** — `--json` `findings[]`, per-finding remediation, novel-only scope (P0).
3. **Retires duplicate end-user doc** — `docs/novel-writing-quickstart.md` deleted in P1; agents cite `.mstar/knowledge/specs/` only.

**Part II (optional)** — multi-work switch, multi-volume, inspiration pool — documentation pointers only; shipped in V1.41–V1.44.

---

## 2. Document map (V1.46)

| Section | Content | Owner plan |
| --- | --- | --- |
| §3 Author path | Bootstrap → first chapter → serial → quality loop → completion | P1 (narrative); CLI detail in Master |
| §4 Author visibility | Human + machine-readable status surfaces | P0 delta on V1.43 baseline |
| §4.1 `--json` contract | `findings[]` + optional `findings_stale` | P0 |
| §5 Residual pointer | `status.json` SSOT | P-last |

**Invariant**: Every command in §3 must exist in [creator-run-preset-entry.md](creator-run-preset-entry.md) or [cli-spec.md](cli-spec.md) at ship time.

---

## 3. Author path — ongoing serial (Part I)

> **CLI detail**: [creator-run-preset-entry.md](creator-run-preset-entry.md). This section is the **narrative** happy path only.

### 3.1 Prerequisites and bootstrap

```bash
nexus42 system doctor
nexus42 creator register --name "Your Name"
nexus42 creator use <handle>
nexus42 creator workspace init
nexus42 daemon start    # separate terminal
```

### 3.2 World + project init

```bash
nexus42 creator world create --title "Neon River"   # → wld_…
nexus42 creator bootstrap --idea "A solpac noir detective story in a floating canal city"
# → Work created, init preset, intake → produce chain
```

Gate/scaffold failures: remediation cites this spec §3.2 or [creator-run-preset-entry.md](creator-run-preset-entry.md) bootstrap section — **not** a quickstart file.

### 3.3 First chapter and serial production

First chapter: outline → draft → finalize via `novel-writing` preset chain (auto-chain default **on**).

```bash
nexus42 creator works status    # current chapter, progress, next action
```

Serial chapter 2+: daemon auto-chain; inject direction:

```bash
nexus42 creator works inspire <work_id> --note "the partner is the informant"
```

On-disk chapter files: see [novel-workflow-profile.md](novel-workflow-profile.md); missing paths surfaced in status (P2 on-disk hints).

### 3.4 Quality loop — dual preset table (Grill #19)

> **V1.47 shipped**: Review preset produces findings per [novel-quality-loop.md §8](novel-quality-loop.md#8-reflection-loop-output-contract-v147-draft) (P0). The preset is named `novel-chapter-review` (replaces the former generic `reflection-loop` demo).

| Intent | Preset id | When |
| --- | --- | --- |
| Generate / refresh findings | `novel-chapter-review` | After draft milestones; produces candidate findings |
| Master decision on open findings | `novel-review-master` | When findings need accept/reject/defer |

```bash
nexus42 creator works status                              # list open findings (human)
nexus42 creator run novel-review-master <work_id>         # enqueue master review
nexus42 creator run novel-chapter-review <work_id>           # optional: generate findings
```

**Remediation (P0, Grill #7)**: `works status` uses **per-finding `routing_hint`** only — no blanket footer pointing only at `novel-chapter-review`. When **zero** open findings, suggest `creator run novel-review-master <work_id>` if author may need a master pass.

96h master-review banner: visible on `creator works status` (V1.39 P4 baseline).

### 3.5 Completion

When all planned chapters finalized:

```bash
nexus42 creator works status    # COMPLETED marker
nexus42 creator works completion-lock release <work_id>   # optional: write more
nexus42 creator works reopen <work_id> --reason "epilogue"
```

Auto-chain stops on completion (`reject_produce_when_novel_complete` — V1.39+).

### 3.6 Part II appendix (optional, doc-only)

| Topic | Surface | Spec |
| --- | --- | --- |
| Multi-work desk | `creator works list/use/status` | [novel-multi-work-lifecycle.md](novel-multi-work-lifecycle.md) |
| Multi-volume | `volume` in status tables | [novel-workflow-profile.md](novel-workflow-profile.md) §multi-volume |
| Inspiration pool | `creator works pool …` | [novel-work-pool.md](novel-work-pool.md) |

---

## 4. Author visibility (P2 baseline + V1.46 delta)

Authors must answer without reading raw JSON APIs (human path). **Novel profile only** for findings (Grill #6) — generic `works status` does **not** fetch findings.

| Question | Surface (minimum) | Status |
| --- | --- | --- |
| Which chapter is active? | `creator works status` — `current_chapter` + chapter table | Shipped V1.43 P2 |
| Is the Work complete? | Completed banner + `COMPLETED` marker | Shipped V1.43 P2 |
| Open findings? | Count + severity; per-row hints | Shipped V1.43 P2; **remediation delta P0** |
| 96h master-review banner? | Stale banner on status path | Shipped V1.39 P4 |
| Run master review? | `creator run novel-review-master [<work_id>] …` | Shipped V1.45 |

Normative finding semantics: [novel-quality-loop.md](novel-quality-loop.md) §3.4.

### 4.1 Machine-readable status (`--json`, V1.46 P0)

For **`work_profile=novel`** only, `creator works status <work_id> --json` **extends** the daemon GET work payload:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| *(work fields)* | object | yes | Unchanged from daemon GET `/v1/local/works/{id}` |
| `findings` | array | conditional | Three-state: present-with-data when the findings endpoint is reachable; present-empty when reachable but no open findings; **omitted** when the daemon findings endpoint is unreachable (best-effort degradation). See §4.1 best-effort paragraph (W-1 reconcile) |
| `findings_truncated` | boolean | no | Present (and `true`) only when `findings[]` hit the fetch cap (`FINDINGS_FETCH_LIMIT = 50`); signals more open findings may exist beyond the fetched page. Omitted otherwise (qc3 F-003) |
| `findings_stale` | object | no | Present when 96h master-review stale banner would show (human parity). **Creator-global scope** (not work-scoped): the payload mirrors the human-path stale banner which is printed before the work block and spans all of the creator's works. A JSON consumer must not assume `findings_stale.stale_count` is scoped to the queried `work_id` (W-2 reconcile) |

Generic (non-novel) works: **omit** `findings` fetch; json output is work API only.

**Best-effort degradation**: `findings` is fetched via the daemon findings endpoint
with a short timeout (`FINDINGS_FETCH_TIMEOUT`, 5 s). When that endpoint is
unreachable, `findings` is **omitted** (rather than fabricated as an empty array)
so a JSON consumer can distinguish a genuinely findings-free Work from a
transient daemon fault. `findings_stale` follows the same novel-only,
best-effort contract and uses a matching short timeout (`STALE_FETCH_TIMEOUT`,
5 s; qc3 F-002) so neither subcall can block the JSON status command beyond ~5 s.
The two subcalls run concurrently (`tokio::join!`; qc3 F-001), bounding the
JSON-path fetch by the slower of the two rather than their sum.

---

## 5. CLI copy alignment (remediation SSOT)

When error/remediation conditions occur, user-visible output must include a **single-line next action** referencing:

- **CLI commands / preset ids** → [creator-run-preset-entry.md](creator-run-preset-entry.md)
- **Author narrative** → this document §3

| Condition | Minimum remediation |
| --- | --- |
| Daemon not reachable | Start daemon; cite §3.1 |
| `preset_gates_failed` | Name gate; cite §3.2 or §3.3 |
| Missing scaffold / intake incomplete | Cite §3.2 |
| Work completed (auto-chain stopped) | Cite §3.5 |
| Open findings (when shown) | Per-finding hint or §3.4 review-master |

**V1.46 P1**: remove all `docs/novel-writing-quickstart.md §N` runtime references.

---

## 6. P-last author-path tech-debt (pointer)

See [2026-06-14-v1.46-hygiene-and-closeout.md](../../plans/2026-06-14-v1.46-hygiene-and-closeout.md). This spec does not duplicate `status.json` rows.

---

## 7. Promotion (iteration close)

At V1.46 P-last:

- [ ] Draft → **Shipped (V1.46)** header
- [ ] BL-10 archive supersede note in shipped tracker (Grill #15)
- [ ] Confirm zero runtime quickstart references

# Rust Core Service Boundary — Cutover Record Historical Notes

**Provenance**: frozen from [`specs/active/architecture/rust-core-service-boundary.md`](../../active/architecture/rust-core-service-boundary.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 (2026-09-21, commit `e3df40f1`) completed the daemon-composition cutover; the removed blocks are the file's own self-declared migration/planning records ("Historical record", "Historical M1 planning record", "Historical cutover instructions", milestone delivery table). Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Original Status line (replaced by a present-tense one)

**Status:** Accepted target — locked 2026-09-13; **v1.193 overlay delivered (P2 complete 2026-09-21).** The **M1 subset is exercised** (v1.189 PR #306, merge `71e01cf9`, 2026-09-15). **v1.192** delivered the Electron desktop cutover and retired the native host. **v1.193 P2 delivered the remaining cutover:** the whole `nexus42 daemon` group, hidden `daemon-run`, Model A `mcp serve`/`host-call`, the app-only `legacy-cli` / `basic-cli` / `web-embed` / `connect-client` / `embedded-mcp` selectors, the `nexus-daemon-runtime` crate and the DaemonClient CLI leaves are **deleted** — retained leaves call core/cloud/Connect directly, with no TS-launcher alias and no permanent legacy fallback. This document is normative for that boundary and is self-contained; the per-leaf command inventory for the iteration is process material, not a tracked contract. Do **not** describe the `Current:` records below as the shipped state — they are the pre-cutover migration record.

## §0 removed "Exercised M1 subset" row

| **Exercised M1 subset** | This document + merged v1.189 on `main` (`71e01cf9`) | World KB graph/patch, candidates read-only projection, napi/provider ACP vertical, standalone TS M1 routes, Cargo-free stable-interface DX, Electron development GO. Not full API/CLI cutover |

## Research-baseline paragraph (removed)

Historical 2026-09-12 research (source baseline `3bb262b`) is advisory structure only. It is not runtime proof, a measurement, or this spec's authority. The merged v1.189 record for the exercised M1 subset is `71e01cf9e1a8c64cb68062ac9a08762e2156a925` (PR #306); the baseline has since advanced — v1.192 (RFT-09 cutover + unsigned packaging) and v1.193 P2 (obsolete-host retirement) are **merged on `main`**, so the retained `cli`/`connect-host` cohorts and the Electron host are shipped current state, not target state. Inherited v1.188 reliability remains `bbaae32d422b673576d683859b474da6bd787743`.

## §1 historical problem-statement note and retired-topology bullets (removed)

> **Historical problem statement (before the v1.193 P2 cutover).** The mixed daemon-handler/HTTP-client topology below is retired; current identities and ownership are in §§0, 2, and 4.

Operators and authors already have three consumption ends. The local stack that serves them mixes:

- authorized domain reads/writes, OCC/CAS, storage, and recovery inside daemon HTTP handlers and `WorkspaceState`
- a CLI that is often a thin HTTP client rather than a direct library caller
- provider/Host/ACP lifecycle inside the same integrated process as storage
- a native desktop shell supervising a `nexus42` sidecar (retired in v1.192; the desktop is now the Electron host)

## Former §3 (Topology before the v1.193 cutover — migration record)

## 3. Topology before the v1.193 cutover (migration record)

**Historical record.** The numbered items below describe the mixed shipped
topology *before* v1.193 P2 finished the cutover; they are kept so the
migration is auditable, not as the current state. Delivered form: the
ordinary `nexus42` default is the `cli` cohort (no embedded `apps/web`, no
`legacy-cli`/`web-embed`, no daemon launch), and every retained CLI leaf
calls the Rust authority directly.

Kept as the record of the mix at lock time; nothing below is a supported instruction any more.

1. **`nexus42`** ordinary default is the `cli` cohort: direct core/cloud/Connect, with **no** embedded `apps/web`, **no** `nexus_daemon_runtime::boot::run_daemon` and **no** hidden `daemon-run` (all deleted in v1.193 P2). The **development/runtime entry cutover** to standalone TS + daemon-free CLI (RFT-07/RFT-08) landed with that deletion, not in M3.
2. **Daemon API** remains loopback HTTP under `/v1/daemon/*` for unmigrated families, with unguarded health/status/cert routes, API-key Tier-1, and API-key + active-Creator Tier-2. The M1 World KB graph/patch/candidates-read and provider-session slice also exist on standalone `apps/nexus-service`. Completing M2 requires every retained family on that TS service with a real Rust owner — not a 501, sample, or old-daemon proxy.
3. **Creator workflow/control CLI leaves** now call Rust (core/cloud/Connect) directly; the `DaemonClient` and the app-only cohort selectors are deleted (v1.193 P2). Several world/KB/memory/SOUL/cron/chronology/directive paths already owned local SQLite/filesystem directly and keep doing so. The M1 World KB graph/patch slice that pioneered the direct path is part of the ordinary `cli` cohort (RFT-08 complete, including Works `list|status|use`).
4. **World KB graph + entity patch** now have one Rust owner (`nexus-core`) with old HTTP translation **and** daemon-free CLI. They remain distinct from local-DB `creator world kb edit` (direct SQLite, no OCC). Candidate **promotion/merge/relationship writes**, packs, forks, rules, findings, and remaining World/Work families are **not** in the M1 subset.
5. **`nexus-runtime`** parses `--listen` / `--allow-peer` / `--home`, starts Connect, and **never** calls `run_daemon`. It is built `--no-default-features --features connect-host` — Connect-only and Node-free, with the `legacy-cli` coupling gone with that cohort (v1.193 P2). Do not fold full Host/scheduler/SPA into this binary.
6. **Desktop** ships as the Electron host (`apps/desktop-electron`) wrapping `apps/web/dist` with an app-managed TS service over the Rust authority. Support floor: macOS arm64 and x86_64. v1.192 delivered the formal switch (RFT-09) and the unsigned `.app`/`.dmg` half of RFT-10, and retired the native v2 composition (`apps/desktop` plus its target-suffixed `nexus42` sidecar); that delivery supersedes the M1 Electron **development** GO as the desktop's product basis. Production signing (RFT-10) and dual-architecture GUI qualification remain open.
7. **Design Studio** is a daemon-free Vite gallery on port 5174. `pnpm run dev:web` / `dev:design-studio` already exist as direct entries.
8. **Providers:** Rust ACP/Claude/Codex/DSH remain. TS currently exposes the M1 ACP vertical; M2 must keep **all four** families available in the TS service through stable ports. DSH stays `cancellation:false` and is never the cancel proof. Independent Rust consumers must not require Node.

`WorkspaceState` is the current daemon aggregate, not the target facade. `nexus-local-db` `runtime_lock` read-then-update without an expected-holder predicate is **not** the target cross-process writer protocol.

## §4 migration-edge note (removed)

> **Migration edge only:** “Old HTTP adapter” in the frozen target diagram and temporary-adapter rule below describes the cutover path, not a surviving host. That adapter and `nexus-daemon-runtime` were deleted in v1.193 P2; browser/Electron now reach core through the standalone TS/native service.

## §4.2 historical cutover instructions (removed note + "retire" paragraph)

> **Historical cutover instructions / v1.195 opening target (next three paragraphs).** Preserve their original “route-not-migrated” / “not yet delivered” wording as the starting record, not today's route availability. Workflow control, same-run observation, and Compute are implemented and exercised; current support limits and outstanding product qualification are in §7.5–§7.5.1.

Retire CLI `creator run`, `bootstrap`, Work `intake`/`resume-chain`, `preset run`, Character `run`/`soul reflect`, reference refresh, compute run and old runtime catalog/tool entrances where the complete direct production/observation closure does not exist. Preserve underlying Work/execution/Host/capture/SOUL/reference/compute libraries. `resume_driven_sessions` is not Work auto-chain; HostHandle query does not supply the Character CLI capture stream. Existing TS schedule add/signal and provider routes are not claimed as equivalent workflow replacements; TS compute run is currently route-not-migrated and forced native SOUL reflection has no synthesizer.


## §4.2 neutral-production moves paragraph (removed cutover instruction)

Neutral production `HostPromptExecutor` moves into optional core execution with provider-host selected, and `AcpSoulNarrativeSynthesizer` into optional core execution. Existing core directive/pack/capability implementations replace daemon wrappers rather than gain duplicates. PATH enrichment moves to existing agent-host discovery for the CLI and remaining discovery probe; Connect-only runtime drops provider probing and gains no Host dependency. Retained semantic tests move to core before the obsolete Rust-host crate and server fixtures are deleted.

## §5 removed rows (retired hosts)

| Integrated `nexus42` daemon + Daemon API | **Retired in v1.193 P2** (was the shipped operator/HTTP host for unmigrated families) | Was owned by the deleted `daemon start` | Deleted, not translated | The CLI group and the obsolete Rust host/SPA composition are gone. It was **not** retargeted at the TS service. TS/Electron remain the HTTP host |
| Native sidecar (retired in v1.192) | Historical record: desktop packaging before the cutover | Sidecar child of the retired native app | Retired with the accepted RFT-09 cutover and unsigned packaging | No product obligation. Retirement was explicit in v1.192, not a silent M2 deletion, and no second desktop UI was introduced |

## §7.1 historical M1 planning record (removed)

> **Historical M1 planning record below (§7.1).** The HTTP transports, daemon-mediated Works reads, leaf counts, and remaining-family assignments describe the M1 baseline and its planned expansion, not current CLI dependencies. The delivered direct-core CLI disposition above supersedes those transport claims; current RFT-05–07 implementation/qualification is in §7.5.

**M1 first slice (RFT-01) — delivered in v1.189:**

| Operator action | Current public command | Current transport | Target |
| --- | --- | --- | --- |
| Read World KB graph (entities + per-row `version`, relationships, source anchors) | `nexus42 creator world kb graph --world-id` | `GET /v1/daemon/worlds/{world_id}/kb/graph` | Same DTO through Rust service, old HTTP translation, **and** daemon-free CLI |
| Authorized entity write with expected-version CAS | `nexus42 creator world kb entity patch --world-id --entity-id --expected-version …` | `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` | Same CAS/409 `world_kb_conflict` semantics through all three callers |

Keep the existing distinction: local `creator world kb edit` is a different command (direct SQLite, no OCC). M1 does not merge or delete it.

Patch preserves create-or-update: absent entity + expected_version 0 + valid title/block type creates revision 1 with HTTP 200; absent + positive version conflicts. Existing merged/deleted denial, body/aliases/modules omission/null semantics, provenance, and graph caps remain. The core owns one private transaction for authorization, canonical spoke lifecycle, CAS, response projection and durable change commit.

The existing canvas unconditionally requests the read-only candidates list, so that existing projection/cursor/auth path is an ancillary M1 core/native/HTTP dependency. It must return real pending rows, not treat an unported error as an empty list. Candidate promotion/merge/relationship writes stay RFT-05. Existing canvas has no create-entity button: browser-context client creation followed by canvas observation proves the create branch without inventing UI.

**Works reads stay assigned to the complete basic CLI (RFT-08), with domain work in RFT-05.** `creator works list|status|use` are currently Daemon API-owned. They are **not** the M1 slice and **must not** be reclassified as operator-only or dropped because they are daemon-mediated today. M2 default-entry cutover does **not** move them to “wait for M3”.

Do **not** claim that all basic commands already work as direct library calls. Static inventory: 252 feature-on clap terminal leaves (247 default features). Many authoring leaves still require the daemon. Local SQLite/filesystem owners that already exist stay local; they are not a new architecture gain.

## §7.4 historical dormant-leaf roster + delivery note (removed)

Historical roster (pre-v1.193 overlay) of the callable-but-incomplete leaves at that time:

- hidden hard-deprecated `creator workspace clone` — now an **unknown command** (the retired leaf is asserted as `retired_creator_workspace_clone_is_unknown`)
- coming-soon `workspace link|unlink|status`
- platform-only `explore browse|search`, deferred `platform context assemble`, coming-soon `publish`
- visible deprecated `system preset` forwarding alias; hidden but callable top-level `sync`/`preset`/`capability`

**Delivered (v1.193 P2):** these dormant rows lost their CLI entries. They are not domain capabilities, and the removal was explicit in the v1.193 overlay rather than a quiet cleanup.

## §7.5 milestone framing + RFT delivery table (removed; program keys/delivery record)

RFT-00–04 shipped together as milestone **RFT-M1** (v1.189). RFT-05–08 are one milestone **RFT-M2** (v1.190), including default product-entry cutover. RFT-09–11 are **RFT-M3**. Do not split M2 into extra milestone iterations, and do not hide any retained M2 family in M3.

| Destination | What moves there | Milestone status |
| --- | --- | --- |
| RFT-00 | Inventory, decision/proof matrix, Cargo-free stable-backend UI loops | Shipped (v1.189) |
| RFT-01 | Neutral Rust service + M1 World KB graph/patch slice + concurrent writer protocol | Shipped (v1.189) |
| RFT-02 | napi adapter, provider ports, ACP LocalSet lifecycle | Shipped (v1.189) |
| RFT-03 | Native npm + packaged Electron **development** feasibility go/no-go | Shipped development GO (v1.189). Production signing is RFT-10 |
| RFT-04 | Standalone TS service + browser vertical of the M1 slice | Shipped (v1.189). Complete TS API is RFT-07 |
| RFT-05 | Remaining World/Work/KB/narrative/fork families, including Works reads/writes beyond the M1 slice | **Implemented and exercised family paths** in `apps/nexus-service` (`works.ts`, `world-kb.ts`, World/content/knowledge modules) over `nexus-core`. Not “M2 / not started”; product-wide acceptance/QA is not established by route-level exercise alone. |
| RFT-06 | Actor/Character/admission/memory/context | **Implemented and exercised family paths** in `apps/nexus-service` (`actors.ts`, `memory.ts`, `context.ts`) over the core admission/domain owners. Product acceptance/QA remains a separate qualification, not an implementation-absence claim. |
| RFT-07 | Execution/scheduler/Host/providers/capabilities/MCP/Connect control plane **and** complete TS API/default service entry | **Implemented and exercised retained route families**, including `execution.ts`, `workflow-observation.ts`, and `compute.ts`, over core execution/Compute owners. Default host cutover is delivered (v1.193 P2); outstanding product acceptance/QA, Run Studio completeness, and specific unmounted identities (§7.5.1) do not mean the service is unstarted or wholly unexercised. |
| RFT-08 | Complete independent Rust CLI + headless product cutover (default authoring entry; Connect-only runtime). **v1.193 target:** no CLI operator control surface for the TS service | **Delivered (v1.193 P2)** — the ordinary `cli` cohort is the default authoring entry, `nexus-runtime` is the Connect-only headless binary, and no CLI operator control surface for the TS service exists. The distinct product-qualification obligations above remain open. |
| RFT-09 | Formal desktop cutover (Electron host after the M1 development GO; reuse web/Studio; no visual redesign) | M3 / **delivered in v1.192** (accepted) |
| RFT-10 | Production distribution / Developer ID signing / notarization / stapling | M3. **v1.192 delivers the unsigned half** (`.app` and `.dmg`, both macOS architectures, no Apple credentials required); that delivery is not dual-architecture GUI qualification. Signing remains the durable destination and is a Non-Goal until explicit release authorization |
| RFT-11 | Obsolete-host retirement **and** retained v1.188 P5 public first-run / Quick Start / live request | **v1.192** retired the native host. **v1.193 P2 delivered:** the remaining daemon/SPA/`legacy-cli` composition and the dormant CLI rows listed in §7.4 were retired in that iteration. **v1.195 P3** exercises the retained public first-run as a clean-home deterministic example with its Quick Start/startup guidance, and one user-authorized live request has been exercised against the pinned official origin (all of it pending plan QC/QA and iteration integration) — see §7.5.1 |

---

## Annotation drops (reworded in place; originals named here)

- §0 "Shipped current policy" row: "(v1.193 P2)", "Since the v1.192 cutover …", and the trailing "daemon-runtime is listed in *Coordinates with* only as the historical host Master" clause.
- §0 "Product names" row: "the retired integrated daemon runtime is no longer a product name".
- §0 Activation rule tail: "**Delivered (v1.193 P2):** that gate fired for every retained family — …".
- §2 identity rows: "**Delivered (v1.193 P2):**" / "**Retired in v1.193 P2.**" prefixes; "(retired in v1.192; the desktop is now the Electron host)"; "The desktop host cut over to Electron in v1.192 (RFT-09 accepted)".
- §4.1: "(`apps/desktop` was retired with the native composition in v1.192)"; "**Delivered (v1.193 P2):**" prefix on the `nexus42` binary row; "The M1 commands … are **shipped**" kept as the current command list without the M1 tag.
- §4.2: "(v1.193 P2 delivered them)"; the "Historical row wording" note; "**Delivered (v1.193 P2).**" prefix on the app-cohorts paragraph; "(v1.195, not yet delivered)" tag on the current-target paragraph.
- §5 table columns "M1 (shipped)" / "M2 product obligation" retitled; row cells' RFT/v1.19x delivery tags.
- §6: "Cancel proof in M1" row label; "stay deferred for M2 (RFT-11 / retained P5; the v1.195 current reading is in §7.5.1)" reworded.
- §7 rule heading "(v1.193 overlay)"; §7.2 "**Delivered (v1.193 P2):**" prefixes.
- §7.5.1 heading "(v1.195)" tag; "v1.195 P3 exercises them as …" past-program framing reworded to present-tense plan-receipt statements.
- §8: "The integrated Rust host is historical and was deleted in v1.193 P2."; §8.1 "**Delivered (v1.193 P2):**" prefix; "Ordinary service/development startup switches to the TS host in M2" reworded.
- §10: "that work completed with the v1.193 P2 cutover (the obsolete daemon/SPA composition and its crate are deleted) and the native composition was retired in v1.192. P5 first-run remains RFT-11." reworded.
- §11: "**v1.192 amendment (2026-09-19):**" prefix; "Desktop floor (resolved 2026-09-13)" reworded; v1.192/RFT-09/M1-GO narration dropped.
- §12 heading "(delivered in v1.192), native host retired" and the versioned bullets; the historical decision-JSON / matrix-run narration (`34881345823`, 13 pass / 0 fail / 22 missing-or-unobserved).
- §14: "This vertical **shipped in v1.189**." and "M2 extends the same flow …" framing.
- §15: "(user direction, 2026-09-14)" provenance.
- §16: "Historical P5 public first-run stays unshipped on `main`, and the v1.195 P3 example is a plan-scope receipt …" bullet reworded to the present-tense authorization state.
- §17: "the deleted composition's Master (`daemon-runtime.md`) is history" clause and the "**Delivered (v1.193 P2):** …" closing paragraph reworded.

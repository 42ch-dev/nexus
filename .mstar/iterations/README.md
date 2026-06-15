# Iteration Specs

Iteration-level specifications for Nexus delivery tracks.

This directory holds all iteration-scoped specs, including:

- `*-delivery-compass-*.md` — version delivery compasses (scope, milestones, acceptance, risk)
- Legacy `v1.*` compass artifacts (overview, matrix, program notes) with non-standard names

Implementation-detail SSOT that is not iteration-scoped stays in [`.mstar/knowledge/`](../knowledge/README.md).

## Index

### Delivery compasses

| Document | Version | Status |
| --- | --- | --- |
| [v1.2-delivery-compass-v1.md](v1.2-delivery-compass-v1.md) | V1.2 | Historical |
| [v1.3-delivery-compass-v1.md](v1.3-delivery-compass-v1.md) | V1.3 | Historical |
| [v1.4-delivery-compass-v1.md](v1.4-delivery-compass-v1.md) | V1.4 | Historical |
| [v1.5-nexus-delivery-compass-v1.md](v1.5-nexus-delivery-compass-v1.md) | V1.5 | Historical |
| [v1.6-delivery-compass-v1.md](v1.6-delivery-compass-v1.md) | V1.6 | Historical |
| [v1.7-delivery-compass-v1.md](v1.7-delivery-compass-v1.md) | V1.7 | Historical |
| [v1.8-delivery-compass-v1.md](v1.8-delivery-compass-v1.md) | V1.8 | Historical |
| [v1.9-delivery-compass-v1.md](v1.9-delivery-compass-v1.md) | V1.9 | Historical |
| [v1.10-delivery-compass-v1.md](v1.10-delivery-compass-v1.md) | V1.10 | Historical |
| [v1.11-delivery-compass-v1.md](v1.11-delivery-compass-v1.md) | V1.11 | Historical |
| [v1.12-delivery-compass-v1.md](v1.12-delivery-compass-v1.md) | V1.12 | Historical |
| [v1.13-delivery-compass-v1.md](v1.13-delivery-compass-v1.md) | V1.13 | Historical |
| [v1.14-delivery-compass-v1.md](v1.14-delivery-compass-v1.md) | V1.14 | Historical |
| [v1.15-delivery-compass-v1.md](v1.15-delivery-compass-v1.md) | V1.15 | Historical |
| [v1.16-delivery-compass-v1.md](v1.16-delivery-compass-v1.md) | V1.16 | Historical (Shipped) — command IA big-bang |
| [v1.18-delivery-compass-v1.md](v1.18-delivery-compass-v1.md) | V1.18 | Shipped |
| [v1.19-delivery-compass-v1.md](v1.19-delivery-compass-v1.md) | V1.19 | Shipped |
| [v1.20-delivery-compass-v1.md](v1.20-delivery-compass-v1.md) | V1.20 | **Shipped** (2026-05-19) |
| [v1.21-local-platform-isolation-delivery-compass-v1.md](v1.21-local-platform-isolation-delivery-compass-v1.md) | V1.21 | **Shipped** (2026-05-21) — plan archived |
| [v1.22-cli-deprecation-cleanup-delivery-compass-v1.md](v1.22-cli-deprecation-cleanup-delivery-compass-v1.md) | V1.22 | **Shipped** (2026-05-21) |
| [v1.24-knowledge-crates-alignment-audit-compass-v1.md](v1.24-knowledge-crates-alignment-audit-compass-v1.md) | V1.24 audit | **Shipped** (2026-05-22) — doc refresh + KCA-002/003 |
| [v1.25-knowledge-crates-product-wiring-audit-compass-v1.md](v1.25-knowledge-crates-product-wiring-audit-compass-v1.md) | V1.25 audit | **Shipped (partial)** (2026-05-22) — in-memory wiring; superseded by V1.26 |
| [v1.26-local-persistence-delivery-compass-v1.md](v1.26-local-persistence-delivery-compass-v1.md) | V1.26 | **Shipped** (2026-05-23) — local SQLite persistence, reference MD layout, context productization |
| [v1.27-local-authoring-delivery-compass-v1.md](v1.27-local-authoring-delivery-compass-v1.md) | V1.27 | **Shipped** (2026-05-24) — CLI-first local authoring writes, User knowledge SQLite, context closure, API hygiene, `acp agent use` |
| [v1.28-context-and-agent-host-delivery-compass-v1.md](v1.28-context-and-agent-host-delivery-compass-v1.md) | V1.28 | **Shipped** (2026-05-25) — `assemble-moment` SSOT, structured KB query, Agent Host Batch 1 |
| [v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md](v1.29-author-intelligence-and-agent-hardening-delivery-compass-v1.md) | V1.29 | **Shipped** (2026-05-26) — Author Intelligence (FL-A/B), Agent Host Batch 2, spec/tracker hygiene |
| [v1.30-residual-convergence-delivery-compass-v1.md](v1.30-residual-convergence-delivery-compass-v1.md) | V1.30 | **Shipped** (2026-05-26) — residual convergence (R5–R20) |
| [v1.31-agentic-design-patterns-delivery-compass-v1.md](v1.31-agentic-design-patterns-delivery-compass-v1.md) | V1.31 | **Shipped** (2026-05-30) — FL-D partial close: orchestration de-stub + 2 Agentic Design Pattern presets |
| [v1.32-preset-quality-gate-delivery-compass-v1.md](v1.32-preset-quality-gate-delivery-compass-v1.md) | V1.32 | **Shipped** (2026-06-03) — preset validator quality gate + `SEC-V131-01` |
| [v1.33-work-experience-loop-delivery-compass-v1.md](v1.33-work-experience-loop-delivery-compass-v1.md) | V1.33 | **Shipped** (2026-06-04) — Work container, Creative Brief Intake, `creator run`, `llm_judge` fix, memory review loop; 5 plans P1–P5 Done |
| [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) | V1.34 | **Shipped** (2026-06-05) — FL-E stage workflow, Agent `nexus.*` tool bridge (8 tools); 6 plans P0–P5 Done (P0 residual convergence, P1 stage model, P3 spec, P2 preset chain, P4 HostToolExecutor, P5 hygiene); DF-47 carry-forward to V1.35 |
| [v1.35-cli-ia-and-product-polish-delivery-compass-v1.md](v1.35-cli-ia-and-product-polish-delivery-compass-v1.md) | V1.35 | **Shipped** (2026-06-07) — CLI IA (5 groups; sync → platform sync deprecated), creator hub polish (KB disambiguation + tier ordering), critical residual P0 (6 criticals + R-CURSOR-PR42-03 + 5 backlog), FL-E UX polish (chain default true); 5 implement plans P0/P2/P3/P4/P5 + prepare P-1 + P1 docs all Done; DF-47 carry-forward to V1.36 (conditional) |
| [v1.36-novel-writing-ux-delivery-compass-v1.md](v1.36-novel-writing-ux-delivery-compass-v1.md) | V1.36 | **Shipped** (2026-06-07) — novel-writing正文产出 UX; `work_profile: novel`; `Works/<work_ref>/` layout; `novel-project-init` grill-me preset + scaffold protocol; `work_chapters` DB SSOT; chapter pipeline with `llm_judge` 五问 quality gate; completion stop; DF-57/58 closed; DF-53 partial (layout-aware); DF-47 conditional (not P0); DF-59 backlog; 5 implement plans P0–P4 + prepare P-1 all Done; PM-validate path used for P1–P4 under time pressure |
| [v1.37-novel-writing-foundation-delivery-compass-v1.md](v1.37-novel-writing-foundation-delivery-compass-v1.md) | V1.37 | **Shipped** (2026-06-08) — Novel Writing UX foundation-first; P0 implemented (gate_evaluator, AddScheduleRequest.input, scaffold atomicity, --force-gates audit, novel-writing/preset.yaml §5.3.2 gate set); P1/P2/P3 spec/roadmap amendments (DF-62 multi-chapter, DF-63 World KB, DF-64/65/66/67 quality loop); R-V136P1-01/02, R-V136P3-02 closed; R-V137P0-01 (serde strict-mode) opened; 5 plans P-1 + P0 + P1 + P2 + P3 all Done |
| [v1.38-multi-chapter-serial-writing-delivery-compass-v1.md](v1.38-multi-chapter-serial-writing-delivery-compass-v1.md) | V1.38 | **Shipped** (2026-06-09) — DF-62 multi-chapter / serial writing first implementation slice; P0 chapter selection/status foundation (`next_chapter(work_id)` single MIN query, `current_chapter` finalize-only advance, completion with intake + current_chapter checks, per-chapter CLI status UX, composite index); P1 `novel-writing` selected-chapter parameterization (preset version5→6, `chapter_label` / `outline_path` / `body_path` / `slug` template vars, fail-fast CLI validation, shared `chapter_label()` helper); DF-53 auto-chain, DF-63 World KB, DF-64/65/66/67 quality loop, multi-volume PK migration, platform publish, multi-work switch, and selection pool remain deferred; 12 P0+P1 residuals registered (medium=2, low=5, nit=5) |
| [v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md](v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md) | V1.39 | **Shipped** (2026-06-09 via PR #50, merge ad9725d8) — DF-53 full FL-E auto-chain (default true) + chapter outer loop + DF-68 daemon continuation checkpoint; side-input lane (inspiration + research KB without forking chain); DF-64/65/66/67 quality-loop full implement (findings, `novel-brainstorm` / `novel-review-master`, rules, logs, 96h banner); P0..P5 + prepare P-1 all Done on `iteration/v1.39`; PR #50 cursor security review finding (preset-gate authorization bypass in P0.5 C-1 fix) closed via fix/v1.39-preset-gate-bypass (commit 3cc1601f) before merge; 22 V1.39 residuals registered (3 medium + 19 low); DF-63 World KB + multi-volume PK deferred to V1.40 |
| [v1.40-novel-world-kb-delivery-compass-v1.md](v1.40-novel-world-kb-delivery-compass-v1.md) | V1.40 | **Shipped** (2026-06-11 via PR #52 merged to main) — DF-63 World KB product closure; integration branch `iteration/v1.40` retired post-PR |
| [v1.41-multi-work-author-desk-delivery-compass-v1.md](v1.41-multi-work-author-desk-delivery-compass-v1.md) | V1.41 | **Shipped** (2026-06-11) — PR [#53](https://github.com/42ch-dev/nexus/pull/53); DF-60/61 archived; post-merge security fixes |
| [v1.42-multi-volume-serial-writing-delivery-compass-v1.md](v1.42-multi-volume-serial-writing-delivery-compass-v1.md) | V1.42 | **Shipped** (2026-06-12) — P0 runtime_lock + P1 DF-62 multi-volume + P2 DF-56 + P3 DF-47 + P-last UX; `iteration/v1.42` |
| [v1.43-novel-author-experience-delivery-compass-v1.md](v1.43-novel-author-experience-delivery-compass-v1.md) | V1.43 | **Shipped** (2026-06-12) — BL-10 author quickstart (ongoing serial) + CLI copy + author visibility + P-last hygiene; `iteration/v1.43` |
| [v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md](v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md) | V1.44 | **Shipped** (2026-06-13) — DF-69 P0 + review-master CLI P1 + multi-volume P2 + author-desk P3 + P-last hygiene; PR [#57](https://github.com/42ch-dev/nexus/pull/57) merged `76a9eb79` |
| [v1.45-creator-run-preset-unification-delivery-compass-v1.md](v1.45-creator-run-preset-unification-delivery-compass-v1.md) | V1.45 | **Shipped** (2026-06-14, P-last closeout) — CLI IA: `creator run <preset_id>` + `creator bootstrap` + atomic `creator works`; `iteration/v1.45` |
| [v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md](v1.46-novel-author-maturity-and-spec-hygiene-delivery-compass-v1.md) | V1.46 | **Shipped** (2026-06-15, P-last closeout) — author desk delta (`creator works status --json` + per-finding remediation; novel-only gate; findings_stale) + spec sweep (BL-10 quickstart retired → embedded §3 of [novel-author-experience.md](../knowledge/specs/novel-author-experience.md) promoted Draft → Shipped; 12 spec amendments; cli-spec §6.2E deleted) + runtime edges (on-disk chapter hints with cap+tracing; dynamic clap `cli_args` help for 3 first-slice presets) + hermetic supervisor research E2E (5 tests) + pool/inspiration mutation tracing (9 paths); PR [#59](https://github.com/42ch-dev/nexus/pull/59) merged to `main`; 6 plans P-1 + P0–P4 + P-last all Done |
| [v1.47-novel-quality-loop-closure-delivery-compass-v1.md](v1.47-novel-quality-loop-closure-delivery-compass-v1.md) | V1.47 | **Active** (2026-06-15, harness prepare) — quality loop closure: reflection-loop → findings + remediation audit + §4.5.7 tests #1–#3 + spec reconcile; `iteration/v1.47`; P-1 Done, P0–P-last Todo; wave-0 [novel-quality-loop.md](../knowledge/specs/novel-quality-loop.md) Draft V1.47 |

### Reference compasses

| Document | Version | Status |
| --- | --- | --- |
| [v1.23-architecture-crate-wiring-reference-compass-v1.md](v1.23-architecture-crate-wiring-reference-compass-v1.md) | V1.23 reference | Reference — non-binding; Cargo target largely met; see V1.24/V1.25 audits for product gaps |

### Legacy `v1.*` iteration artifacts

| Document | Version | Description |
| --- | --- | --- |
| [v1.1-overview-v2.md](v1.1-overview-v2.md) | V1.1 | Program overview snapshot (status-aligned). |
| [v1.2-reclassification-matrix-v1.md](v1.2-reclassification-matrix-v1.md) | V1.2 | Cross-version reclassification matrix. |

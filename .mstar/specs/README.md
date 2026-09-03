[.mstar/specs/README.md#D3E0]
1:# Specs
2:
3:Functional and normative specifications for the Nexus OSS repo.
4:
5:**Rules (invariants):** [AGENTS.md](AGENTS.md)  
6:**Not here:** schema-boundary policy → [knowledge root](../knowledge/README.md)
7:
8:---
9:
10:## Global narrative (first principles)
11:
12:Nexus OSS specs describe a **local-first creative runtime** with optional cloud mount:
13:
14:```text
15:Identity & scope          →  who owns data (Creator, User, World, …)
16:Architecture & contracts  →  which crate owns which concern; wire vs local types
17:Runtime topology          →  CLI → daemon → Daemon API → ACP workers
18:Persistence               →  state.db, reference store, workspace layout
19:Orchestration             →  presets, capabilities, schedules, sessions
20:Product surface (CLI)     →  command IA, entry paths, per-flag behavior
21:Product lines             →  shipped journeys (Work, FL-E, agent tools, …)
22:Exploration               →  future engine/product lines without implement authority
23:```
24:
25:**Why flat files:** each layer exposes a few long-lived **Master** documents agents can cite by stable basename. Iteration velocity is handled by **Draft overlays**, not by renaming or sharding directories.
26:
27:**Why not one mega-spec:** CLI command detail, orchestration grammar, and ACP hosting evolve on different cadences; Feature line specs record shipped product contracts without bloating Masters.
28:
29:**Discovery:** this README is the only maintained index. After adding or retiring a spec, update the tables below — do not duplicate the list in AGENTS.md.
30:
31:**Three pillars (V1.122 canonized):** Nexus OSS specs describe a product built on three pillars — **Harness** (control strategy / orchestration / agent host / capability registry / presets; UI still reads "Strategy/Preset"), **Canvas** (spatial steering surface, with **Timeline-centric World building** as the hero World-entry surface), and **Computable** (the WASM layer that makes worlds react). Pillar definitions live in repo-root [`STRATEGY.md`](../../STRATEGY.md) + [`CONCEPTS.md`](../../CONCEPTS.md). Specs carry a `Pillar (V1.122)` header cross-reference where applicable (e.g. `orchestration-engine.md` → Harness; `compute-module-abi.md` + `wasm-host.md` → Computable; `canvas-strategy-surface.md` + `web-ui.md` → Ca…
32:
33:---
34:
35:## Document classes
36:
37:| Class | Implement authority | Typical header `Status` |
38:| --- | --- | --- |
39:| Master | When normative / active | Normative, Active, Accepted |
40:| Draft overlay | While Status is Draft | Draft (Vx.xx) |
41:| Feature line | Yes | Shipped (Vx.xx) |
42:| Exploration | No | Exploration |
43:| Companion | OSS scope only | Normative (companion) |
44:| Legacy scope | Cited subdomain only | Active (legacy scope) |
45:
46:See [AGENTS.md](AGENTS.md) for create/extend/merge rules.
47:
48:---
49:
50:## Layout
51:
52:Spec files live **flat** in this directory except **`novel-writing/`** — the novel `work_profile` subtree (relocated 2026-06-17). See [novel-writing/README.md](novel-writing/README.md) for the domain index.
53:
54:---
55:
56:## Master index (by domain)
57:
58:*Statuses reflect document headers as of last README maintenance; authoritative per-file header wins on conflict.*
59:
60:### Architecture and boundaries
61:
62:| Document | Class | Status |
63:| --- | --- | --- |
64:| [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) | Master | Active |
65:| [entity-scope-model.md](entity-scope-model.md) | Master | Normative — V1.40 Shipped §5.1.1; V1.51 Shipped §5.5.6; **V1.62 Shipped** §5.5.9 (computable-flag + structured validation). **V1.158**: §1.4 V1.123 three-layer overlay + V1.156 3×2 matrix completion amendment promoted to Normative (World×Moment + Work×Brief closed; frontend-only, `wire_contracts_changed: false`). **V1.159**: §5.1.1 era taxonomy amendment (`era_type` + §5.6 `custom`/`custom_label: "parent_era"` nesting carrier — additive, `wire_contracts_changed: false`). **V1.162**: §6.6 fork-creation write boundary + lineage projection contract amendment (PD-01 local-vs-platform reconciliation; carrier approach B locked — branch-level `is_fork`/`parent_branch_id`/`forked_from_event_id`/`label?` fro…
66:| [local-runtime-boundary.md](local-runtime-boundary.md) | Master | Normative |
67:| [schemas-directory-layout.md](schemas-directory-layout.md) | Master | Normative — V1.64 Shipped (local-api common + findings list-response) |
68:| [local-api-surface-conventions.md](local-api-surface-conventions.md) | Master | Normative — **V1.67 amended** (§3.2 casing ratification, §4 `items` enforcement, §5 sort-param contract; 0.5.0→0.6.0) |
69:| [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md) | Master | Normative — V1.77 amendment (§11 findings PATCH as non-OCC resource PATCH); cross-resource Daemon API response/query conventions for `schemas/daemon-api/` + `nexus-daemon-runtime` handlers |
| [outbox-consolidation.md](outbox-consolidation.md) | Master | Normative — V1.59 P-last promote (single-writer contract + schema ownership); **V1.177 revision** (daemon `outbox` table dropped at V1.163 — §2.3/§6 closed history) |
70:| [reference-knowledge.md](reference-knowledge.md) | Master | Normative — V1.58 P-last promote (reference body refreshable scan pipeline) |
71:| [spoke-adapter-architecture.md](spoke-adapter-architecture.md) | Master | **Normative (v0.19 — V1.155 P1 capability-token production + tenant isolation: `nexus42 connect token issue` CLI (issuer.key Ed25519 create-once 0600, `claims.iss` MUST equal issuer-derived peer id), operator config `~/.nexus42/connect/config.json` (`trusted_issuers` / `require_capability_token` / `capability_token_provider{enabled, issuer_key_path}`, deny-unknown-fields, absent ⇒ pre-V1.155 defaults, malformed ⇒ fail-closed boot error, require-without-issuers ⇒ boot error); enforcement spoke-side fail-closed (`evaluate_invoke_token_gate` ⇒ `auth_failed` before the nexus handler, zero side effects) + nexus `PeerScope` intersection — token can never widen allowlist scope; all opt-in, …
72:| [schemas-external-consumer-boundary.md](schemas-external-consumer-boundary.md) | Master | Normative — external-consumer boundary rule for `schemas/` (platform wire + external Local API clients incl. bundled web UI); moved from knowledge root 2026-08-17 |
73:| [world-kb-runtime-architecture.md](world-kb-runtime-architecture.md) | Master | Normative — World KB implementation SSOT (crate responsibilities, loops, taxonomy; V1.139 SPOKE alignment); moved from knowledge root 2026-08-17 |
| [embedding-readiness.md](embedding-readiness.md) | Master | Normative — V1.181 P0 (RN-OGA-3 readiness-contract form): platform-provided embeddings, OSS ships no execution; `EmbeddingIdentity` tuple + fail-closed derived-index protocol + explicit lexical fallback; governs `crates/nexus-embedding/` |
74:
75:### Runtime and persistence
76:
77:| Document | Class | Status |
78:| --- | --- | --- |
79:| [daemon-runtime.md](daemon-runtime.md) | Master | Normative — V1.64 amendment (bundled local Web UI static serving); **V1.118 amendment** (§17 no-Profile boot + lazy `state.db` open) |
80:| [local-db-schema.md](local-db-schema.md) | Master | Normative — V1.40 Shipped §4.1.2 (KB validation + narrative_worlds + kb_extract_jobs artifact locator) |
81:| [concurrency.md](concurrency.md) | Master | **Normative — V1.51 Shipped (T-B P0/P1)** — advisory lock + heartbeat + OCC + zombie detection |
82:| [canvas-strategy-surface.md](canvas-strategy-surface.md) | Draft overlay | **Shipped α (V1.70)** — Canvas product vision (Nexus = AI-autonomous executor; human steers via Canvas, AI owns prose) + 3 surfaces (Strategy/outline+timeline/World KB) on React Flow + no-raw-file-editing principle + TipTap-as-in-node + Preset→Strategy terminology. **V1.70** shipped the Strategy read/overlay/Idea-steer α slice; write-boundary + node-granular edits + outline/timeline + World KB remain Draft for V1.71+. **V1.122 Draft overlay (§3.3.2 + §4.5)** — fourth peer surface `CanvasSurfaceKind = "timeline"` (World-building hero) + architect-locked World-building projection + write-boundary reuse + Timeline-as-default-World-entry IA; shipped β text preserved (additive); `wire_co…
83:| [reference-store-layout.md](reference-store-layout.md) | Master | Normative |
84:| [chapter-content-local-api.md](chapter-content-local-api.md) | Draft overlay | Draft (V1.65) — chapter-content Daemon API field contract (`/v1/daemon/works/{work_id}/chapters/*`); cited by daemon-api-surface-conventions §6 |
85:
86:### Compute and WASM
87:
88:| Document | Class | Status |
89:| --- | --- | --- |
90:| [compute-module-abi.md](compute-module-abi.md) | Master | **Normative — V1.62 Shipped (P2)** — V1 envelope ABI: exports, host imports, marshalling, manifest.json contract |
91:| [wasm-host.md](wasm-host.md) | Master | **Normative — V1.62 Shipped (P2)** — nexus-wasm-host crate: engine, sandbox, limits, watchdog, module loading, error taxonomy |
92:
93:### CLI product surface
94:
95:| Document | Class | Status |
96:| --- | --- | --- |
97:| [cli-spec.md](cli-spec.md) | Master | **Normative — V1.51 Shipped** — V1.40 §6.2G world binding + **V1.51** `kb adopt`/`rescan`/`pending --missing-only` (T-A P0/P1/P2); legacy V1.46 overlay fully merged; V1.52 §6.2G.2/§6.2G.1 overlays promoted (V1.158) |
98:| [cli-command-ia.md](cli-command-ia.md) | Master (Shipped V1.35) | Shipped (V1.35) |
99:| [creator-centric-entry-model.md](creator-centric-entry-model.md) | Master (Shipped V1.35) | Shipped (V1.35) |
100:
101:**Read order:** CLI Master (§6–§7) → shipped IA supplement → shipped entry-model supplement.
102:
103:### Orchestration and presets
104:
105:| Document | Class | Status |
106:| --- | --- | --- |
107:| [orchestration-engine.md](orchestration-engine.md) | Master | Active; **V1.62 Shipped** §5.2 narrative.compute + §8.4 combat-engine |
108:| [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) | Legacy scope | Active (WS7 schedule/core_context) |
109:| [preset-conditional-routing.md](preset-conditional-routing.md) | Feature line | **Shipped (V1.42 P2)** — DF-56 `llm_judge` GO/NOGO minimal slice; V1.52/V1.56 overlays promoted (V1.158) |
110:| [llm-extract.md](llm-extract.md) | Master | **Normative — V1.51 Shipped (T-A P0)** — `nexus.llm.extract` capability + `LlmExtractTask` + `kb_extract_jobs` LLM payload extension (closes R-V150KBED-01) |
111:
112:### Creator product lines
113:
114:| Document | Class | Status |
115:| --- | --- | --- |
116:| [work-experience-model.md](work-experience-model.md) | Feature line | Shipped (V1.33) |
117:| [creator-workflow.md](creator-workflow.md) | Feature line | Shipped (V1.34; V1.40 Shipped — DF-63 W5 `novel-review-master sync_world_kb` extract binding) |
118:| **[novel-writing/](novel-writing/README.md)** | Feature subtree | **`work_profile: novel`** — see [novel-writing/README.md](novel-writing/README.md) for per-file index (workflow-profile, quality-loop, author-experience, overlays, …) |
119:| [essay-profile.md](essay-profile.md) | Feature line | Draft (V1.52) — `work_profile: essay` first non-novel profile |
120:| [web-ui.md](web-ui.md) | Feature line | **Shipped (V1.65)** — local Web UI product contract (`apps/web` React/Vite SPA, daemon-served, Tauri-ready); Control Room + Setup (V1.64) + Content-Authoring UI stage (V1.65 §13) + Desktop Shell stage (V1.66 §14, Shipped) + Surface Convergence & De-risk stage (V1.67 §15, Shipped) + V1.69 Design System Maturation & Canvas Draft + **V1.70 Canvas Strategy Implement (α) stage (V1.70 §16, Shipped)** + CI/desktop-build optimization (parallel ops track). **V1.118 Draft amendment** (§29.17 Creation peer groups + Canvas-first work shell). **V1.156 PD-4**: §29.2/§29.3/§29.4 Harness pillar-entry rename (user-visible Strategy/Strategies → Harness; Preset stays; internal identifiers unchanged). |
121:| [design-studio.md](design-studio.md) | Feature line | Draft (V1.98) with V1.99–V1.101 studio-first amendments — standalone contributor/dev gallery and visual proving ground; not author-facing product UI |
122:| [desktop-shell.md](desktop-shell.md) | Feature line | **Shipped (V1.66)** — Tauri v2 desktop shell contract (`apps/desktop` wrapper, `TauriClient`, sidecar lifecycle, port discovery, native file actions + path guard); macOS-first unsigned dev build. **V1.118 Draft amendment** (§13.11 Daemon no-Profile boot). |
123:| [creator-run-preset-entry.md](creator-run-preset-entry.md) | Master | **Shipped (V1.45)** — `creator run <preset_id>` generic entry; wave 0 for V1.45 CLI IA (promoted P-last) |
124:| [creator-challenge-solver.md](creator-challenge-solver.md) | Master | Normative |
125:| [creator-memory-soul-lifecycle.md](creator-memory-soul-lifecycle.md) | Draft overlay | Draft (V1.82 amendment) — per-(creator, world) narrative lifecycle |
126:| [reading-chrome-profile-checklist.md](reading-chrome-profile-checklist.md) | Feature line | Shipped (V1.91) — acceptance checklist on DESIGN.md `reading-chrome-*` tokens |
127:| [web-ui-design-requirements.md](web-ui-design-requirements.md) | Companion | Input brief (V1.64/V1.65) for repo-root `DESIGN.md` — product/design intent; sole SSOT since `apps/web/DESIGN*.md` retired (V1.98) |
128:
129:### ACP and agent integration
130:
131:| Document | Class | Status |
132:| --- | --- | --- |
133:| [acp-client-tech-spec.md](acp-client-tech-spec.md) | Master | Accepted |
134:| [acp-capability-set.md](acp-capability-set.md) | Master | Normative |
135:| [agent-host.md](agent-host.md) | Master | Normative |
136:| [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) | Feature line | Shipped (V1.34) |
137:| [capability-registry.md](capability-registry.md) | Draft overlay | Draft (V1.53) — runtime SSOT framework for `nexus.*` dispatch |
138:| [registry-integration.md](registry-integration.md) | Master | Normative |
139:
140:### Feature contracts and companions
141:
142:| Document | Class | Status |
143:| --- | --- | --- |
144:| [canonical-hash.md](canonical-hash.md) | Companion | Normative (OSS notes; platform ADR-006 authoritative) |
145:| [world-delta-propose-apply.md](world-delta-propose-apply.md) | Feature line | Normative — V1.60 P-last promotion (world-delta propose/apply local parity) |
146:
147:*Novel-writing sync module contract: [novel-writing/sync-contract.md](novel-writing/sync-contract.md).*
148:
149:---
150:
151:## Normative hierarchy (conflict resolution)
152:
153:When specs disagree, higher row wins:
154:
155:1. Repo root **AGENTS.md**
156:2. Architecture Masters (crate graph, entity scope)
157:3. **Draft overlay** over a conflicting legacy Master section until merge
158:4. Domain **Master**
159:5. Shipped supplement / retained overlay for rationale and acceptance details after Master merge
160:6. **Feature line** spec
161:7. **Exploration** (non-binding)
162:
163:---
164:
165:## Authority matrix (overlapping topics)
166:
167:| Topic | Primary SSOT | Secondary |
168:| --- | --- | --- |
169:| Top-level CLI groups | cli-spec §6.0B | cli-command-ia (Shipped V1.35 supplement) |
170:| First-run / local vs platform | cli-spec §7 | creator-centric-entry-model (Shipped V1.35 supplement) |
171:| Work / `creator run` | [creator-run-preset-entry.md](creator-run-preset-entry.md) (V1.45 Draft) | work-experience-model, cli-spec §6.2 |
172:| Novel profile / `Works/<work_ref>/` layout | [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) | work-experience-model, [novel-writing/sync-contract.md](novel-writing/sync-contract.md), cli-spec §12.1 |
173:| Creator workflow stages / chain | creator-workflow | work-experience-model, novel-writing/workflow-profile (produce) |
174:| Preset YAML / loader / validator | orchestration-engine | creator-schedule § YAML additions |
175:| Schedule / core_context | creator-schedule-and-core-context | orchestration-engine sessions |
176:| On-demand chapter audit (DF-69) | [novel-writing/manuscript-audit.md](novel-writing/manuscript-audit.md) | novel-writing/quality-loop §3, cli-spec §6.2 |
177:| Agent `nexus.*` tools | agent-nexus-tool-bridge | acp-capability-set, agent-host |
178:| ACP worker process | acp-client-tech-spec | daemon-runtime, local-runtime-boundary |
179:| KB naming (KCA-003) | entity-scope-model §5.4 + cli-command-ia §3.2 | cli-spec §6.2E–F |
180:| LLM extraction capability | [llm-extract.md](llm-extract.md) | entity-scope-model §5.5.6, world-kb-runtime-architecture §5.5, cli-spec §6.2G |
181:| Compute module ABI (V1 envelope) | [compute-module-abi.md](compute-module-abi.md) | wasm-host, schemas-directory-layout §3.5, orchestration-engine §8.4, entity-scope-model §5.5.9, `schemas/local-api/compute/` |
182:| WASM compute host runtime | [wasm-host.md](wasm-host.md) | compute-module-abi, orchestration-engine §8.4, `crates/nexus-wasm-host/AGENTS.md` |
183:
184:---
185:
186:## Hygiene schedule (consolidation policy)
187:
188:| Trigger | Required action | Status |
189:| --- | --- | --- |
190:| **Post-V1.35 CLI changes** | Update cli-spec §6–§7 first; update shipped supplements only when rationale, acceptance, or migration history changes | V1.36-V1.40 amendments folded into Master (no follow-up merge needed yet) |
191:| **V1.53 ACP capability registry hygiene** | Promote or retain `capability-registry.md` after P0/P1 registry semantics land; skills-export compatibility spec retired and DF-50 Cancelled | Active V1.53 |
192:| **Novel-writing sync module removed from code** | Archive novel-writing-sync-contract | Module still shipped (V1.36+); sync contract retained |
193:| **V1.40 shipped (DF-63 closed)** | Mark `entity-scope-model.md` §5.1.1 + `cli-spec.md` §6.2G + `creator-workflow.md` persist + `local-db-schema.md` §4.1.2 + `novel-writing/workflow-profile.md` §3.5.1 as Shipped V1.40 in their headers | **Done 2026-06-11** (see headers + this index) |
194:
195:**Retained splits (do not merge):** creator-schedule-and-core-context (schedule domain); ACP cluster (independent evolution cadence).
196:
197:---
198:
199:## Platform cross-repo references
200:
201:Cite **`nexus-platform`** `v1-spec/` for cloud product, shared ADRs, and architecture umbrella. Wire JSON in this repo: `schemas/` → `nexus-contracts`.
202:
203:| Need | Platform path |
204:| --- | --- |
205:| Architecture umbrella | `v1-spec/architecture.md` |
206:| ADR | `v1-spec/adr/{name}.md` |
207:| Shared contracts | `v1-spec/shared/...` |
208:| Platform HTTP / product | `v1-spec/platform/...` |
209:
210:---
211:
212:## Archived superseded specs
213:
214:| Former spec | Superseded by |
215:| --- | --- |
216:| `daemon-api-workspace-write-architecture.md` | Stale — historical |
217:| `local-fs-layout-creator-workspace.md` | Retired |
218:| `nexus42-single-binary-daemon-runtime-architecture.md` | [daemon-runtime.md](daemon-runtime.md) |
219:| `agent-host-architecture.md` | [agent-host.md](agent-host.md) §8 |
220:| `fl-d-conditional-routing-exploration-v1.35-prepare.md` | [preset-conditional-routing.md](preset-conditional-routing.md) |
221:| `novel-findings-maturity.md` | [novel-writing/quality-loop.md](novel-writing/quality-loop.md) §9 |
222:| `body-editor.md` | [canvas-strategy-surface.md](canvas-strategy-surface.md) (2026-06-26 — body-editor direction rejected) |
223:| `non-novel-profiles-roadmap.md` | [game-bible-profile.md](game-bible-profile.md) + [script-profile.md](script-profile.md) + [essay-profile.md](essay-profile.md) (all targets shipped) |
224:| `findings-lifecycle.md` | [novel-writing/quality-loop.md](novel-writing/quality-loop.md) §2 |
225:| `narrative-indexes.md` | [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) §4.6 |
226:
227:**Former filename:** `local-platform-isolation-and-crate-architecture.md` → `local-cloud-crate-architecture.md` (2026-05-20).
228:
229:---
230:
231:## Maintaining this index
232:
233:When adding, renaming, or archiving a spec:
234:
235:1. Set header **`Status`**, **`Document class`**, and **`Coordinates with`** in the spec file.
236:2. Update the domain table in this README.
237:3. Update this README index when specs are added, retired, or promoted.
238:4. Do **not** add file lists to AGENTS.md.

[Some lines truncated to 768 chars]
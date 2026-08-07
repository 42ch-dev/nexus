# React TRPG Turn Strategies Sample

A forkable orchestration sample for a turn-based TRPG loop: the partner's
backend orchestrates Nexus, Spoke carries the protocol, the host-local rules
module settles deterministically, and the AI understands intent and narrates.

**Division of labor.** Nexus orchestrates; Spoke carries protocol; the
host-local rules module settles; the AI understands intent and narrates.
Attack/check/damage/resource/status/spell-slot results are computed and
committed ONLY by the local rules module. The AI may request an operation but
never computes, rewrites, or overrides settlement results.

The sample covers the three trigger types of the partner's turn contract as
**two preset lanes** (mechanical-op, natural-language-turn) **plus one README
contract** (browse guard). It is orchestration-level: it declares capability
routing + prompt templates and references the settlement op; it does not
embed or ship a rules module.

## Bundle layout

```
react-trpg-turn/
├── preset.yaml            # one bundle, two named lanes (routed by trigger_type)
├── templates/
│   ├── lane-route.md               # lane selector judge (GO = mechanical, NOGO = natural language)
│   ├── mechanical-op-request.md    # op-request contract — explicit client action
│   ├── natural-language-intent.md  # intent parse — derived structure, raw input preserved
│   ├── natural-language-op-request.md  # op-request contract — propose, never pre-announce
│   ├── settle-receipt.md           # receipt acceptance — sole mechanical source, no recompute
│   └── receipt-narration.md        # narrate confirmed results only, stop at player response
└── README.md              # this file: browse guard + idempotency/completion contract
```

## Run payload (`preset.input`)

| Key | Meaning |
|---|---|
| `trigger_type` | `"mechanical"` or `"natural_language"` — selects the lane |
| `turnId` | unique turn identifier, client-generated, one per turn |
| `operationId` | stable rule-op identifier (client-supplied in the mechanical lane; AI-proposed in the natural-language lane) |
| `input` | raw player input, preserved verbatim in the ledger — never overwritten by AI/parser rewrites |
| `params` | operation parameters |
| `state` | current public game state (context assembly; not mutated by the preset) |
| `receipt` | confirmed structured receipt from the local rules module, injected by the caller after settlement |

## Browse guard (trigger type 1) — README contract

Pure-UI operations — viewing a character sheet, switching pages, expanding a
spell, opening inventory — are **not** a preset lane. The contract:

- **No AI call** — UI operations never invoke a prompt template or an LLM.
- **No world-time advance** — the game clock and timeline stay untouched.
- **No state mutation** — no op is settled, no entry is written, no resource
  or status changes.

The client handles these locally and routes only mechanical or
natural-language actions into the preset lanes.

## The two preset lanes

### Mechanical-op lane (trigger type 2)

Explicit mechanical action (attack, cast, shield block, check shortcut):

1. The client submits a stable `operationId` + `params`.
2. The local rules module settles FIRST — the host invokes the E2 `compute`
   op over Connect (host-local WASM module; see
   [Settlement](#settlement-host-local-module-over-connect)).
3. The AI narrates from the confirmed receipt ONLY
   (`templates/settle-receipt.md` → `templates/receipt-narration.md`).
   Narration is optional: when UI feedback already suffices, the client may
   skip the narration node and render the receipt directly.
4. The AI never recalculates or overrides settlement.

### Natural-language-turn lane (trigger type 3)

Natural-language action ("I cast phase bolt at the guard"):

1. `templates/natural-language-intent.md` parses the player's intent; the raw
   input stays unchanged in the ledger.
2. `templates/natural-language-op-request.md` PROPOSES an op request + params
   — never pre-announcing outcomes (no success/failure/status claims).
3. The local rules module settles (E2 `compute` op over Connect).
4. The AI narrates the confirmed receipt only, then **stops at the player
   response point** — the outer state machine parks at `wait_for_player`
   (`ExitWhen::Manual` → `NextAction::WaitForInput`); no automatic transition,
   extra op, or outcome narration before the player resumes.
5. Chained dependent ops resolve **stepwise**: each graph run proposes/settles
   one op; the next step is requested in a later turn based on the confirmed
   result. The full chain is never guessed in one shot.
6. When settlement is not needed, the op request declares
   `needs_settlement: false` and narration proceeds with no mechanical claims.

## Turn idempotency and completion contract

**Idempotency.** Each turn carries a unique `turnId`; each rule op carries a
unique `operationId`. Refresh, reconnect, timeout-retry, or duplicate
responses must never double-settle (no double cost, no double damage, no
duplicate writes). The preset/loader guarantees the schema, graph shape, and
referenced assets; the guarantees below belong to the **client/runtime
boundary**, because no preset symbol can prove uniqueness, deduplicate
retries, atomically commit, or unlock the client:

- The caller generates stable, non-empty `turnId` (per turn) and
  `operationId` (per rule op) values.
- The caller persists the ledger and rejects duplicate `(turnId, operationId)`
  settlement (re-applying a confirmed receipt is a no-op; a retry reuses the
  same ids and settles at most once).
- The caller applies world-aware CAS / structured-failure rules when
  committing state (`stored_revision_stale` / `revision_conflict` are
  handled by re-read + retry, never by forced overwrite).
- The request templates carry `{{preset.input.turnId}}` and
  `{{preset.input.operationId}}`; the receipt-confirmation template matches
  the receipt's operation reference to the requested `operationId`.
- **Invalid op path:** an invalid op, bad parameter, or tool failure leaves
  local state UNCHANGED and returns a structured failure receipt
  (`valid: false` + reason), which the AI expresses naturally to the player.

**Completion.** A turn is complete only when ALL of the following hold:

1. **No pending op requests** — every proposed op has settled (or was
   declared unnecessary).
2. **State committed** — the module's confirmed state delta is persisted.
3. **Final narration done or declared unneeded** — the AI returned the final
   narration, or the client explicitly skipped narration (UI feedback
   suffices).
4. **Ledger complete** — the ledger holds the raw player input, the op
   receipts, and the final output (narration or the receipt itself).
5. **Client input lock released** — the player may act again.

## Distillation mapping

The sample distills the structure of the partner's turn contract
(`NEXUS-SPOKE-STRATEGY-SAMPLE.md`, React TRPG core turn strategy v0.1).
Principles are encoded here; prose is not copied.

| Partner-doc section | Sample artifact |
|---|---|
| 目标 (goals: Nexus orchestrates, Spoke carries protocol, local rules module settles, AI narrates) | README "Division of labor" + "Settlement" |
| 触发类型 1 — 纯界面操作 (pure UI ops) | README "Browse guard" (contract, not a preset) |
| 触发类型 2 — 明确的机械操作 (explicit mechanical ops) | `preset.yaml` mechanical-op lane + `templates/mechanical-op-request.md` + `templates/settle-receipt.md` + `templates/receipt-narration.md` |
| 触发类型 3 — 自然语言行动 (natural-language actions) | `preset.yaml` natural-language-turn lane + `templates/natural-language-intent.md` + `templates/natural-language-op-request.md` + `templates/settle-receipt.md` + `templates/receipt-narration.md` |
| 核心回合流程 1 — 保存原始输入 (raw input preserved) | `templates/natural-language-intent.md` (ledger preservation) + README run payload `input` |
| 核心回合流程 2 — 组装本轮上下文 (context assembly) | Run payload `state` (caller-assembled context) |
| 核心回合流程 3 — AI 判断是否需要结算 (settlement decision) | `templates/natural-language-intent.md` `needs_settlement` + `templates/natural-language-op-request.md` no-settlement path |
| 核心回合流程 4 — 本地结算并提交状态 (local settle + commit) | Settlement step (E2 `compute` op over Connect) + README "Turn idempotency and completion" (state committed) |
| 核心回合流程 5 — AI 根据回执续写 (narrate from receipt) | `templates/settle-receipt.md` + `templates/receipt-narration.md` (confirmed results only) |
| 核心回合流程 6 — 按依赖继续操作 (stepwise dependent ops) | `templates/natural-language-op-request.md` stepwise contract (one op per graph run) |
| 核心回合流程 7 — 停在玩家回应点 (stop at player response) | `preset.yaml` `wait_for_player` (`ExitWhen::Manual`) + `templates/receipt-narration.md` "Stop at the player response point" |
| 操作无效 / 参数错误 (invalid-op path) | `templates/settle-receipt.md` invalid-op path + README idempotency contract |
| 上下文与 Prompt 分层 (context layering) | Run payload context keys (`state`, `input`, `receipt`) — assembly is the caller's job, not a fixed prompt count |
| 输出职责 — AI 输出 (narration / dialogue / gm) | `templates/receipt-narration.md` response format (separate from the receipt) |
| 输出职责 — 本地运行时输出 (ruling / mechanics / status) | The confirmed receipt (module output, echoed by `templates/settle-receipt.md`) |
| 幂等与回合完成 (idempotency + completion) | README "Turn idempotency and completion contract" |
| 示例 (phase-bolt worked example) | `templates/natural-language-op-request.md` `cast.phase-bolt` example |
| MVP 验收 (acceptance items) | README "Browse guard" + "Turn idempotency and completion" + the two lanes |
| 本阶段不包含 (exclusions) | Not included: PF2R content, prompt counts/copy, token budgets, DM Guide detail, UI art, Godot — structure only |

## Nexus surface mapping

| Flow step | Nexus surface |
|---|---|
| Ledger writes (raw input, receipts, final output persisted by the caller) | N-C1 write ops over Connect (`upsert` / `promote` / `relate` via `@42ch/spoke-connect`) |
| Context assembly (public state, world info, characters, applicable contracts) | N-C2 read half over Connect (`check` / `assemble`) |
| Settlement (rules module invocation) | E2 compute half: the `compute` op over Connect (N-C2 compute) against the host-local module |
| Turn strategy orchestration | This preset bundle (capability routing + prompt templates; validated by `./strategy-samples/validate.sh react-trpg-turn`) |

The caller's Connect peer must be allowlisted for the ops it uses
(`world_scope` / `op_scope` on the host; `module_scope` gates which compute
modules the peer may invoke). Caller identity is the authenticated session
peer — never a payload-carried claim.

## Settlement (host-local module over Connect)

Settlement maps to the E2 `compute` op: the host invokes the Connect compute
half with the proposed `operationId` + params, and the **host-local WASM
module** (installed under `~/.nexus42/modules/<id>/`, e.g.
`modules/basic-combat` as the stand-in for a rules module) deterministically
computes and commits the result, returning the confirmed structured receipt.
**Module bytes are never peer-supplied** — the rules module lives on the
host; peers only request its operations. Presets orchestrate and reference
the op; they do not embed or ship module bytes.

## Validating the sample

```bash
./strategy-samples/validate.sh strategy-samples/react-trpg-turn
```

Runs the real offline validator core (`nexus42 system preset validate
--offline`) — no daemon needed. Exit 0 = clean.

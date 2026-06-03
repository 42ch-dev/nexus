# V1-Spec Wire Schema Sprint — Platform Contract Unblock

> **For agentic workers:** Use **superpowers:subagent-driven-development** or **superpowers:executing-plans** for task execution. Track steps with `- [ ]` checkboxes.

**Goal:** Close the **JSON Schema SSOT gap** in this repository so **nexus-platform** plans **16–21** (and downstream **CLI parity** plans that depend on generated DTOs) are not blocked on missing or hand-wired wire shapes. Deliverables live only in **this repo** (`schemas/`, codegen output, `enum_conversions.rs` when enums change, semver / `schema_version` policy per root `AGENTS.md`).

**Priority:** **P0 — highest** (contract unblock ahead of platform feature execution).

**Architecture:** Follow **v1-spec** contract stack: HTTP resource semantics and field meaning from platform **v1-spec** prose; **authoritative JSON shapes** align with `schema/codegen-strategy-v1.md` **§6 (minimal HTTP freeze set)** and `cli-sync/platform-capability-map-v1.md` **route → wire name** matrix. **No second DTO source** on the platform — types flow **nexus `schemas/` → `pnpm run codegen` → `@42ch/nexus-contracts` / `nexus-contracts` crate**.

**Tech Stack:** JSON Schema (draft per repo tooling), `tooling/codegen`, Rust + TypeScript generated packages, CI `verify-codegen`.

---

## Authoritative design input (read order)

Design prose is **not** vendored in this public tree. Contributors resolve paths via `**.mstar/local-paths.json`** (gitignored) → private **nexus-platform** checkout:


| Layer                                        | Role                                                                                                                                                            |
| -------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **v1-spec `README.md`**                      | Frozen product/API index; states that request/response JSON shapes align with `schema/` + `codegen-strategy-v1.md`                                              |
| `**schema/codegen-strategy-v1.md**` §6       | Minimal HTTP freeze set — **backlog ordering** for which routes must have schemas first                                                                         |
| `**cli-sync/platform-capability-map-v1.md`** | Capability → HTTP → wire DTO names                                                                                                                              |
| **Per-wave specs**                           | e.g. `platform/platform-api-v1.md`, `domain/social-graph-v1.md`, `platform/explore-ai-v1.md`, `platform/notifications-v1.md`, `pre-freeze-spec-log.md` (**W3**) |


**Cross-repo plan map (conceptual, platform `status.json`):** platform **16** (Explore creator profile / W3), **17** (social graph), **18** (memory web read), **19** (explore AI), **20** (notifications), **21** (optional contracts/OpenAPI freeze audit). This sprint **feeds** those rows; it does **not** implement platform routes.

---

## Non-goals

- Implement or change **nexus-platform** API handlers, Prisma, or Zod-only DTOs without mirroring in `schemas/`.
- Replace **v1-spec** prose — schemas must **trace** to spec sections (traceability table in Task A).
- Expand scope to **V1.2** backlog items not referenced by platform **16–20** unless PM explicitly promotes.

---

## Acceptance criteria

- **Coverage matrix** (committed under `.mstar/plans/reports/2026-04-10-v1-spec-wire-schema-sprint/coverage-matrix.md`): each **minimal-freeze-set** route family × **schema file** × **v1-spec anchor** × **platform plan id** (16–20) × status (done / gap / N/A).
- New or updated JSON Schemas under `schemas/` for **all identified gaps** blocking **16–20**, including at minimum the wire envelopes implied by those plans (Explore creators list/detail projections, social graph mutations + feed, memory web read, explore AI Q&A/summary, notifications list/read).
- `pnpm run validate-schemas` + `pnpm run codegen` clean; `git diff --exit-code` on generated dirs **clean** after commit.
- `cargo clippy --all -- -D warnings` + `cargo test -p nexus-contracts` (and any crate touched).
- `pnpm run typecheck` for `packages/nexus-contracts`.
- **Versioning:** bump `**schema_version`** / package versions per root `AGENTS.md` if wire shapes are new or breaking; document downstream expectation (`@42ch/nexus-contracts` consumers).
- `**enum_conversions.rs**` updated if new/changed enum values appear in schemas.

---

## Task group A — Inventory & traceability

- **A1:** Build the **coverage matrix** from `codegen-strategy-v1.md` §6 + `platform-capability-map-v1.md` + platform plans **16–20** task text (explicit “add JSON Schema in nexus” items).
- **A2:** Diff against **current** `schemas/`** inventory — list missing `$id` files and missing exports from `packages/nexus-contracts/src/generated/index.ts` / `crates/nexus-contracts/src/generated/mod.rs`.
- **A3:** Mark **P0** rows that unblock **16** (W3 / Explore creators) first; then **17–20** in program order unless a hard shared dependency forces reorder.

---

## Task group B — Schema authoring (P0 first)

- **B1 (W3 / 16):** Explore **creators** (and related) **list/query + response** schemas — field tiers and omission rules must match v1-spec **§5** / data-model **Creator** projection (no speculative fields; use `$ref` to `common.schema.json` IDs).
- **B2 (17):** Social graph **request/response** bodies (follow, favorite, collections, personalized feed) per `social-graph-v1.md` + `platform-api-v1.md`.
- **B3 (18):** Memory web **read** DTOs (list/detail/filter) — align with existing domain `memory.schema.json` where possible; extend only per v1-spec **§6B** + consistency rules.
- **B4 (19):** Explore AI **request/response** (Q&A, summary, citations envelope) per `explore-ai-v1.md` + boundary from `context-assembly-v1.md`.
- **B5 (20):** Notifications **list + mark-read** response/request shapes per `notifications-v1.md` + `platform-api-v1.md` §7A.

---

## Task group C — Codegen, Rust enum bridge, versioning

- **C1:** Register new schemas in codegen config (if required by `tooling/codegen` pipeline).
- **C2:** Run `pnpm run codegen`; commit **only** generated output from schemas (no hand-edits in `*/generated/`).
- **C3:** Update `crates/nexus-contracts/src/enum_conversions.rs` for any new enums.
- **C4:** Apply **semver / `schema_version` bump** per release policy; if breaking, **major** npm bump note for platform coordination.

---

## Task group D — Verification & handoff

- **D1:** Full CI-parity commands from root `AGENTS.md` (`validate-schemas`, codegen diff, `cargo +nightly fmt`, clippy, TS typecheck).
- **D2:** Short **handoff note** in plan report folder: package version, new exported TS types list, platform `pnpm contracts:link` smoke expectation.
- **D3:** Optional — open or reference **platform** plan **21** audit when drift risk remains (orthogonal).

---

## Verification commands

```bash
pnpm run validate-schemas
pnpm run codegen
git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
cargo +nightly fmt --all -- --check
cargo clippy --all -- -D warnings
cargo test -p nexus-contracts
pnpm run typecheck
```

---

## Downstream (nexus OSS)

- **`2026-04-10-cli-explore-read-parity`** is registered in `status.json` with **`dependency`: this plan** (`blocks_plans` on this row). Fork/world snapshot parity is **Done** and remains a completed prerequisite for Explore ordering; the **active** gate for CLI Explore is **wire types** from this sprint.

---

## Working branch

`feature/v1-spec-wire-schema-sprint` (from `main` unless PM directs otherwise).

---

*Plan id: `2026-04-10-v1-spec-wire-schema-sprint` · Created: 2026-04-10*
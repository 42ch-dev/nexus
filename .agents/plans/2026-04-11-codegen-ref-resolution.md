# Codegen $ref resolution (common IDs + whole-schema refs)

**plan_id:** `2026-04-11-codegen-ref-resolution`  
**Priority:** Medium — addresses open residuals **PUBLISH-CODEGEN-01** and **FORK-SNAP-01** (not the long-lived `v1-tech-debt-cleanup` batch).

## Goal

- Rust: stop emitting `serde_json::Value` for (a) common `common.schema.json` string-pattern definitions not listed in the manual switch (e.g. `ManuscriptId`, `StoryManifestId`) and (b) whole-schema `$ref` URIs (e.g. `fork-branch.schema.json` → `ForkBranch`).
- TypeScript: emit proper cross-module types for whole-schema refs (not `string` / `unknown`).

## Acceptance

- [x] `pnpm run codegen` updates `packages/nexus-contracts` and `crates/nexus-contracts` generated output with no manual edits under `*/generated/`.
- [x] `cargo clippy --all -- -D warnings`, `pnpm run typecheck`, and targeted `cargo test` (lib + affected integration tests) pass; full `cargo test --all` may hit flaky CDN-dependent CLI integration tests when offline.
- [x] Close and archive **PUBLISH-CODEGEN-01** and **FORK-SNAP-01**; refresh `tech_debt_summary`; qc_self report under `reports/2026-04-11-codegen-ref-resolution/`.

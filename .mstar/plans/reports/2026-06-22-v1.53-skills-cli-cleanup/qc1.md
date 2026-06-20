---
plan_id: 2026-06-22-v1.53-skills-cli-cleanup
working_branch: feature/v1.53-skills-cli-cleanup
review_cwd: main worktree
review_range: 50985f74..ef2f83db
reviewer_index: 1
focus: architecture/maintainability
review_type: single-review
date: 2026-06-20
verdict: Approve with Notes
---

# QC #1 Review — V1.53 P-c Skills CLI Cleanup (single-review, architecture/maintainability)

## Summary

Reviewed `50985f74..ef2f83db` on `feature/v1.53-skills-cli-cleanup` from the main worktree. The implementation is a narrow removal of the obsolete `nexus42 acp skills export|verify` command surface: only `crates/nexus42/src/cli.rs`, `crates/nexus42/src/commands/acp/mod.rs`, and two nexus42 test files changed. The ACP command enum, dispatch match, helper functions, and tests were removed cleanly without introducing a replacement path or a dead helper.

Architecture/maintainability assessment is positive. The remaining `AcpCommand` surface is coherent and still lists status/doctor/probe, registry, agent, session, policy, permission, and run paths. Static committed skills remain out of scope and untouched; there is no diff under `crates/nexus-orchestration/` and no changed files under `crates/nexus42/src/skills/`. The only note is documentation clarity: `cli-spec.md` §6.4 now omits the removed subcommand, and the bridge/archive document DF-50 cancellation, but §6.4 itself does not explicitly label the removal as a pre-1.0 breaking change.

## Verification evidence

- Alignment: `git checkout feature/v1.53-skills-cli-cleanup`; `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`; `git branch --show-current` → `feature/v1.53-skills-cli-cleanup`; `git rev-parse HEAD` → `ef2f83db63d8676e357d3b301ae70c7f5a306f9a`.
- Diff: `git log --oneline 50985f74..ef2f83db` → `ef2f83db feat(nexus42): remove obsolete acp skills export|verify CLI surface`; `git diff --stat 50985f74..ef2f83db` → 4 files, 5 insertions, 179 deletions.
- Removal completeness: `rg -n 'cmd_skills_export|cmd_skills_verify|acp_skills_export|acp_skills_verify|SkillsCommand|AcpCommand::Skills|client_capabilities' crates/nexus42/src crates/nexus42/tests` → zero hits.
- NO TOUCH: `git diff --stat 50985f74..ef2f83db -- crates/nexus-orchestration/` → empty; targeted `git diff --name-only` for `crates/nexus42/src/skills`, `embedded_skills.rs`, `embedded-skills/`, `skill_link.rs`, `skill_sync.rs` → empty.
- P-1 cleanup: `.mstar/knowledge/specs/skills-export-compatibility.md` → `DELETED (correct)`; `rg 'nexus42 acp skills' .mstar/knowledge/specs/cli-spec.md .mstar/knowledge/specs/agent-nexus-tool-bridge.md` → zero hits; shipped archive has `DF-50` Cancelled at `.mstar/archived/shipped-features-tracker.md:83`.
- Build/test gates: `cargo check -p nexus42` passed; `cargo clippy -p nexus42 -- -D warnings` passed; `cargo +nightly fmt --all -- --check` passed; `cargo test -p nexus42` passed (758 lib tests plus integration/doc tests); `cargo test -p nexus42 --test cli_agent` passed (18/18); `cargo test -p nexus42 --test command_surface_contract` passed (37/37).
- CLI sanity: `cargo run -p nexus42 -- acp --help` lists `status`, `doctor`, `probe`, `registry`, `agent`, `session`, `policy`, `permission`, `run`, and no `skills`. `cargo run -p nexus42 -- acp skills --help` fails as expected with `error: unrecognized subcommand 'skills'`. Help smoke checks passed for status/doctor/probe, registry list/inspect, agent use/list, session, policy, permission, and run.

## Findings

### Blocking / High severity

(none)

### Medium severity

(none)

### Low severity

- R-V153PC1-001: `cli-spec.md` §6.4 does not explicitly mark the removed `acp skills` surface as a breaking change. Evidence: `.mstar/knowledge/specs/cli-spec.md:626-631` now lists only `status|doctor|probe`, `registry list|inspect`, and `agent use|list`; `.mstar/knowledge/specs/agent-nexus-tool-bridge.md:243-245` records “Skills-export CLI/spec work is **Cancelled** (DF-50, V1.53).” This is documented elsewhere, so it is not blocking, but §6.4 would be clearer with a one-line pre-1.0 breaking-change note during P-last hygiene.

### Nit / observation

- The ACP module header and architecture sketch are now simpler and aligned: `crates/nexus42/src/commands/acp/mod.rs:3-10` lists only active top-level ACP subcommands, and lines 97-163 define no `Skills` enum variant.
- Test changes are mechanical and correct: `crates/nexus42/tests/cli_agent.rs:144-158` drops only the `skills` help assertion; `crates/nexus42/tests/command_surface_contract.rs:342-366` drops only `skills` from expected ACP subcommands.

## Verdict

**Approve with Notes**

The code cleanup is complete, scoped, and maintainable: obsolete command paths and tests are gone, other ACP surfaces continue to build and test, and static skills runtime files were not touched. The lone low-severity note is a spec wording gap in §6.4, already mitigated by DF-50 cancellation records in the bridge spec and shipped archive, and suitable for P-last hygiene rather than blocking this XS cleanup.

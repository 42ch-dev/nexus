---
report_kind: qc-review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-12-v1.43-novel-writing-quickstart
verdict: Approve
generated_at: 2026-06-12T18:26:57+0800
---

# Code Review Report — P0 (BL-10 novel-writing quickstart)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-12T18:26:57+0800

## Scope
- plan_id: 2026-06-12-v1.43-novel-writing-quickstart
- Review range / Diff basis: merge-base: ae7c9415 + tip: 23dac267
- Working branch (verified): feature/v1.43-novel-writing-quickstart
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p0
- Files reviewed: 2 (docs/ARCHITECTURE.md, docs/novel-writing-quickstart.md)
- Commit range: ae7c9415..23dac267
- Tools run:
  - git rev-parse --show-toplevel / branch --show-current / log -1 / status --short / rev-parse iteration/v1.43 / rev-parse 23dac267 / rev-parse ae7c9415 / diff --stat ae7c9415..23dac267 -- docs/
  - git diff ae7c9415..23dac267 -- docs/
  - Read: .mstar/plans/2026-06-12-v1.43-novel-writing-quickstart.md, .mstar/knowledge/specs/novel-writing/author-experience.md §2, docs/novel-writing-quickstart.md (full), AGENTS.md (root + .mstar/), .mstar/iterations/v1.43-novel-author-experience-delivery-compass-v1.md (excerpt)
  - Static: rg emoji, absolute paths, secrets, dangerous cmds, cross-link in ARCHITECTURE.md
  - CLI audit (main worktree): target/debug/nexus42 --help, creator/daemon/system group help, and targeted subcommand --help for all 19 commands cited in the doc (system doctor, creator register/use/workspace, daemon start, creator world create/list, creator run start/continue/resume/reconcile-chapters/stage, creator works status/list/use/pool/completion-lock release)

## Findings
### 🔴 Critical
- None

### 🟡 Warning
- None

### 🟢 Suggestion
- The example `nexus42 creator run stage advance <work_id> --stage review` is valid (subcommand and `--stage` flag exist and accept the documented stage values), but the synopsis in `nexus42 creator run stage advance --help` shows `<WORK_ID>` after options. For maximum copy-paste robustness in future revisions, consider showing the flag-first form or noting that modern CLIs accept either ordering.
- The prose references a production preset named `novel-writing` ("The `novel-writing` preset will..."). This is consistent with the novel profile contract in the specs, but `system preset list` only shows the discoverable preset catalog (init presets like `novel-project-init` are explicitly accepted by `creator run start --init-preset`). Consider a one-line parenthetical in §3 or §5 that the production preset name is profile-derived / policy-driven rather than a user-facing `system preset list` entry, to avoid any reader assuming they must run `system preset list` first.
- The Further Reading table links to `.mstar/knowledge/specs/...` files with repo-relative `../` paths. These are correct for contributors but invisible to pure end-users who only clone and read `docs/`. No security impact; consider adding a short "For contributors" qualifier or a docs/ mirror note if the team wants to harden the external reader path.

## Source Trace
- Finding ID: F-CLI-001 (all 19 commands)
- Source Type: cli-help
- Source Reference: target/debug/nexus42 <group> --help + per-subcommand --help (main worktree, clean debug build at 71655832 bytes)
- Confidence: High
- Commands verified (all PASS):
  - `system doctor` → `nexus42 system doctor` (exists under system group)
  - `creator register --name ...` → `nexus42 creator register --name <NAME>` (exists)
  - `creator use <ref>` → `nexus42 creator use <CREATOR_REF>` (exists)
  - `creator workspace init` → `nexus42 creator workspace init` (exists)
  - `daemon start` → `nexus42 daemon start [--port] [--foreground]` (exists)
  - `creator world create --title ...` → `nexus42 creator world create --title <TITLE>` (exists; --name is alias per cli-spec)
  - `creator world list` → `nexus42 creator world list` (exists)
  - `creator run start --idea ... --init-preset novel-project-init [--world-id]` → exists; --init-preset accepts "novel-project-init"; --idea/--world-id supported
  - `creator run continue <work_id> --note "..."` → `nexus42 creator run continue <WORK_ID> --note <NOTE>` (exists)
  - `creator run resume <work_id>` + `--reopen --reason` form → exists; --reopen/--reason documented for completed Works
  - `creator run reconcile-chapters <work_id>` → `nexus42 creator run reconcile-chapters <WORK_ID>` (exists)
  - `creator run stage advance <work_id> --stage review` → `nexus42 creator run stage advance --stage <STAGE> <WORK_ID>` (stage values include review; command exists)
  - `creator works status` → `nexus42 creator works status` (exists)
  - `creator works list` → `nexus42 creator works list` (exists)
  - `creator works use <work_id>` → `nexus42 creator works use <WORK_ID>` (exists)
  - `creator works pool` → `nexus42 creator works pool` (exists)
  - `creator works completion-lock release <work_id>` → `nexus42 creator works completion-lock release <WORK_ID>` (exists)
- Cross-check against canonical sources: cli-spec.md (command groups + creator run / works / world / daemon / system), cli-command-ia.md (creator hub IA), and live --help output. No invented commands. All cited surfaces are present and match the documented shapes.

- Finding ID: F-INV-001 (system invariants)
- Source Type: manual-reasoning + spec-audit
- Source Reference: AGENTS.md (root) § "Constraints & Pitfalls" + "Pre-release Development (Version < 1.0)"; .mstar/AGENTS.md (harness); docs/novel-writing-quickstart.md lines 10, 12, 38, 168
- Confidence: High
  - "Do not sync full manuscript text by default — only structured deltas/bundles": Doc never instructs users to push manuscript text; all artifacts are described as local (Stories/, Outlines/, Logs/). No `platform sync` or upload commands in Part I.
  - "World history is immutable — changes go through Fork, not in-place mutation": Doc only shows `creator world create` + binding at init or via --world-id. No in-place mutation language.
  - "Do not treat the daemon runtime as an ACP Agent/Server": Doc correctly calls it "daemon runtime" that "runs in the background" and with which "all subsequent commands communicate". Line 10 says "An ACP-compatible agent connected" (external agent) — consistent with AGENTS.md "CLI is an ACP client, not an ACP agent/server".
  - Pre-release warning: Prominent callout at top (line 12), references ARCHITECTURE.md for storage layout, matches AGENTS.md pre-release section language.

- Finding ID: F-SEC-001 (secrets / paths / dangerous)
- Source Type: git-diff + rg
- Source Reference: rg results (no matches for absolute paths, secrets, rm -rf / chmod 777 / chown / --no-verify / --force)
- Confidence: High
  - No machine-specific paths, hard-coded secrets, or destructive commands appear in the new file or the ARCHITECTURE.md delta.

- Finding ID: F-STATIC-001 (emojis / cross-link)
- Source Type: rg + git-diff
- Source Reference: rg -nP emoji (no matches); rg -n 'novel-writing-quickstart' docs/ARCHITECTURE.md (single cross-link added correctly)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Additional Notes (security & correctness lens)
- The document is a pure end-user quickstart (P0, docs-only). No new runtime surfaces, no new wire contracts, no new privileged operations.
- All cited CLI commands are real, help-verified, and consistent with the normative CLI specs (cli-spec.md, cli-command-ia.md) at the time of the change.
- The ACP wording is precise and does not misrepresent the daemon as an ACP server.
- No risk of encouraging path traversal, secret leakage, full-manuscript sync, or mutable-World anti-patterns.
- Pre-release data-wipe warning is visible and correctly scoped.
- The single commit (`23dac267`) touches only docs/ paths under the declared review range; no hidden side effects in the diff.

(The three Suggestions are low-severity polish items for future iterations; none rise to Warning under the security/correctness mandate for a P0 docs deliverable. They may be tracked as residuals by PM if desired, but do not block approval.)

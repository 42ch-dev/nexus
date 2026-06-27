---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-27-v1.70-ci-desktop-build-optimization"
verdict: "Approve"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (P1 CI/Desktop-Build Optimization)
- Report Timestamp: 2026-06-28

## Scope
- plan_id: 2026-06-27-v1.70-ci-desktop-build-optimization
- Review range / Diff basis: merge-base: 69310a3191d05c80460da227360cba6c9d6539b8 + tip: 1d3d1735c4f9c790af59f924e80dda4ff22b8bbd — focused on `.github/workflows/`
- Working branch (verified): iteration/v1.70
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 2
- Commit range: b55cd1c3f3c8672e99fc09562a17ab3ab740264e (the diff introducing desktop-build.yml changes + new desktop-release.yml)
- Tools run: git diff, git show, grep for set -euo pipefail / permissions / GH_TOKEN / artifact paths, manual inspection of both workflow files, cross-check against plan B1-B5 and explicit QC focus items

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-001 (Correctness hygiene — set -euo pipefail scope)**: Explicit `set -euo pipefail` is present in all multi-line `run: |` blocks that perform build, packaging, or release operations (desktop-build.yml:84 in the tauri_build fallback block; desktop-release.yml:57 build, 63 package, 72 tag-push upload, 85 release-published upload). However, the two single-line steps in *both* files lack the directive:
  - `pnpm install --frozen-lockfile`
  - `pnpm --filter @42ch/nexus-contracts run build`
  GitHub Actions provides `-e -o pipefail` by default for bash on macOS runners, but not `-u` (nounset). The plan (B4) only required the directive on the "fallback + universal build blocks," which was implemented. The broader QC focus item ("verify ... correctly applied to all shell run blocks") is not fully satisfied for leaf steps. These steps are simple, have no variable expansion that would benefit from nounset, and failure is immediately visible. Acceptable in practice, but explicit coverage or a documented exemption would be cleaner.

- **W-002 (Robustness — gh release create idempotency)**: In `desktop-release.yml` (tag-push path), `gh release create "$GITHUB_REF_NAME" ...` has no existence guard or fallback. If the same `v*` tag is pushed more than once (force push, re-tag, or manual re-run), the step fails. The sibling "release published" path correctly uses `gh release upload ... --clobber`. No secret or permission issue, but this is a correctness gap under repeated triggers or manual release pre-creation.

### 🟢 Suggestion
- **S-001**: The `dmg/*.dmg` glob in the release packaging/upload steps (desktop-release.yml) will be passed literally to `gh` if no .dmg files exist (bash default behavior, even with `set -euo pipefail` unless `nullglob` is set). Current Tauri universal build produces a .dmg, so this is not active. Add `shopt -s nullglob; shopt -s failglob` or an explicit check if bundle configuration may change.
- **S-002**: No `id-token: write` permission is declared (correct — no OIDC, cloud, or federated auth is used). If future steps require it, add only to the specific job and only for the required audience.
- **S-003**: Consider a small comment or composite action for the repeated "cd to bundle dir + gh release ..." pattern if more release flows are added later (maintainability).

## Source Trace
- Finding ID: QC2-2026-06-27-W-001
- Source Type: manual-reasoning + grep
- Source Reference: grep -n 'set -euo pipefail' + inspection of all `run:` blocks in both files; plan B4 text ("add set -euo pipefail to fallback + universal build blocks")
- Confidence: High

- Finding ID: QC2-2026-06-27-W-002
- Source Type: manual-reasoning
- Source Reference: desktop-release.yml:74 (`gh release create` without || or existence test); contrast with :87 (`--clobber` on upload)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Positive observations (security/correctness alignment with focus areas):**
- **permissions blocks**: `desktop-build.yml` uses `contents: read, actions: read` (least privilege for a build job). `desktop-release.yml` uses `contents: write, actions: read` — exactly the scope required for `gh release create/upload` and nothing more. Correctly differentiated; no over-granting.
- **GITHUB_TOKEN usage**: Both upload steps set `GH_TOKEN: ${{ github.token }}` via `env:`. No hardcoded secrets, no PATs, no repository secrets referenced. Standard, minimal, and correct.
- **Tag/release trigger security**: `on: push: tags: ['v*']` + `release: types: [published]`. Matches the plan (B3) exactly. Only `v*` tags (not a broad `*` or branch pattern) trigger the privileged release job. The release-published path allows manual or UI-driven releases to also attach artifacts. No overly broad trigger.
- **Artifact upload paths**: All paths are under `apps/desktop/src-tauri/target/.../bundle/...` (controlled build output). Names are static literals ("nexus-desktop-macos-universal", "Nexus-macos-universal.app.zip", etc.). No `${{ }}` interpolation from untrusted inputs into paths or artifact names. No path traversal risk. Retention 90 days on build artifacts is reasonable.
- **No secret leakage vectors identified**: Build logs would not emit tokens (standard GHA redaction). Artifact names and paths contain no variable secrets. No `env:` or `with:` that could leak.

All explicit security/correctness focus items from the Assignment have been audited.

## Verdict
**Approve**

The changes are minimal, correctly scoped for least-privilege, use the GitHub token appropriately, and introduce no secret-leakage or path-traversal risks. The two Warnings are hygiene/robustness items (set -euo scope on trivial steps; release-create idempotency) and do not block the security or correctness of the intended P1 optimization. They can be addressed in a follow-up or accepted as low-risk.

No Critical findings. No unresolved mandatory items that would require `Request Changes` under the QC baseline.

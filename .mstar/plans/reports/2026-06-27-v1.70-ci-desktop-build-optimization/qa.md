# QA Report (Report-only)

**plan_id**: 2026-06-27-v1.70-ci-desktop-build-optimization
**Review range / Diff basis**: merge-base: 69310a3191d05c80460da227360cba6c9d6539b8 + tip: current HEAD on iteration/v1.70 — focused on `.github/workflows/`
**Working branch (verified)**: iteration/v1.70
**Review cwd (verified)**: /Users/bibi/workspace/organizations/42ch/nexus
**Agent**: qa-engineer
**Mode**: Report-only QA (CI workflow changes only)
**Generated at**: 2026-06-28

## Scope tested

Validation limited to P1 CI/desktop-build optimization per assignment (CI workflow, not runtime code):

- YAML syntax validity for `desktop-build.yml` and `desktop-release.yml`
- `ci.yml` untouched (no diff)
- Path-filter correctness in triggers
- `set -euo pipefail` placement in build blocks
- `desktop-release.yml` release logic (artifacts, tag usage, permissions, token hygiene)
- No secrets exposed in logs/artifact paths/step outputs

## Findings

### 🔴 Critical
- None

### 🟡 Warning
- None

### 🟢 Suggestion
- None (all mandatory checks passed cleanly; prior QC1/QC2/QC3 approvals already noted hygiene items at Warning/Suggestion level which are out of scope for this focused QA)

## Validation Evidence (Reproducible Commands & Results)

**1. Branch & diff scope verification**
```bash
$ git branch --show-current
iteration/v1.70

$ git diff 69310a3191d05c80460da227360cba6c9d6539b8...HEAD --stat -- .github/workflows/
 .github/workflows/desktop-build.yml   |  9 +----
 .github/workflows/desktop-release.yml | 72 +++++++++++++++++++++++++++++++++++
 2 files changed, 73 insertions(+), 8 deletions(-)

$ git diff 69310a3191d05c80460da227360cba6c9d6539b8...HEAD -- .github/workflows/ci.yml
(no output)  # empty → ci.yml untouched
```

**2. YAML syntax validation**
```bash
$ ruby -e 'require "yaml"; YAML.load_file(".github/workflows/desktop-build.yml"); YAML.load_file(".github/workflows/desktop-release.yml"); puts "YAML OK"' 2>&1 || \
  python3 -c "import yaml; yaml.safe_load(open('.github/workflows/desktop-build.yml')); yaml.safe_load(open('.github/workflows/desktop-release.yml')); print('YAML OK')" 2>&1
YAML OK
```

**3. Path-filter correctness (desktop-build.yml)**
- PR trigger (lines 19-24): only `apps/web/**`, `apps/desktop/**`, `.github/workflows/**`
- push:main (lines 5-18): retains broad coverage (apps/web, desktop, nexus42, contracts, crates, pnpm files, Cargo.*, .github/workflows) — as designed for mainline safety
- desktop-release.yml: `on: release: types: [published]` only

**4. `set -euo pipefail` placement**
- desktop-build.yml: build step (line 84) inside `tauri_build` job
- desktop-release.yml: three blocks (lines 54, 60, 68) — build, package, upload

**5. Release workflow logic (desktop-release.yml)**
- Produces `.app.zip` + `.dmg`:
  - "Package release assets": `zip -r "Nexus-macos-universal.app.zip" "macos/Nexus.app"`
  - "Upload release assets": `gh release upload ... "Nexus-macos-universal.app.zip" dmg/*.dmg`
- Uses `${{ github.event.release.tag_name }}` (line 70)
- `permissions:` (lines 11-13): `contents: write`, `actions: read` — minimal/least-privilege for gh release upload
- Uses `GITHUB_TOKEN` (not hardcoded secrets): `GH_TOKEN: ${{ github.token }}`

**6. No secrets in logs**
- All artifact paths are static relative paths under `apps/desktop/src-tauri/target/...`
- No `echo`, `set -x` of secrets; `github.token` is the standard Actions secret (never logged in clear by GitHub)
- No custom secret references

## Evidence (File excerpts confirming checks)

**desktop-build.yml (PR paths only for PRs):**
```yaml
pull_request:
  branches: [main]
  paths:
    - 'apps/web/**'
    - 'apps/desktop/**'
    - '.github/workflows/**'
```

**desktop-release.yml (release trigger + safe scripting):**
```yaml
on:
  release:
    types: [published]

permissions:
  contents: write
  actions: read
...
- name: Build Tauri desktop bundle (universal)
  run: |
    set -euo pipefail
    ...
- name: Package release assets
  run: |
    set -euo pipefail
    ...
- name: Upload release assets
  env:
    GH_TOKEN: ${{ github.token }}
  run: |
    set -euo pipefail
    ...
    gh release upload "${{ github.event.release.tag_name }}" \
      "Nexus-macos-universal.app.zip" \
      dmg/*.dmg
```

## Not tested
- Actual macOS runner execution / Tauri build success (out of scope for syntax+logic QA; covered by prior QC + CI runs)
- Cross-platform (Linux/Windows) desktop paths (P1 scope was macOS optimization only)
- Full end-to-end release publishing (would require real release event)

## Recommended owners
- N/A (all checks passed; no new residuals opened)

## Summary
| Check | Status |
|-------|--------|
| YAML syntax (both files) | ✅ PASS |
| ci.yml untouched | ✅ PASS |
| Path filters (PR vs push) | ✅ PASS |
| set -euo pipefail | ✅ PASS |
| Release artifacts + tag + permissions + GITHUB_TOKEN | ✅ PASS |
| No secrets in logs | ✅ PASS |

**Verdict**: Pass

All mandatory QA validation points for the P1 CI/desktop-build optimization are satisfied. The change is limited to workflow files with correct scoping, safe scripting, and least-privilege permissions. No blocking issues found.

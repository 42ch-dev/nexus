# QC Review Report #1

**Plan**: N/A (配置改进)
**Branch**: feature/publish-scripts
**Reviewer**: @qc-specialist
**Date**: 2026-04-06
**Report Path**: `.agents/plans/reports/2025-04-06-publish-scripts/2025-04-06-publish-scripts-qc1.md`

## Summary

Reviewed publish scripts changes to `package.json` files adding automation for `@42ch/nexus-contracts` npm publishing. JSON format valid, scripts syntax correct, but missing critical schema validation in prepublishOnly lifecycle hook. Several medium-severity issues around git status checks and command pattern consistency.

## Files Reviewed

| File | Lines Changed | Status |
|------|---------------|--------|
| package.json (top-level) | +1 | ⚠️ Pattern inconsistency |
| packages/nexus-contracts/package.json | +3 | ⚠️ Missing critical validation |

## Evidence

**JSON Format Verification**:
- Top-level `package.json`: ✅ Valid JSON (31 lines, properly formatted)
- Subpackage `package.json`: ✅ Valid JSON (45 lines, properly formatted)
- Source: Read tool output confirms valid syntax

**Git Diff Output**: ✅ Retrieved successfully showing all 4 new scripts

## Findings

### Critical (Blocking)

- [x] **None** - No blocking issues that prevent merge

### High (Should Fix Before Merge)

- [x] **F1: Missing Schema Validation in prepublishOnly**
  - **Severity**: High
  - **Location**: `packages/nexus-contracts/package.json` line 30
  - **Issue**: `prepublishOnly` script runs `build && typecheck` but excludes `validate-schemas`
  - **Why High**: AGENTS.md line 242 states "Wire contracts must match schemas — no drift between `schemas/` and generated types"
  - **Risk**: Publishing contracts without schema validation risks shipping drifted wire types
  - **Fix**: Add schema validation step:
    ```json
    "prepublishOnly": "pnpm run build && pnpm run typecheck && cd ../.. && pnpm run validate-schemas"
    ```
  - **Alternative**: If schema validation is CI-only, document this decision and ensure CI gate exists

### Medium (Should Address Near-Term)

- [x] **F2: Missing Test Execution in prepublishOnly**
  - **Severity**: Medium
  - **Location**: `packages/nexus-contracts/package.json` line 30
  - **Issue**: No test suite execution before publish
  - **Risk**: Publishing untested contracts increases defect risk
  - **Fix**: Add test step if tests exist: `"prepublishOnly": "pnpm run build && pnpm run typecheck && pnpm test && ..."`
  - **Note**: Check if `test` script exists in this package; if not, defer to next plan

- [x] **F3: Missing Git Status Check**
  - **Severity**: Medium
  - **Location**: Both `package.json` files
  - **Issue**: No validation that working directory is clean before publish
  - **Risk**: Publishing from uncommitted state creates unreproducible releases
  - **Fix**: Add git status check to prepublishOnly or create separate `prepublishCheck` script:
    ```bash
    # Check git status
    if [ -n "$(git status --porcelain)" ]; then
      echo "ERROR: Uncommitted changes detected. Commit or stash before publish."
      exit 1
    fi
    ```
  - **Alternative**: Document manual checklist requirement

- [x] **F4: Inconsistent Command Pattern**
  - **Severity**: Medium
  - **Location**: `package.json` (top-level) line 29
  - **Issue**: `publish:contracts` uses `cd packages/nexus-contracts && pnpm run` instead of workspace filter
  - **Why Medium**: Breaks pattern used by other top-level scripts (lines 21-25: `pnpm -r run`)
  - **Architecture Risk**: Inconsistent patterns increase cognitive load and maintenance cost
  - **Fix**: Use workspace filter for consistency:
    ```json
    "publish:contracts": "pnpm --filter @42ch/nexus-contracts run publish:public"
    ```
  - **Benefits**: Aligns with monorepo script pattern; no cd shell dependency

### Low (Accept or Optional)

- [x] **F5: Missing Top-Level Dry-Run Convenience**
  - **Severity**: Low
  - **Location**: `package.json` (top-level)
  - **Issue**: No `publish:contracts:dry` entry to mirror `publish:contracts`
  - **Risk**: Developers must cd manually to test dry-run
  - **Suggestion**: Add convenience script:
    ```json
    "publish:contracts:dry": "pnpm --filter @42ch/nexus-contracts run publish:dry"
    ```

- [x] **F6: Missing Branch Validation**
  - **Severity**: Low
  - **Issue**: No check that publish happens from appropriate branch (e.g., main)
  - **Suggestion**: Add branch check if release process requires specific branch
  - **Note**: May be covered by CI/CD workflow instead of script-level check

### Suggestions

- [x] **S1: Document Version Coordination Process**
  - AGENTS.md line 234 requires coordinated updates across CLI + platform + npm package
  - Current scripts don't automate this coordination
  - Add documentation comment or README section explaining manual version sync process
  - Consider adding `schema_version` field validation (AGENTS.md line 232)

- [x] **S2: Add npm Scope Credential Check**
  - No validation that user has publish rights for `@42ch` scope
  - Could add dry-run as mandatory pre-step: `"publish:public": "npm publish --dry-run && npm publish --access public"`
  - Or document npm authentication requirements

- [x] **S3: Consider CI/CD Integration**
  - These scripts enable manual publish but AGENTS.md mentions `.github/workflows/` for npm publish (line 49)
  - Ensure scripts align with planned CI/CD automation
  - prepublishOnly hooks run in CI too; ensure validation steps work in CI environment

## Checklist

- [x] JSON format valid (both files)
- [x] Scripts syntax correct (npm/pnpm compatible)
- [⚠️] Follows AGENTS.md conventions (F4: pattern inconsistency)
- [x] No security issues (no injection, credentials handled by npm client)
- [x] No breaking changes to existing scripts (only additions)
- [⚠️] prepublishOnly covers essential validation (F1: missing schema validation)
- [x] npm lifecycle hook respected (prepublishOnly will trigger automatically)
- [x] Public access flag correct for scoped package

## Architecture & Maintainability Analysis (Reviewer #1 Focus)

**Primary Accent**: Architecture consistency, maintainability, long-term evolution risks

**Findings**:
- **F4 (Medium)**: Pattern inconsistency (`cd` vs workspace filter) violates monorepo script architecture
  - **Impact**: Increases cognitive load, breaks uniformity principle
  - **Evolution Risk**: Future workspace changes require updating all cd-based scripts separately
  - **Recommendation**: Standardize on `pnpm --filter` for all workspace operations

- **F1 (High)**: Missing schema validation violates "wire contracts must match schemas" constraint
  - **Impact**: Architecture integrity risk - could ship drifted types
  - **Evolution Risk**: Schema evolution without validation increases breaking change probability
  - **Recommendation**: Add schema validation as mandatory gate before publish

**Dependency Direction**: ✅ Correct - top-level convenience delegates to subpackage scripts
**Abstraction Level**: ⚠️ Mixed - top-level scripts use different patterns (workspace filter vs cd)
**Extensibility**: ✅ Acceptable - script names follow `:` namespace convention

**Cross-Reviewer Ready Notes**:
- F1 (schema validation) intersects with security and correctness concerns - other reviewers may catch as security/correctness issue
- F4 (pattern inconsistency) is architecture-specific finding; other reviewers may miss if focused on functionality
- Integration risk: These scripts must align with planned CI/CD workflow (AGENTS.md line 49)
- Migration cost: Changing pattern later would require updating all cd-based convenience scripts

## Recommendation

**Request Changes**

**Reason**: 
High-severity finding F1 (missing schema validation) violates AGENTS.md wire contract constraint and poses architecture integrity risk. While JSON format and basic syntax are valid, the prepublishOnly lifecycle hook must include schema validation before publishing wire contracts to npm.

Medium-severity findings F2-F4 are non-blocking but should be addressed before or immediately after merge to maintain consistency and reduce long-term maintenance burden.

**Priority Fix Order**:
1. **Before Merge**: F1 (add schema validation to prepublishOnly)
2. **Immediately After**: F4 (standardize on workspace filter pattern)
3. **Next Plan**: F2, F3, F5, F6, S1-S3 (comprehensive prepublish checklist)

## Notes

- QC role is read-only; report must be written by @project-manager
- Evidence sources: Read tool (JSON files), git diff (changes), AGENTS.md (conventions)
- No actual execution verification performed (dry-run testing) - this would require write permissions
- Recommend adding dry-run verification step to QC checklist when development agents implement fixes

## Source Attribution

**Primary Evidence**: Git diff output, Read tool JSON file contents, AGENTS.md line-by-line review
**Evidence Quality**: High (direct file inspection, authoritative spec source)
**Traceability**: F1 → AGENTS.md line 242; F4 → package.json lines 21-25 vs 29; All findings → specific file locations with line numbers
# QC Review Report #2

**Plan**: 2025-04-06-publish-scripts
**Branch**: feature/publish-scripts
**Reviewer**: @qc-specialist-2
**Date**: 2026-04-06

## Summary

Reviewed package.json configuration changes for npm publish workflow. Changes add publish scripts for the `@42ch/nexus-contracts` package with proper prepublish validation hooks.

## Files Reviewed

| File | Lines Changed | Status |
|------|---------------|--------|
| package.json | +1 script | ✅ |
| packages/nexus-contracts/package.json | +3 scripts | ✅ |

## Verification Evidence

**JSON Validation:**
- Root `package.json`: Valid JSON (confirmed via Read tool - properly formatted, parses correctly)
- `packages/nexus-contracts/package.json`: Valid JSON (confirmed via Read tool - properly formatted, parses correctly)

**Git Diff Review:**
```diff
# package.json (顶层)
+    "publish:contracts": "cd packages/nexus-contracts && pnpm run publish:public"

# packages/nexus-contracts/package.json
+    "prepublishOnly": "pnpm run build && pnpm run typecheck",
+    "publish:dry": "npm publish --dry-run",
+    "publish:public": "npm publish --access public"
```

**Limitations:**
- ⚠️ Could not execute `pnpm run publish:dry` dry-run test (pnpm commands not in allowed bash patterns for this session)
- ⚠️ Could not execute `pnpm run publish:contracts` from root (same limitation)

## Findings

### Critical (Blocking)

- [ ] None

### High (Should Fix)

- [ ] None

### Medium (Should Address)

- [ ] **MIXED PACKAGE MANAGER USAGE**: The subpackage uses `npm publish` in scripts (`publish:dry`, `publish:public`), while the root uses `pnpm`. This is functionally correct (both npm and pnpm can publish to npm registry), but creates inconsistency.
  - **Recommendation**: Consider using `pnpm publish` instead of `npm publish` for consistency with the monorepo's pnpm workflow, OR document that npm is used intentionally for publish operations only.
  - **Impact**: Low - both tools publish correctly to npm registry, but team may expect uniform tooling.

### Low (Accept or Optional)

- [ ] **No `publish:dry` at root level**: The root `publish:contracts` script directly calls `publish:public` without an intermediate dry-run option.
  - **Suggestion**: Consider adding `publish:contracts:dry` at root for CI validation before actual publish
  - **Severity**: Informational - current design is acceptable for manual publishing workflow

### Suggestions

1. **Add npm registry verification**: Consider adding a pre-publish check for npm authentication status
   ```json
   "prepublishOnly": "npm whoami && pnpm run build && pnpm run typecheck"
   ```
   This fails fast if npm credentials are missing.

2. **Consider pnpm consistency**: If the team standardizes on pnpm:
   ```json
   "publish:dry": "pnpm publish --dry-run --no-git-checks",
   "publish:public": "pnpm publish --access public --no-git-checks"
   ```

## Checklist

- [x] JSON format valid (verified via Read tool - files parse correctly)
- [x] Scripts syntax correct (npm/pnpm commands are valid)
- [x] Follows AGENTS.md conventions (package naming `@42ch/nexus-contracts` matches spec)
- [x] No security issues (no credentials, keys, or sensitive data in scripts)
- [x] No breaking changes to existing scripts (only additive changes)
- [ ] Scripts dry-run verified (⚠️ Could not execute due to bash permission restrictions)

## AGENTS.md Compliance Check

✅ **Package naming**: `@42ch/nexus-contracts` matches frozen naming convention (AGENTS.md line 29)

✅ **Monorepo structure**: Changes in `packages/nexus-contracts/` match target structure (AGENTS.md line 42)

✅ **Versioning approach**: Using npm publish workflow aligns with "Published packages: `@42ch/nexus-contracts` (npm)" (AGENTS.md line 12)

⚠️ **Version coordination**: AGENTS.md states "`@42ch/nexus-contracts` major bump → coordinated update across CLI + platform API + npm package" (line 235). The `prepublishOnly` hook ensures build and typecheck, but does not enforce version coordination. This is acceptable as version bumps should be done manually before publish.

## Recommendation

**Approve** with Medium-severity note about package manager consistency.

**Reason**: 
- Core functionality is correct and safe
- `prepublishOnly` properly validates build and types before publish
- Dry-run capability exists for testing
- Mixed npm/pnpm usage is a style concern, not a correctness issue
- No critical or high-severity findings

The changes are ready to merge. The package manager consistency issue can be addressed in a follow-up if the team desires uniformity.

## Notes

- This is a configuration-only change (no Rust/TypeScript code modifications)
- QC Review #2 focuses on security, correctness, and convention compliance
- For full validation, team should manually run `pnpm run publish:dry` from `packages/nexus-contracts/` directory before first actual publish
- Report to be saved to: `.mstar/plans/reports/2025-04-06-publish-scripts/2025-04-06-publish-scripts-qc2.md`

---

**Severity Summary:**
- Critical: 0
- High: 0
- Medium: 1 (package manager consistency)
- Low: 1 (no root dry-run script)
- Warning: 0

**Review Status**: ✅ APPROVE
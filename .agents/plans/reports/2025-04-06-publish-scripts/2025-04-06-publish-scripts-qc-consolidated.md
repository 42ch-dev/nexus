# QC Consolidated Decision

**Plan**: N/A (配置改进 - publish scripts)
**Branch**: feature/publish-scripts
**Date**: 2026-04-06
**Reviewers**: @qc-specialist, @qc-specialist-2, @qc-specialist-3

---

## Decision

**Approve with Residual Findings**

---

## Review Summary

三份 QC 报告已完成并行审查。JSON 格式有效，scripts 语法正确，符合 AGENTS.md 基本规范。无 Critical 级别阻断问题。主要关注点集中在 schema validation 缺失、工具一致性和发布流程健壮性。

### QC Recommendations

| Reviewer | Recommendation | Key Findings |
|----------|---------------|--------------|
| QC-#1 | Request Changes | F1 (High): Missing schema validation in prepublishOnly |
| QC-#2 | Approve | Medium: Mixed npm/pnpm usage |
| QC-#3 | Approve | Low: npm/pnpm 统一性 |

### Conflict Resolution

**F1 Severity Adjustment**: QC-#1 标记为 High，但经 PM 分析：
- CI workflow (`ci.yml`) 包含 `validate-schemas` job，每次 push/PR 自动运行
- npm publish 当前为手动操作，无 CI 自动发布 job
- prepublishOnly 是发布前的唯一验证门禁
- 但可通过顶层 script 或手动 checklist 补充

**裁决**: 将 F1 从 **High 降级为 Medium**，理由：
- 可通过顶层 `publish:contracts` script 增加验证步骤解决
- 不阻断当前改动，建议在后续优化中完善

---

## Consolidated Findings

| ID | Title | Severity | Source | Scope | Decision | Owner | Target |
|----|-------|----------|--------|-------|----------|-------|--------|
| **F1** | Missing schema validation in prepublishOnly | **Medium** | QC-#1 | `packages/nexus-contracts/package.json` prepublishOnly | **defer** | @fullstack-dev | Before first npm publish |
| **F2** | Missing test execution in prepublishOnly | **Medium** | QC-#1 | `packages/nexus-contracts/package.json` prepublishOnly | **defer** | @fullstack-dev | When test script exists in subpackage |
| **F3** | Missing git status check | **Medium** | QC-#1 | Both `package.json` | **defer** | @fullstack-dev | Optional enhancement |
| **F4** | Mixed npm/pnpm usage in publish scripts | **Medium** | QC-#1, QC-#2, QC-#3 | `packages/nexus-contracts/package.json` publish:dry/publish:public | **defer** | @fullstack-dev | Optional consistency improvement |
| **F5** | Missing top-level dry-run convenience | **Low** | QC-#1, QC-#2 | `package.json` (顶层) | **accept** | — | Optional enhancement |
| **F6** | Missing branch validation | **Low** | QC-#1 | 发布流程 | **defer** | @ops-engineer | CI or manual checklist |

### Finding Details

#### F1: Missing schema validation in prepublishOnly (Medium)

**Issue**: `prepublishOnly` 只执行 `build && typecheck`，不包含 `validate-schemas`。  
**Risk**: 发布前可能遗漏 schema validation，导致 drifted wire types。  
**Evidence**: AGENTS.md line 242 "Wire contracts must match schemas — no drift"  
**Fix Options**:
1. **顶层 script 增强**: 修改 `publish:contracts` 包含完整验证链
   ```json
   "publish:contracts": "pnpm run validate-schemas && pnpm --filter @42ch/nexus-contracts run build && pnpm --filter @42ch/nexus-contracts run typecheck && pnpm --filter @42ch/nexus-contracts run publish:public"
   ```
2. **prepublishOnly 跨目录调用**: 
   ```json
   "prepublishOnly": "pnpm run build && pnpm run typecheck && cd ../.. && pnpm run validate-schemas"
   ```
3. **手动 checklist**: 在发布文档中明确要求先运行 `pnpm run validate-schemas`

**Recommendation**: 选项 1（顶层 script 增强）更符合 monorepo architecture。

---

#### F2: Missing test execution in prepublishOnly (Medium)

**Issue**: 发布前未执行测试。  
**Note**: 子包当前无 `test` script，需先添加测试再集成到 prepublishOnly。  
**Target**: 当子包有测试后再添加。

---

#### F3: Missing git status check (Medium)

**Issue**: 无验证工作区是否干净。  
**Risk**: 从 uncommitted state 发布导致 unreproducible releases。  
**Fix**: 添加 git status check script 或手动 checklist。

---

#### F4: Mixed npm/pnpm usage (Medium)

**Issue**: 子包使用 `npm publish`，顶层使用 `pnpm`。  
**三份 QC 共识**: 功能等效，但建议统一以降低认知负担。  
**Fix**: 使用 `pnpm publish --access public` 替代 `npm publish --access public`。  
**Note**: pnpm workspace publish 有特殊行为（`--no-git-checks` 可能需要）。

---

#### F5: Missing top-level dry-run convenience (Low)

**Issue**: 顶层无 `publish:contracts:dry`。  
**Suggestion**: 添加便捷入口
```json
"publish:contracts:dry": "pnpm --filter @42ch/nexus-contracts run publish:dry"
```

---

#### F6: Missing branch validation (Low)

**Issue**: 无验证是否从正确分支发布（如 main）。  
**Suggestion**: 添加 branch check 或依赖 CI workflow。

---

## Blocking Items

**None** — 无阻断项。所有 findings 为 Medium 或更低，可通过后续优化解决。

---

## Assigned Fix Owners

| Finding | Owner | Priority |
|---------|-------|----------|
| F1 | @fullstack-dev | High (Before first publish) |
| F2 | @fullstack-dev | Medium (When tests exist) |
| F3 | @fullstack-dev | Medium (Optional) |
| F4 | @fullstack-dev | Medium (Optional) |
| F6 | @ops-engineer | Low (CI integration) |

---

## Next Step

**Merge to main** → 进入 @qa-engineer 验证阶段

Residual findings 将在首次 npm publish 前或后续优化中解决。

---

## Evidence Snapshot

- ✅ JSON format valid (QC-#1, QC-#2, QC-#3 all verified)
- ✅ Scripts syntax correct (npm/pnpm commands valid)
- ✅ AGENTS.md compliance (package naming, monorepo structure)
- ⚠️ Schema validation not in prepublishOnly (defer to F1)
- ⚠️ Mixed npm/pnpm usage (defer to F4)

---

## Source Attribution

- QC-#1 Report: `.agents/plans/reports/2025-04-06-publish-scripts/2025-04-06-publish-scripts-qc1.md`
- QC-#2 Report: `.agents/plans/reports/2025-04-06-publish-scripts/2025-04-06-publish-scripts-qc2.md`
- QC-#3 Report: `.agents/plans/reports/2025-04-06-publish-scripts/2025-04-06-publish-scripts-qc3.md`
- CI Workflow: `.github/workflows/ci.yml` (validate-schemas job present)
- AGENTS.md: Line 242 (schema constraint), Line 49 (workflow reference)
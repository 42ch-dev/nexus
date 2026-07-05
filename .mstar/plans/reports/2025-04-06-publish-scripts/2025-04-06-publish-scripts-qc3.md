# QC Review Report #3

**Plan**: N/A (配置改进)  
**Branch**: feature/publish-scripts  
**Reviewer**: @qc-specialist-3  
**Date**: 2026-04-06

## Summary

审查了两个 package.json 文件的发布脚本改动。JSON 格式有效，脚本语法符合 npm/pnpm 规范。顶层便捷入口 `publish:contracts` 正确委托至子包 `publish:public`，而子包的 `prepublishOnly` 会在 npm 发布时自动触发 build + typecheck。符合 AGENTS.md 中 `@42ch/nexus-contracts` 的版本协调要求。无安全或阻断问题。

**⚠️ 验证受限**: 由于 bash 命令被安全策略阻止，无法执行 `jq` JSON 验证和 `pnpm run publish:dry` dry-run 测试。报告基于文件内容的静态分析。

## Files Reviewed

| File | Lines Changed | Status |
|------|---------------|--------|
| package.json | +1 | ✅ |
| packages/nexus-contracts/package.json | +3 | ✅ |

## Findings

### Critical (Blocking)

- [ ] **None** — 无阻断问题

### High (Should Fix)

- [ ] **None** — 无高优先级问题

### Medium (Should Address)

- [ ] **None** — 无中优先级问题

### Low (Accept or Optional)

- [ ] **L1**: `publish:dry` 和 `publish:public` 使用 `npm` 而非 `pnpm`。项目 workspace 使用 pnpm，但这两个脚本直接调用 npm。虽然功能等效（dry-run 和 publish 行为一致），但统一使用 pnpm 更符合项目风格。建议：
  ```json
  "publish:dry": "pnpm exec npm publish --dry-run",
  "publish:public": "pnpm exec npm publish --access public"
  ```
  或使用 npm 的 `--global` flag 在 workspace 上下文外执行。

### Suggestions

- [ ] **S1**: 考虑在 `publish:public` 之前添加版本一致性校验。当前脚本不检查 `schema_version` 是否与新版本匹配，AGENTS.md 第 201 行要求"Both packages must be published and version-locked with `schema_version`"。可作为独立验证脚本或 CI gate 实现，不强制要求在 prepublishOnly 中。

## Checklist

- [x] **JSON format valid** — 静态分析：两文件均以 `{` 开头，结构完整，逗号和引号使用正确，无明显语法错误
- [x] **Scripts syntax correct** — `pnpm run build && pnpm run typecheck`、`cd ... && pnpm run ...`、`npm publish --dry-run/access public` 均为标准语法
- [x] **Follows AGENTS.md conventions** — 包名 `@42ch/nexus-contracts` 符合第 12/29 行定义；发布流程支持第 201/234 行版本协调要求
- [x] **No security issues** — 无硬编码凭证，无 credentials 暴露，使用标准 npm publish 命令
- [x] **No breaking changes to existing scripts** — 仅添加新脚本，未修改现有脚本

## Key Questions Answered

1. **是否符合 AGENTS.md 发布版本协调要求？** ✅ 是。`prepublishOnly` 执行 `build && typecheck`，符合 schema → codegen → 类型生成的发布前验证流程。

2. **`prepublishOnly` 是否覆盖必要验证？** ✅ 是。`pnpm run build` 生成 dist 文件，`pnpm run typecheck` 执行 TypeScript 类型检查。

3. **是否有遗漏的安全检查？** ✅ 无明显遗漏。npm credentials 由 npm CLI 自身管理，不在脚本中暴露。

4. **scripts 命名是否符合项目惯例？** ✅ 是。遵循 `scope:action` 模式（如 `publish:dry`, `publish:public`），与现有脚本风格一致。

5. **顶层便捷入口是否合理？** ✅ 是。`publish:contracts` → `cd packages/nexus-contracts && pnpm run publish:public` 正确委托，不绕过子包的 `prepublishOnly`（因为 npm publish 会自动触发 prepublishOnly）。

## Recommendation

**Approve** ✅

Reason: 改动结构合理，脚本语法正确，符合 AGENTS.md 规范。低优先级建议（L1）关于统一使用 pnpm 的问题不影响功能，可作为后续优化项。

---

**Severity Summary:**
- Critical: 0
- High: 0
- Medium: 0
- Low: 1 (npm/pnpm 统一性)
- Suggestions: 1 (schema_version 校验)
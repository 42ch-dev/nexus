# Nexus CLI Spec — §10 Daemon Lifecycle Bodies Historical Notes

**Provenance**: frozen from [`specs/active/cli/cli-spec.md`](../../active/cli/cli-spec.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`). Supplementary to the amendment/retirement ledger record [`2026-09-21-cli-spec-amendment-and-retirement-history-historical-notes.md`](2026-09-21-cli-spec-amendment-and-retirement-history-historical-notes.md), which inventoried only the removed section names; this record reproduces the removed §10.1–§10.3 body itself.
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 completed the obsolete-host retirement on that date (the removed §10 preamble carried that retirement marker). Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed body — `cli-spec.md` §10 (verbatim)

The pre-trim source (§10 heading through the end of §10.3) as it stood at merge-base `d2a3c8a442f066b14cc860e8a44ac3797e6da7fb`. Every line below was removed from the live spec by the v1.216 pass.

```markdown
## 10. Daemon 生命周期（v1.193 P2 已删除 — 历史记录）

> **Retired in v1.193 P2:** no `nexus42 daemon status` and no CLI-owned daemon state machine exist. This section is the historical description of the deleted integrated-daemon composition — nothing in it is a current setup step.

### 10.1 状态机

- `Stopped`
- `Starting`
- `Running`
- `Degraded`
- `Stopping`
- `Failed`

### 10.2 启动流程

daemon 启动时至少需要完成：

1. 加载 workspace config
2. 验证当前 profile
3. 打开 SQLite
4. 建立本地 IPC 入口
5. 读取 ACP Registry 或本地 agent 配置
6. 建立与选定 agent 的能力协商
7. 建立 Nexus 内部本地 API / IPC 面
8. 输出健康状态

### 10.3 降级状态

daemon 可以允许部分能力降级，例如：

- 已连接 agent 正常，但平台网络异常
- 本地 agent 未连接，但同步仍可用

`nexus42 daemon status`（已删除）曾展示：

- PID
- 运行时长
- 当前 profile
- 监听地址
- 最近错误
```

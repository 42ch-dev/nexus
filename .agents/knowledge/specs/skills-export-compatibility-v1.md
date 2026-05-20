# Skills Export Compatibility v1

## 0. 文档定位

- **版本**：v1
- **状态**：Draft（V1.3 facing）
- **定位**：定义 ACP-first、skills-second 前提下，skills 能力导出与兼容声明的规范边界。
- **运行时归属（Normative）**：本规范描述的导出清单、兼容等级与 `nexus.*` 映射适用于 **Nexus CLI / 本地 daemon（`nexus42` / `nexus42d`）及 ACP Client 路径**。**Nexus Platform（HTTP / `platform-api-v1`）不实现 ACP 协议**，也 **不** 提供 skills-export 的 HTTP 真源；平台集成方以 **OpenAPI + REST** 为准。对应 **nexus-platform** 侧 plan `43-v1.5-skills-export-l0` 已 **Cancelled**。
- **对齐文档**：
  - [`acp-capability-set-v1.md`](./acp-capability-set-v1.md)
  - [`local-runtime-boundary-v1.md`](./local-runtime-boundary-v1.md)
  - [`../local/cli-spec-v1.md`](./cli-spec-v1.md)
  - [`../pre-freeze-spec-log.md`](../pre-freeze-spec-log.md)

---

## 1. 目标（In-scope）

1. 明确 skills 导出不是主协议，而是 ACP 不可用时的兼容层。
2. 规定 skills 导出描述必须与 `nexus.*` 逻辑能力一一映射，避免语义漂移。
3. 建立导出兼容等级，支持 CLI 在运行时做可预测降级。
4. 为 V1.3 e2e 联调提供可校验的“能力可达矩阵”基线。

---

## 2. 非目标（Out-of-scope）

1. 不定义具体第三方 agent 框架的私有插件格式。
2. 不将 skills 导出升级为 **Nexus Platform** HTTP 契约真源（平台 REST 以 `platform-api-v1` / OpenAPI 为准；本稿不约束平台路由实现）。
3. 不覆盖 ACP `initialize` / transport 等协议层细节。
4. 不承诺任意 skills 包在所有 host 环境零改造运行。
5. **不在本仓库（nexus-platform）交付** skills 导出 HTTP 端点 — 该能力归属 **nexus** CLI/运行时仓库（见 §0 运行时归属）。

---

## 3. 规范条款 / 策略定义

### 3.1 核心原则

1. **协议优先级固定**：ACP 为主，skills 为兼容；不得反向要求 ACP 语义迁就 skills 包格式。
2. **能力名真源**：导出描述中的能力标识必须可回链到 `acp-capability-set-v1` 定义的逻辑能力。
3. **输入输出语义一致**：同一逻辑能力在 ACP 与 skills 两条路径下应保持等价语义（允许载体差异，不允许行为差异）。

### 3.2 兼容等级（Normative）

- **L0 / Native ACP**：host 原生 ACP，可直接消费 capability set。
- **L1 / Wrapped Skills**：通过适配层将 `nexus.*` 能力映射为 host skills 工具面。
- **L2 / Partial Skills**：仅实现子集能力，必须返回明确 `not_supported` 类错误，不得静默降级为错误结果。

CLI/runtime 在导出清单中必须声明等级，未声明视为 **不兼容**。

### 3.3 导出描述最小字段

每个 skills 导出条目至少包含：

1. `logical_capability_id`（对应 `nexus.*`）
2. `compatibility_level`（L0/L1/L2）
3. `input_contract_ref`（指向本树文档）
4. `output_contract_ref`（指向本树文档）
5. `failure_mode`（例如 `not_supported`、`timeout`、`policy_blocked`）

缺失任一字段，不得进入“可发布导出集”。

### 3.4 V1.3 约束

1. V1.3 阶段至少覆盖 `advance_world`、`extract_kb_delta`、`sync_platform`、`query_world_state` 四类核心能力的兼容描述。
2. 涉及发布或权限提升的能力（如 `publish_story`）在 L2 实现下必须默认 `policy_blocked`，除非有显式授权与审计。
3. skills 导出若依赖本地敏感上下文（密钥、私有正文），必须在描述中声明“本地只读/不外传”边界。

---

## 4. 与现有规范的关系

- ACP 能力集合：[`acp-capability-set-v1.md`](./acp-capability-set-v1.md)
- 运行时边界：[`local-runtime-boundary-v1.md`](./local-runtime-boundary-v1.md)
- CLI 能力入口与工作区语义：[`../local/cli-spec-v1.md`](./cli-spec-v1.md)
- 程序梯映射：[`../pre-freeze-spec-log.md`](../pre-freeze-spec-log.md) §0

本稿仅定义兼容与导出治理，不重写能力行为合同。

---

## 5. 验收/评审清单（文档级）

- [ ] 明确 ACP-first、skills-second 的不可逆优先级
- [ ] 定义了兼容等级与最小字段集合
- [ ] 定义了 V1.3 最小覆盖能力集合
- [ ] 定义了失败语义与禁止静默降级规则
- [ ] 与现有能力文档引用关系完整且可解析

---

## 6. 子规格补丁清单（如果需要）

1. `local/local-runtime-boundary-v1.md`：补充“skills compatibility descriptor”读取顺序。
2. `shared/platform-capability-map-v1.md`：增加 ACP vs skills 兼容等级列。
3. `pre-freeze-spec-log.md`：将 skills export 由待补齐项迁移到 V1.3 已立项规格清单。

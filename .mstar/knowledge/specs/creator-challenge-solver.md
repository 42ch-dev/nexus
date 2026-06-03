# Creator Challenge Solver（CLI 侧 — 冻结）

**状态**: Frozen  
**创建**: 2026-04-23  
**平台侧对照**: nexus-platform `v1-spec/platform/creator-agent-registration-v1.md` §3–§4  
**实现仓库**: `nexus`（CLI），**非** `nexus-platform`  
**CLI Spec**: [`cli-spec.md`](./cli-spec.md) §6.2B  
**集成边界**: [`local-cloud-crate-architecture.md`](./local-cloud-crate-architecture.md) §6 — registration via **`nexus-cloud-sync`** (CLI → platform HTTP), **not** daemon Local API.

> **权威说明**：本文档是 CLI 侧 Challenge 解题逻辑的**独立冻结规格**（原 V1.3 程序提取；历史计划见 `.mstar/archived/plans/2026-04-16-v1.3-creator-register-and-residuals.json`）。CLI 实现若有分歧，以本文档为准。平台侧 Challenge 生成与验证逻辑见 nexus-platform `v1-spec/platform/creator-agent-registration-v1.md`。

---

## 0. 概述

Creator 自注册的 challenge-verify 流需要 **CLI 端实现解题逻辑**。平台返回混淆数学题，CLI 内置 solver 自动解析并提交答案，人类用户无需手动操作。

**这不是"调 API 就行"** — CLI 需要完整实现：

1. 调用 `POST /api/v1/creators/register`
2. 解析 `verification.challenge_text`（去混淆 → 还原数学题 → 计算答案）
3. 调用 `POST /api/v1/creators/verify` 提交答案
4. 存储激活后的 `creator_api_key`

---

## 1. CLI 命令

```bash
nexus42 creator register --name "My Agent" [--source cli|web_agent] [--handle my-agent]
```

### 1.1 参数

| 参数 | 必选 | 说明 |
|------|------|------|
| `--name` | 是 | Creator `display_name`（1–100 字符） |
| `--source` | 否 | `registration_source`，默认 `cli` |
| `--handle` | 否 | 人类可读标识符（4–15 字符，`[a-z0-9\-_.]`，设置后不可变） |

### 1.2 输出

成功：
```text
✓ Creator registered
  ID:       clx7abc123def
  Handle:   my-agent (optional)
  Status:   active
```

失败（解题错误）：
```text
✗ Verification failed
  Attempts remaining: 0
  Tip: Re-run `nexus42 creator register` to start a new registration
```

失败（网络/API 错误）：
```text
✗ Registration failed: <error message>
  Hint: Run `nexus42 doctor` to check connectivity
```

---

## 2. 内部流程

```
1. POST {PLATFORM_URL}/api/v1/creators/register
   Body: { display_name, registration_source, handle? }
   ↓
2. 解析响应:
   - 保存 creator_id, creator_api_key (pending)
   - 提取 verification_code, challenge_text, expires_at
   ↓
3. 解题（内置 solver）:
   - 去除噪声符号: stripNoiseSymbols(challenge_text)
   - 还原大小写: normalizeCase(challenge_text)
   - 重组拆分单词: joinHyphenated(challenge_text)
   - 提取数学关系: parseMathProblem(normalizedText)
   - 计算答案: evaluate(problem)
   ↓
4. POST {PLATFORM_URL}/api/v1/creators/verify
   Body: { verification_code, answer }
   ↓
5. 验证成功 → 存储 creator_api_key 到本地凭证库
   验证失败 → 显示错误 + 剩余尝试次数（最多重试 1 次自动）
```

### 2.1 重试策略

- 解题后自动提交一次
- 若第一次 verify 失败，CLI **自动重试 1 次**（同一 `challenge_text` 重新解析，以防实现 bug）
- 若仍失败，终止并提示用户重新 `nexus42 creator register`
- **不自动重试注册**（每次 `register` 会生成新的 challenge）

### 2.2 超时

- `challenge_text` 有 5 分钟有效期（平台侧 `expires_at`）
- CLI 应在收到 challenge 后 **30 秒内** 完成解题+提交
- 若本地处理超时（极端场景），提示用户并建议重试

---

## 3. Solver 实现

### 3.1 策略：纯逻辑优先，LLM fallback

**纯逻辑路径**（v1 主路径）：噪声去除 + 大小写还原 + 数字词映射是确定性字符串操作，不需要 LLM。

**LLM fallback**（v1 可选）：若纯逻辑解析失败（未知的数字词、非常规句式），CLI 可调用 agent runtime 的 LLM 以 `challenge-solver-skill.md` 作为提示词重试。

### 3.2 纯逻辑实现

```typescript
function solveChallenge(challengeText: string): string {
  // Step 1: 去除噪声符号
  const clean = challengeText.replace(/[\]\^\*\|\-~\/\[]/g, '');
  // Step 2: 统一小写
  const lower = clean.toLowerCase();
  // Step 3: 重组拆分单词 (ApPl-Es → apples) — 已在 step 1 去除连字符
  // Step 4: 文字数字 → 数字 (thirty five → 35)
  const withNumbers = replaceWordsWithDigits(lower);
  // Step 5: 提取数学关系
  const problem = extractMathProblem(withNumbers);
  // Step 6: 计算
  return String(evaluate(problem));
}
```

### 3.3 噪声符号清单

平台侧 challenge 生成器注入的噪声字符集：

| 符号 | 说明 |
|------|------|
| `]` `[` | 方括号 |
| `^` | 脱字符 |
| `*` | 星号 |
| `\|` | 管道符 |
| `-` | 连字符（用于拆分单词，如 `ApPl-Es`） |
| `~` | 波浪号 |
| `/` | 斜杠 |

去除规则：`challengeText.replace(/[\]\^\*\|\-~\/\[]/g, '')`

### 3.4 数字词映射

Solver 必须支持的英文数字词（完整列表）：

| 词语 | 数字 |
|------|------|
| zero, one, two, three, four, five | 0–5 |
| six, seven, eight, nine, ten | 6–10 |
| eleven, twelve, thirteen, fourteen, fifteen | 11–15 |
| sixteen, seventeen, eighteen, nineteen | 16–19 |
| twenty, thirty, forty, fifty | 20, 30, 40, 50 |
| sixty, seventy, eighty, ninety | 60, 70, 80, 90 |
| hundred | 100（复合如 "thirty five" = 35） |

复合数字规则：`<tens-word> <ones-word>` → `tens + ones`（如 "thirty five" = 35）。

### 3.5 运算词映射

| 运算词 | 数学运算 |
|--------|---------|
| `adds`, `and`, `more` | `+` |
| `subtracts`, `removes`, `takes` | `-` |
| `multiplies`, `times` | `*` |
| `divides`, `splits` | `/` |

### 3.6 答案规则

- 答案始终为**非负整数**
- 返回纯数字字符串，不含任何解释或格式
- 示例：`"47"` 而非 `"forty-seven"` 或 `"The answer is 47"`

---

## 4. LLM Fallback Skill

文件路径：`packages/cli/src/skills/challenge-solver.md`

```markdown
# Creator Registration Challenge Solver

You are solving an anti-bot verification challenge during Creator registration.

## Input
You receive a `challenge_text` — a math problem disguised with noise.

## Task
1. Remove noise: strip all `]`, `^`, `*`, `|`, `-`, `~`, `/`, `[` symbols
2. Normalize casing: convert all text to lowercase
3. Rejoin hyphenated words: `ApPl-Es` → `apples`
4. Parse the math: extract two numbers and one operation (+, -, *, /)
5. Compute the answer
6. Return ONLY the numeric answer as a string

## Examples

Input:  "A bAs]KeT ^hAs tHiR*tY fI|vE ApPl-Es aNd ^sOmEoNe A*dDs ^TwEl/Ve Mo[Re, hOw MaN~y Ap-PlEs tO|tAl"
Step 1: "A basket has thirty five apples and someone adds twelve more how many apples total"
Step 2: "a basket has thirty five apples and someone adds twelve more how many apples total"
Step 3: "a basket has thirty five apples and someone adds twelve more how many apples total"
Step 4: thirty five = 35, adds = +, twelve = 12
Answer: "47"

## Rules
- Numbers are always spelled out in English words
- Operations: "adds/and/more" = +, "subtracts/removes/takes" = -, "multiplies/times" = *, "divides/splits" = /
- Result is always a non-negative integer
- Return ONLY the number, no explanation
```

---

## 5. 实现模块

| 模块 | 文件（建议） | 职责 |
|------|-------------|------|
| `creator-register.ts` | `packages/cli/src/commands/creator/register.ts` | CLI 命令入口，编排 register→solve→verify |
| `challenge-solver.ts` | `packages/cli/src/lib/challenge-solver.ts` | 噪声去除 + 数学解析（纯逻辑，无需 LLM） |
| `challenge-solver-skill.md` | `packages/cli/src/skills/challenge-solver.md` | LLM 提示词（纯逻辑失败时的 fallback） |
| `register.test.ts` | `packages/cli/src/commands/creator/__tests__/register.test.ts` | 解题逻辑单元测试 |

---

## 6. 两仓协作边界

| 职责 | nexus-platform（平台） | nexus（CLI） |
|------|----------------------|-------------|
| 注册 API | `POST /creators/register` + `POST /creators/verify` | 调用这两个端点 |
| 挑战题生成 | 服务端生成 + 存储（`CreatorVerification`） | — |
| 解题 | — | CLI 内置 solver 解析 + 计算 |
| 凭证签发 | 生成 `creator_api_key` + hash | 存储明文到本地 credential store |
| 账号激活 | `Creator.status` pending → active | — |
| 凭证存储 | — | 写入 `$HOME/.nexus42/creators/<creator_id>/` |

---

## 7. Effort 预估

| 模块 | Effort | 说明 |
|------|--------|------|
| CLI 命令 + 流程编排 | S | 调用 API + 存储凭证 |
| 解题逻辑（纯逻辑） | S | 字符串处理 + 数字词映射 |
| LLM fallback skill | XS | 提示词文件 |
| 测试 | S | 单元测试 + mock API 测试 |
| **合计** | **M** | 约 1–2 个 agent 会话 |

---

## 8. 测试用例

### 8.1 单元测试（Solver 逻辑）

| ID | 输入 | 期望输出 | 说明 |
|----|------|---------|------|
| T01 | `"A bAs]KeT ^hAs tHiR*tY fI\|vE ApPl-Es aNd ^sOmEoNe A*dDs ^TwEl/Ve Mo[Re, hOw MaN~y Ap-PlEs tO\|tAl"` | `"47"` | 标准示例：35 + 12 = 47 |
| T02 | `"^FiVe TiMeS ThReE iS hOw MaNy"` | `"15"` | 乘法：5 × 3 = 15 |
| T03 | `"tWeNtY SuBtRaCtS EiGhT eQuAlS WhAt"` | `"12"` | 减法：20 − 8 = 12 |
| T04 | `"^eiGhtY/foUr iS^eQuAl ~tO?"` | `"20"` | 除法：80 / 4 = 20 |
| T05 | `"ZErO aNd ZeRo Mo[rE"` | `"0"` | 零值边界 |
| T06 | `"one tImEs oNe HuNdReD"` | `"100"` | 复合数字（hundred） |

### 8.2 集成测试

| ID | 场景 | 期望 |
|----|------|------|
| T07 | 注册 → 解题成功 → verify 200 | `creator_api_key` 存入本地，status = active |
| T08 | 解题错误 → verify 400 | 显示错误 + 剩余尝试次数 |
| T09 | 自动重试 1 次后仍失败 | 终止，提示重新注册 |
| T10 | challenge 过期（模拟 `expires_at` 已过） | 提示过期，建议重新注册 |
| T11 | 网络不通 | 显示连接错误 + `nexus42 doctor` 提示 |

---

## 9. 变更历史

| 版本 | 日期 | 说明 |
|------|------|------|
| v1 | 2026-04-23 | 从 V1.3 程序提取为独立冻结 spec |
| v1.1 | 2026-05-20 | 对齐 cloud line：注册走 `nexus-cloud-sync`，修上游链接 |

# Nexus

[![CI](https://github.com/42ch-dev/nexus/actions/workflows/ci.yml/badge.svg)](https://github.com/42ch-dev/nexus/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Node](https://img.shields.io/badge/node-%3E%3D22-brightgreen.svg?logo=nodedotjs&logoColor=white)](package.json)
[![pnpm](https://img.shields.io/badge/pnpm-%3E%3D11-F69220.svg?logo=pnpm&logoColor=white)](package.json)
[![TypeScript](https://img.shields.io/badge/TypeScript-contracts-3178C6.svg?logo=typescript&logoColor=white)](packages/nexus-contracts)
[![Rust](https://img.shields.io/badge/Rust-CLI-DEA584.svg?logo=rust&logoColor=black)](apps/nexus42)
[![Electron](https://img.shields.io/badge/Electron-desktop%20host-47848F.svg?logo=electron&logoColor=white)](apps/desktop-electron)
[![Schema](https://img.shields.io/badge/JSON%20Schema-SSOT-0B7285.svg)](schemas)
[![npm](https://img.shields.io/npm/v/@42ch/nexus-contracts.svg?logo=npm&logoColor=white)](https://www.npmjs.com/package/@42ch/nexus-contracts)
[![Last commit](https://img.shields.io/github/last-commit/42ch-dev/nexus)](https://github.com/42ch-dev/nexus/commits/main)
[![Greptile: The War on Bugs](https://www.greptile.com/badge.svg)](https://www.greptile.com/?utm_source=oss_badge&utm_medium=readme&utm_campaign=greptile_for_open_source)

[English](README.md) · [Concepts](CONCEPTS.md) · [Strategy](STRATEGY.md)

Nexus 是一款本地优先、AI驱动的叙事编排引擎。

## 快速开始

Nexus 目前没有对外的安装包或更新通道 —— 下面的首次运行就是从源码检出开始、当前产品的真实路径。这是公开的 first workflow 示例：干净的隔离 home、真实 `dsh` 运行时与受控的 loopback 模型协议，端到端跑通一次被准入的 workflow —— 同 run 流式输出与回放、一次受授权的 `workspace.commit` 生效、inspect、cancel，以及重启后生效内容保留且不重复提交。

### 前置条件

- **Node.js 22.22 或更高**（`node --version`）
- **pnpm** 11 或更高
- **Rust** stable 工具链（准备步骤会构建 CLI 与 native 附加模块）
- **受支持的 `dsh` 运行时**位于 `PATH`，或用 `DSH_RUNTIME_BIN=/absolute/path/to/dsh` 指定

### 准备构建产物（首次一次，之后仅在 Rust 或 contract 变更后）

沿用现有项目命令 —— 示例自身不会构建或安装任何东西：

```bash
pnpm install
pnpm -F @42ch/nexus-contracts build
pnpm -F @42ch/nexus-native build
pnpm -F @42ch/nexus-provider-acp build
pnpm --dir apps/nexus-service run build         # → apps/nexus-service/dist/main.js
pnpm run build:cli                              # → target/debug/nexus42
node packages/nexus-native/scripts/build.mjs    # → packages/nexus-native-<platform>/native/nexus_core_node.node
```

缺少任一产物、缺少 `nexus42`、或没有可用的 `dsh` 运行时，示例会直接停下（退出码 `2`，`missing_prerequisite`），而不是跳过。

### 运行

```bash
NEXUS42_BIN="$PWD/target/debug/nexus42" node scripts/public-first-workflow.mjs --mode deterministic
```

加 `--json` 输出脱敏的机器可读 receipt（重定向到文件即可留存），加 `--keep` 保留临时隔离根目录以便检查：

```bash
NEXUS42_BIN="$PWD/target/debug/nexus42" node scripts/public-first-workflow.mjs --mode deterministic --json > /tmp/pfw-receipt.json
```

`NEXUS42_BIN` 指向已构建好的二进制且必须是绝对路径；`target/debug/nexus42` 是 Cargo 默认位置，若你设置了 `CARGO_TARGET_DIR` 请改用对应路径。不设置时，驱动会在 `PATH` 中查找 `nexus42`（`dsh` 同样，可由 `DSH_RUNTIME_BIN` 覆盖）。驱动会自建临时根目录（`home/`、`dsh-home/`、`workspace/` 与证据目录各一份），自行分配 service 与模型端口，并通过公开 CLI/HTTP 接口创建 Creator、workspace 与 preset。它不会触碰你的真实 home、不写入产品数据库、也不发起任何非 loopback 网络请求；形如凭据的环境变量会按 **名称** 从子进程环境中剔除，绝不读取其值。

成功的运行退出码为 `0`，且每一步都是 `ok`：fixture、preflight、隔离、service 启停、Creator/workspace/preset 建立、admission、inspect、steer、stream/replay/refusals、sealed 工具拒绝、已提交的 workspace 文件与其 revision、cancel、restart、请求预算 guard 的 `loaded`/`admitted`/`denied`/`spent` 证据，以及两个自有子进程的确认清理。恰好一次被准入的模型请求发往驱动自有的 loopback 端点，因此整个过程不出网、不消耗凭据。

### 可选的 live 模型请求（仅限明确授权）

驱动还有 `--mode live` 路径，会把同一套流程打到唯一固定的官方 HTTPS 模型源（`https://api.deepseek.com/chat/completions`）：

```bash
NEXUS42_BIN="$PWD/target/debug/nexus42" node scripts/public-first-workflow.mjs --mode live \
  --deterministic-receipt /tmp/pfw-receipt.json --attempt-dir /tmp/pfw-attempt-<fresh>
```

该模式 **不属于** 本快速开始，且没有用户针对该次尝试的明确授权时绝不能运行。它会先核验 deterministic receipt 与当前构建产物/运行时一致，然后 **仅按名称** 观察继承来的凭据通道 —— 驱动从不读取、复制、打印或持久化密钥，取值完全交给 sealed 运行时正常的凭据解析器。遇到继承来的模型源覆盖它只会拒绝，不会悄悄剥离。若环境完全没有命名该通道，运行会在分配任何资源或发起请求之前以 `blocked/credentials_unavailable`（退出码 `2`）停下：零准入、不发请求。没有密钥回退、没有重试、没有第二次尝试；若在准入之后出现传输或认证失败，该次授权即被消耗，只有用户再次明确授权才可重试。本示例不会创建、轮换、复制或检查任何凭据。

**状态（2026-09-24）：** 项目唯一一次用户授权的 live 请求已经执行完毕 —— 对官方源恰好 1 次被准入的请求，`outcome: ok`、25 步 `ok`、guard `loaded_dsh 6` / `admitted 1` / `denied 0` / `spent` / `evidence_integrity: complete`、同 run 回放 4/4、提交 revision、重启不重复提交、service 清理确认。该授权已 **用尽**：不应再次运行 live 模式，后续 live 尝试需要用户新的明确授权。live 不是通用的模型消耗通道，也不构成任何已发布版本的声明。

---

## 开发

面向在本 monorepo 中工作的贡献者与维护者。根目录 `package.json` 脚本封装了常用的 `pnpm -F <workspace>` 调用 — 请在仓库根目录执行。

### 环境准备

```bash
git clone https://github.com/42ch/nexus.git
cd nexus
pnpm install
```

前置条件与完整 PR 前检查清单见 [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md)。

### 应用开发服务器

| 命令 | 作用 |
|------|------|
| `pnpm run dev` | CLI + web 本地开发 — 在 manifest/hash/protocol 匹配时复用兼容的 `nexus42` 产物，确保独立 TS service 运行在所选 loopback 端点（默认 127.0.0.1:8420；未运行时以 detached 方式启动），校验 service 健康与身份，然后在前台运行 Vite（`scripts/dev-cli-web.sh`）。没有 daemon 回退；产物缺失或不兼容会以 `pnpm dev:backend:refresh` 快速失败 |
| `pnpm run dev:backend:refresh` | 显式后端刷新 — 唯一会在 Rust/contract 变更后运行 Cargo build/codegen 的常规 DX 路径（`scripts/refresh-dev-backend.mjs`） |
| `pnpm run dev:design-studio` | Design Studio 画廊 — [http://localhost:5174](http://localhost:5174)；无需 TS service |
| `pnpm run dev:web` | Web UI — [http://localhost:5173](http://localhost:5173)；需要先运行独立 TS service（用 `pnpm run dev` 启动，或手动 `node apps/nexus-service/dist/main.js --home <home> --host 127.0.0.1 --port <port>`） |
| `pnpm run dev:desktop` | Electron 桌面端开发 — 宿主加载构建后的 `apps/web` dist（驱动自行构建 TS 依赖闭包与宿主；需已准备的 native payload） |
| `pnpm run dev:desktop:web` | 桌面端 Vite HMR 开发 — Electron 宿主直连 Vite dev origin，不加载构建产物 |

开发快捷命令对接的是**独立 TypeScript service**（`apps/nexus-service/dist/main.js`，通常以 `--home <home> --host 127.0.0.1 --port <port>` 启动）。已退休的 `nexus42 daemon` 组合已删除，因此没有任何 CLI 命令会启动、停止、查询或代理该 service —— 该生命周期归属开发快捷命令与桌面宿主。`pnpm run dev:backend:refresh` 是唯一运行 Cargo 或 codegen 的常规 DX 路径，且仅在 Rust 或 contract 变更后使用。完整的公开 first workflow 示例（真实 `dsh` 与构建产物前置条件）见 [快速开始](#快速开始)。

### 构建

| 命令 | 作用 |
|------|------|
| `pnpm run build` | 构建全部 TS workspace（web、design-studio、contracts、ui、codegen、desktop 宿主；不含打包） |
| `pnpm run build:web` | `apps/web` 生产构建 → `dist/` |
| `pnpm run build:design-studio` | `apps/design-studio` 生产构建 |
| `pnpm run build:desktop` | 未签名 macOS `.app` / `.dmg`（arm64/x64，`-- --arch <arch>`；Electron 打包，不含签名） |
| `pnpm run build:cli` | `nexus42` Debug 构建 |
| `pnpm run build:cli:release` | `nexus42` Release 构建 |

按需构建单个包：

```bash
pnpm -F @42ch/nexus-contracts build
pnpm -F @42ch/nexus-ui build
```

### 测试与类型检查

| 命令 | 作用 |
|------|------|
| `pnpm run test` | 运行所有定义了 `test` 脚本的 workspace 测试 |
| `pnpm run test:web` | Web UI Vitest |
| `pnpm run test:design-studio` | Design Studio Vitest |
| `pnpm run typecheck` | 对定义了 `typecheck` 的 workspace 执行 TypeScript `--noEmit` |

### Schema 与代码生成

| 命令 | 作用 |
|------|------|
| `pnpm run validate-schemas` | 校验 `schemas/` 下全部 JSON Schema |
| `pnpm run codegen` | 从 schema 重新生成 Rust + TypeScript 类型，并重建 `@42ch/nexus-contracts` |
| `pnpm run codegen:watch` | codegen 工具监听模式（改 schema 时用） |

编辑 `schemas/` 后，先跑 `validate-schemas` 再跑 `codegen`，并将生成物与 schema 变更一并提交。完整 PR 前清单见 [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md)。

### 桌面端（Electron）

桌面宿主位于 [`apps/desktop-electron`](apps/desktop-electron)，产出 arm64 与 x64 的未签名 macOS 构建。开发需要已准备的 native payload（`@42ch/nexus-native`）；开发驱动会自行构建 TypeScript 依赖闭包与宿主。

```bash
pnpm run dev:desktop                     # 宿主加载构建后的 apps/web dist
pnpm run dev:desktop:web                 # Vite HMR + Electron 宿主
pnpm run build:desktop -- --arch arm64   # 未签名 .app + .dmg（默认当前架构）
```

`nexus42 desktop bundle --arch <arch>` 转发到同一驱动。不再有 sidecar 拉取步骤 —— 打包步骤自行暂存 service 与 native payload。

### 清理

```bash
pnpm run clean    # 清理 contracts、nexus-ui、codegen 等包的 dist/
```

### Monorepo 布局

| 目录 | 内容 |
|------|------|
| `apps/` | 产品表面 — `nexus42`（Rust CLI）、`desktop-electron`（Electron 桌面宿主）、`web`（浏览器 SPA） |
| `crates/` | 可复用 Rust 库（core 授权、orchestration、local DB、contracts 等） |
| `packages/` | npm 包 — `@42ch/nexus-contracts` 由 `schemas/` 生成 |
| `modules/` | 领域内容（内嵌 presets、WASM 模块、参考数据） |
| `tooling/` | Codegen 流水线与 CI 辅助 |
| `schemas/` | JSON Schema 线上契约 — Rust + TypeScript 类型的单一真相源 |

## 许可证

Apache-2.0

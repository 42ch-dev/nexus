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

TBD — 面向最终用户的体验尚未就绪；请见 [开发](#开发) 从源码构建。

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

开发快捷命令对接的是**独立 TypeScript service**（`apps/nexus-service/dist/main.js`，通常以 `--home <home> --host 127.0.0.1 --port <port>` 启动）。已退休的 `nexus42 daemon` 组合已删除，因此没有任何 CLI 命令会启动、停止、查询或代理该 service —— 该生命周期归属开发快捷命令与桌面宿主。`pnpm run dev:backend:refresh` 是唯一运行 Cargo 或 codegen 的常规 DX 路径，且仅在 Rust 或 contract 变更后使用。端到端 first workflow 驱动（真实 `dsh` 与构建产物前置条件）见 [`scripts/public-first-workflow.mjs`](scripts/public-first-workflow.mjs) —— 开发者示例，非最终用户流程。

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

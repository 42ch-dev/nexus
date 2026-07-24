# Nexus

[![CI](https://github.com/42ch-dev/nexus/actions/workflows/ci.yml/badge.svg)](https://github.com/42ch-dev/nexus/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Node](https://img.shields.io/badge/node-%3E%3D20-brightgreen.svg?logo=nodedotjs&logoColor=white)](package.json)
[![pnpm](https://img.shields.io/badge/pnpm-%3E%3D8-F69220.svg?logo=pnpm&logoColor=white)](package.json)
[![TypeScript](https://img.shields.io/badge/TypeScript-contracts-3178C6.svg?logo=typescript&logoColor=white)](packages/nexus-contracts)
[![Rust](https://img.shields.io/badge/Rust-CLI%20%2B%20daemon-DEA584.svg?logo=rust&logoColor=black)](apps/nexus42)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB.svg?logo=tauri&logoColor=white)](apps/desktop)
[![Schema](https://img.shields.io/badge/JSON%20Schema-SSOT-0B7285.svg)](schemas)
[![npm](https://img.shields.io/npm/v/@42ch/nexus-contracts.svg?logo=npm&logoColor=white)](https://www.npmjs.com/package/@42ch/nexus-contracts)
[![Last commit](https://img.shields.io/github/last-commit/42ch-dev/nexus)](https://github.com/42ch-dev/nexus/commits/main)
[![Greptile: The War on Bugs](https://www.greptile.com/badge.svg)](https://www.greptile.com/?utm_source=oss_badge&utm_medium=readme&utm_campaign=greptile_for_open_source)

[English](README.md) · [Concepts](CONCEPTS.md) · [Strategy](STRATEGY.md)

Nexus 是一款本地优先的创意写作工具。

## 快速开始

> **待补充** — 终端用户安装、首次运行与日常用法。

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
| `pnpm run dev:design-studio` | Design Studio 画廊 — [http://localhost:5174](http://localhost:5174)；无需 daemon |
| `pnpm run dev:web` | Web UI — [http://localhost:5173](http://localhost:5173)；请先启动 daemon（`nexus42 daemon start`） |
| `pnpm run dev:desktop` | Tauri 桌面端开发 — 通过 `tauri.conf.json` 自动启动 web 开发服务 |

### 构建

| 命令 | 作用 |
|------|------|
| `pnpm run build` | 构建全部 TS workspace（**不含** desktop：web、design-studio、contracts、ui、codegen） |
| `pnpm run build:web` | `apps/web` 生产构建 → `dist/` |
| `pnpm run build:design-studio` | `apps/design-studio` 生产构建 |
| `pnpm run build:desktop` | 未签名 macOS `.app` / `.dmg`（含 web 构建 + sidecar + Tauri bundle） |
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

### Desktop sidecar

桌面构建需要在 `apps/desktop/src-tauri/binaries/` 下捆绑 `nexus42` 二进制（全新 clone 时该目录被 gitignore）：

```bash
pnpm run sidecar
```

Intel Mac 请显式指定目标：

```bash
SIDECAR_TARGETS="x86_64-apple-darwin" pnpm run sidecar
```

### 清理

```bash
pnpm run clean    # 清理 contracts、nexus-ui、codegen 等包的 dist/
```

### Monorepo 布局

| 目录 | 内容 |
|------|------|
| `apps/` | 产品表面 — `nexus42`（Rust CLI + daemon）、`desktop`（Tauri 客户端）、`web`（浏览器 SPA） |
| `crates/` | 可复用 Rust 库（daemon runtime、orchestration、local DB、contracts 等） |
| `packages/` | npm 包 — `@42ch/nexus-contracts` 由 `schemas/` 生成 |
| `modules/` | 领域内容（内嵌 presets、WASM 模块、参考数据） |
| `tooling/` | Codegen 流水线与 CI 辅助 |
| `schemas/` | JSON Schema 线上契约 — Rust + TypeScript 类型的单一真相源 |

## 许可证

Apache-2.0

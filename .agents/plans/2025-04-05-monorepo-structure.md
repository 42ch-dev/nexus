# Monorepo Structure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the foundational monorepo directory structure for Nexus, establishing Rust workspace, TypeScript workspace, and core directories (schemas, crates, packages, tooling, docs) with appropriate configuration files.

**Architecture:** Single monorepo with clearly separated layers: schemas/ (truth source), crates/ (Rust implementation), packages/ (TypeScript contracts), tooling/ (codegen scripts), docs/ (user documentation). Rust workspace uses Cargo.toml with member crates; TypeScript workspace uses package.json with workspaces field.

**Tech Stack:** Rust 1.75+, Node.js 20+, pnpm workspaces, Cargo workspaces

---

## Files to Create/Modify

**Create:**
- `Cargo.toml` (root workspace config)
- `package.json` (root workspace config)
- `pnpm-workspace.yaml` (pnpm workspace config)
- `schemas/.gitkeep` (empty directory marker)
- `crates/.gitkeep`
- `packages/.gitkeep`
- `tooling/.gitkeep`
- `docs/.gitkeep`
- `.github/workflows/.gitkeep`
- `crates/nexus-contracts/Cargo.toml` (placeholder crate)
- `packages/nexus-contracts/package.json` (placeholder package)

---

## Task 1: Create Core Directory Structure

**Files:**
- Create: `schemas/.gitkeep`, `crates/.gitkeep`, `packages/.gitkeep`, `tooling/.gitkeep`, `docs/.gitkeep`, `.github/workflows/.gitkeep`

- [x] **Step 1: Create schemas directory with gitkeep marker**

Run: `mkdir -p schemas && touch schemas/.gitkeep`

Expected: Directory created successfully, empty `.gitkeep` file exists

- [x] **Step 2: Create crates directory with gitkeep marker**

Run: `mkdir -p crates && touch crates/.gitkeep`

Expected: Directory created successfully

- [x] **Step 3: Create packages directory with gitkeep marker**

Run: `mkdir -p packages && touch packages/.gitkeep`

Expected: Directory created successfully

- [x] **Step 4: Create tooling directory with gitkeep marker**

Run: `mkdir -p tooling && touch tooling/.gitkeep`

Expected: Directory created successfully

- [x] **Step 5: Create docs directory with gitkeep marker**

Run: `mkdir -p docs && touch docs/.gitkeep`

Expected: Directory created successfully

- [x] **Step 6: Create GitHub workflows directory**

Run: `mkdir -p .github/workflows && touch .github/workflows/.gitkeep`

Expected: Directory created successfully

- [x] **Step 7: Verify directory structure**

Run: `ls -la schemas crates packages tooling docs .github/workflows`

Expected: All directories exist with `.gitkeep` files

- [x] **Step 8: Commit directory structure**

Run: `git add schemas crates packages tooling docs .github && git commit -m "feat: create monorepo directory structure"`

Expected: Commit successful

---

## Task 2: Initialize Rust Workspace

**Files:**
- Create: `Cargo.toml` (root)
- Create: `crates/nexus-contracts/Cargo.toml` (placeholder)

- [x] **Step 1: Create root Cargo.toml with workspace configuration**

Create file: `Cargo.toml`

```toml
[workspace]
members = [
    "crates/nexus-contracts",
    # Future members (commented out for now):
    # "crates/nexus42",
    # "crates/nexus42d",
    # "crates/nexus-sync",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["42ch"]
license = "MIT"
repository = "https://github.com/42ch/nexus"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
tokio = { version = "1.35", features = ["full"] }
```

Expected: File created with workspace configuration

- [x] **Step 2: Create placeholder nexus-contracts crate**

Run: `mkdir -p crates/nexus-contracts/src && touch crates/nexus-contracts/src/lib.rs`

Expected: Crate directory structure created

- [x] **Step 3: Create nexus-contracts Cargo.toml**

Create file: `crates/nexus-contracts/Cargo.toml`

```toml
[package]
name = "nexus-contracts"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
# Will add test dependencies later
```

Expected: Crate manifest created

- [x] **Step 4: Add minimal lib.rs placeholder**

Create file: `crates/nexus-contracts/src/lib.rs`

```rust
//! Nexus Wire Contracts (Generated from JSON Schema)
//!
//! This crate contains type definitions generated from `schemas/` JSON Schema files.
//! All wire types are auto-generated - do not modify manually.

pub mod placeholder;

// Placeholder for future generated types
pub struct PlaceholderContract {
    pub schema_version: String,
}
```

Create file: `crates/nexus-contracts/src/placeholder.rs`

```rust
//! Placeholder module until schema codegen is implemented

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceholderContract {
    pub schema_version: String,
}

impl Default for PlaceholderContract {
    fn default() -> Self {
        Self {
            schema_version: "0.1.0".to_string(),
        }
    }
}
```

Expected: Source files created

- [x] **Step 5: Verify Cargo workspace resolves**

Run: `cargo metadata --format-version 1 | head -20`

Expected: Cargo workspace resolves without errors, shows `nexus-contracts` member

- [x] **Step 6: Commit Rust workspace setup**

Run: `git add Cargo.toml crates/nexus-contracts && git commit -m "feat: initialize Rust workspace with nexus-contracts placeholder"`

Expected: Commit successful

---

## Task 3: Initialize TypeScript Workspace

**Files:**
- Create: `package.json` (root)
- Create: `pnpm-workspace.yaml`
- Create: `packages/nexus-contracts/package.json` (placeholder)

- [x] **Step 1: Create root package.json with workspaces configuration**

Create file: `package.json`

```json
{
  "name": "nexus-monorepo",
  "version": "0.1.0",
  "private": true,
  "description": "Nexus open-source monorepo - CLI, daemon, and wire contracts",
  "author": "42ch",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "https://github.com/42ch/nexus"
  },
  "workspaces": [
    "packages/*"
  ],
  "engines": {
    "node": ">=20.0.0",
    "pnpm": ">=8.0.0"
  },
  "devDependencies": {},
  "scripts": {
    "build": "pnpm -r run build",
    "test": "pnpm -r run test",
    "lint": "pnpm -r run lint",
    "clean": "pnpm -r run clean"
  }
}
```

Expected: Root package.json created with workspaces field

- [x] **Step 2: Create pnpm-workspace.yaml**

Create file: `pnpm-workspace.yaml`

```yaml
packages:
  - 'packages/*'
```

Expected: pnpm workspace config created

- [x] **Step 3: Create placeholder nexus-contracts package**

Run: `mkdir -p packages/nexus-contracts/src && touch packages/nexus-contracts/src/index.ts`

Expected: Package directory structure created

- [x] **Step 4: Create nexus-contracts package.json**

Create file: `packages/nexus-contracts/package.json`

```json
{
  "name": "@42ch/nexus-contracts",
  "version": "0.1.0",
  "description": "Nexus Wire Contracts - TypeScript types generated from JSON Schema",
  "author": "42ch",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "https://github.com/42ch/nexus"
  },
  "main": "./dist/index.js",
  "types": "./dist/index.d.ts",
  "module": "./dist/index.mjs",
  "exports": {
    ".": {
      "import": "./dist/index.mjs",
      "require": "./dist/index.js",
      "types": "./dist/index.d.ts"
    }
  },
  "files": [
    "dist",
    "src"
  ],
  "scripts": {
    "build": "tsup src/index.ts --format cjs,esm --dts",
    "dev": "tsup src/index.ts --format cjs,esm --dts --watch",
    "clean": "rm -rf dist",
    "typecheck": "tsc --noEmit"
  },
  "devDependencies": {
    "typescript": "^5.3.0",
    "tsup": "^8.0.0"
  },
  "peerDependencies": {},
  "keywords": [
    "nexus",
    "contracts",
    "wire-types",
    "json-schema"
  ]
}
```

Expected: Package manifest created

- [x] **Step 5: Create TypeScript placeholder types**

Create file: `packages/nexus-contracts/src/index.ts`

```typescript
/**
 * Nexus Wire Contracts (Generated from JSON Schema)
 *
 * This package contains TypeScript type definitions generated from `schemas/` JSON Schema files.
 * All wire types are auto-generated - do not modify manually.
 */

export interface PlaceholderContract {
  schema_version: string;
}

export const SCHEMA_VERSION = "0.1.0";

export function createPlaceholderContract(): PlaceholderContract {
  return {
    schema_version: SCHEMA_VERSION,
  };
}
```

Expected: TypeScript source file created

- [x] **Step 6: Create TypeScript config**

Create file: `packages/nexus-contracts/tsconfig.json`

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "lib": ["ES2022"],
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```

Expected: TypeScript configuration created

- [x] **Step 7: Initialize pnpm and install dependencies**

Run: `pnpm install`

Expected: pnpm installs dependencies, workspace resolves correctly

- [x] **Step 8: Verify TypeScript workspace**

Run: `pnpm list --depth 0`

Expected: Shows `@42ch/nexus-contracts` package in workspace

- [x] **Step 9: Commit TypeScript workspace setup**

Run: `git add package.json pnpm-workspace.yaml packages/nexus-contracts && git commit -m "feat: initialize TypeScript workspace with @42ch/nexus-contracts placeholder"`

Expected: Commit successful

---

## Task 4: Add Build Configuration and CI Skeleton

**Files:**
- Create: `.gitignore` (update)
- Create: `.github/workflows/ci.yml` (basic CI skeleton)

- [x] **Step 1: Update .gitignore for monorepo**

Read current `.gitignore`, then append:

```gitignore
# Rust build artifacts
/target/
**/*.rs.bk
*.pdb

# Node/pnpm
node_modules/
.pnpm-store/
.pnpm-debug.log
dist/
*.tsbuildinfo

# Generated files (will be regenerated by codegen)
packages/nexus-contracts/src/generated/
crates/nexus-contracts/src/generated/

# Environment files
.env
.env.local
.env.*.local

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db
```

Run: `cat .gitignore`

Expected: `.gitignore` contains monorepo-specific patterns

- [x] **Step 2: Create basic CI workflow skeleton**

Create file: `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  validate-schemas:
    name: Validate JSON Schemas
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check schemas directory exists
        run: test -d schemas || echo "schemas/ not yet populated"

  rust-checks:
    name: Rust fmt & clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: Check formatting
        run: cargo fmt --all -- --check
      - name: Run clippy
        run: cargo clippy --all -- -D warnings

  typescript-checks:
    name: TypeScript typecheck
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v2
        with:
          version: 8
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'pnpm'
      - name: Install dependencies
        run: pnpm install
      - name: Typecheck
        run: pnpm run typecheck
```

Expected: CI workflow created with placeholder jobs

- [x] **Step 3: Commit CI configuration**

Run: `git add .gitignore .github/workflows/ci.yml && git commit -m "feat: add monorepo gitignore rules and CI skeleton"`

Expected: Commit successful

---

## Task 5: Create Root Documentation Files

**Files:**
- Create: `README.md` (update existing placeholder)
- Create: `docs/CONTRIBUTING.md`
- Create: `docs/ARCHITECTURE.md`

- [x] **Step 1: Update README.md with monorepo overview**

Replace current `README.md` content with:

```markdown
# Nexus

**Open-source monorepo** for the Nexus CLI, daemon, and wire contracts.

## Repository Structure

```
schemas/                # JSON Schema truth source (codegen input)
crates/
  nexus-contracts/      # Generated Rust types
  nexus42/              # CLI binary (future)
  nexus42d/             # Daemon (future)
  nexus-sync/           # Bundle/outbox state machine (future)
packages/
  nexus-contracts/      # Generated TypeScript wire types (npm package)
tooling/
  codegen/              # Schema → TS + Rust pipeline (future)
docs/                   # User docs (installation, sync, troubleshooting)
.github/workflows/      # CI: schema validation, Rust fmt/clippy/test, npm publish
```

## Development

### Prerequisites

- Rust 1.75+ (stable)
- Node.js 20+
- pnpm 8+

### Setup

```bash
# Install Rust dependencies
cargo build

# Install Node dependencies
pnpm install
```

### Build

```bash
# Build all Rust crates
cargo build --all

# Build all TypeScript packages
pnpm run build
```

## Wire Contracts

**JSON Schema as truth source** - All wire types are generated from `schemas/`:

- TypeScript: `packages/nexus-contracts/src/generated/` (published to npm as `@42ch/nexus-contracts`)
- Rust: `crates/nexus-contracts/src/generated/` (published to crates.io as `nexus-contracts`)

**Do not modify generated types manually.** Update schemas, then run codegen.

## License

MIT
```

Expected: README updated with monorepo documentation

- [x] **Step 2: Create CONTRIBUTING.md**

Create file: `docs/CONTRIBUTING.md`

```markdown
# Contributing to Nexus

## Development Workflow

### Schema-First Development

1. **Define schema** in `schemas/` (JSON Schema format)
2. **Run codegen** to generate TS + Rust types
3. **Implement features** using generated contracts
4. **Write tests** for implementation
5. **Commit** schema changes + generated code together

### Branch Strategy

- **Phase 0** (initial setup): Commits to `main`
- **Phase 1+** (feature development): Feature branches from `main`
  - `feature/<feature-name>` for new features
  - `fix/<bug-name>` for bug fixes

### Code Style

**Rust:**
- Run `cargo fmt` before committing
- Run `cargo clippy --all` and fix warnings

**TypeScript:**
- Use strict TypeScript (`strict: true` in tsconfig)
- Run `pnpm run typecheck` before committing

### Testing

- All Rust crates must have unit tests
- All TypeScript packages must have type tests
- Integration tests for critical paths

## Pull Request Process

1. Ensure all CI checks pass
2. Update documentation if needed
3. Add tests for new functionality
4. Keep PRs focused - one feature/fix per PR

## Code of Conduct

Be respectful, constructive, and inclusive.
```

Expected: CONTRIBUTING guide created

- [x] **Step 3: Create ARCHITECTURE.md**

Create file: `docs/ARCHITECTURE.md`

```markdown
# Nexus Architecture

## Monorepo Architecture

### Truth Source: JSON Schema

All wire contracts defined in `schemas/` directory.

**Code Generation Flow:**
```
schemas/*.json → codegen → Rust (crates/nexus-contracts) + TypeScript (packages/nexus-contracts)
```

**Why JSON Schema?**
- Single source of truth for DTOs
- Automatic type generation for both languages
- Version-locked contracts (`schema_version` field)
- Easy validation and testing

### Rust Workspace

**Members:**
- `nexus-contracts`: Generated wire types (library crate)
- `nexus42` (future): CLI executable
- `nexus42d` (future): Daemon/supervisor
- `nexus-sync` (future): Bundle/outbox state machine

**Design Principles:**
- Use official ACP Rust SDK
- Share generated contract types
- Client-only (not ACP agent/server)

### TypeScript Workspace

**Packages:**
- `@42ch/nexus-contracts`: Generated wire types (npm package)

**Design Principles:**
- Consumed by private `nexus-platform` repo
- No handwritten second DTO source
- All types come from this repo's schemas

## Versioning

- Schema contracts use `schema_version` field
- CLI SemVer reflects breaking wire changes
- npm package major bump → coordinated update

## Constraints

- **Do not** treat `nexus42d` as ACP Agent/Server - it's client-only
- **Do not** sync full manuscript text by default - only deltas/bundles
- **World history is immutable** - changes via Fork only
- **Wire contracts must match schemas** - no drift
```

Expected: Architecture documentation created

- [x] **Step 4: Commit documentation**

Run: `git add README.md docs && git commit -m "docs: add monorepo documentation (README, CONTRIBUTING, ARCHITECTURE)"`

Expected: Commit successful

---

## Verification

- [x] **Final verification: Check complete monorepo structure**

Run: `tree -L 2 -a`

Expected output structure:
```
.
├── .agents
├── .git
├── .github
│   └── workflows
│       ├── .gitkeep
│       └── ci.yml
├── .gitignore
├── Cargo.toml
├── LICENSE
├── README.md
├── package.json
├── pnpm-workspace.yaml
├── crates
│   ├── .gitkeep
│   └── nexus-contracts
│       ├── Cargo.toml
│       └── src
│           ├── lib.rs
│           └── placeholder.rs
├── docs
│   ├── .gitkeep
│   ├── ARCHITECTURE.md
│   └── CONTRIBUTING.md
├── packages
│   ├── .gitkeep
│   └── nexus-contracts
│       ├── package.json
│       ├── src
│       │   └── index.ts
│       └── tsconfig.json
├── schemas
│   └── .gitkeep
└── tooling
    └── .gitkeep
```

- [x] **Verify Rust workspace resolves**

Run: `cargo check --workspace`

Expected: No errors, workspace compiles successfully

- [x] **Verify TypeScript workspace resolves**

Run: `pnpm run typecheck`

Expected: TypeScript typecheck passes

---

## Completion

After all tasks complete:
- [ ] Update `.agents/plans/status.json` with completion status
- [ ] Create git tag: `git tag v0.1.0-structure -a -m "Phase 0: Monorepo structure initialized"`
- [ ] Push to remote: `git push origin main --tags`

---

**Plan saved to:** `.agents/plans/2025-04-05-monorepo-structure.md`
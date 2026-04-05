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

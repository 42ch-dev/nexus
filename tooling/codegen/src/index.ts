import { execSync } from 'node:child_process';
import path from 'node:path';
import { runPrep } from './schema-prep';
import { generateTSTypes } from './ts-gen';
import { logger } from './utils';

/**
 * Main codegen orchestrator.
 *
 * Pipeline:
 *   1. prep     — localize + dereference `schemas/` → `.schemas-dereferenced/`
 *                 (shared by TS + Rust generation)
 *   2. ts-gen   — TypeScript via `json-schema-to-typescript` over the localized tree
 *   3. rust-gen — Rust wire types via the typify-based `nexus-rust-gen` binary,
 *                 consuming the dereferenced tree produced by stage 1.
 */
export async function runCodegen(): Promise<void> {
  logger.info('Starting Nexus Codegen Pipeline');
  logger.info('==============================');

  // Stage 1: schema prep (localize + dereference)
  const prepPaths = await runPrep();

  // Stage 2: TypeScript types → packages/nexus-contracts/src/generated/
  logger.info('\n--- Generating TypeScript Types ---');
  await generateTSTypes();

  // Stage 3: Rust types → crates/nexus-contracts/src/generated/
  // Invokes the external `nexus-rust-gen` binary (typify) over the dereferenced tree
  // from stage 1. It is an isolated workspace (tooling/codegen/rust-gen, excluded from
  // the root `[workspace]`) so it is built independently here via `cargo run --release`.
  logger.info('\n--- Generating Rust Types ---');
  execSync('cargo run --quiet --release', {
    cwd: path.join(prepPaths.repoRoot, 'tooling', 'codegen', 'rust-gen'),
    env: {
      ...process.env,
      NEXUS_REPO_ROOT: prepPaths.repoRoot,
      NEXUS_DEREF_SCHEMAS_DIR: prepPaths.derefSchemasDir,
      NEXUS_SRC_SCHEMAS_DIR: prepPaths.srcSchemasDir,
    },
    stdio: 'inherit',
  });

  logger.success('\n✓ Codegen complete');
}

// Run if executed directly
// eslint-disable-next-line @typescript-eslint/no-require-imports
if (typeof require !== 'undefined' && require.main === module) {
  runCodegen().catch((err: Error) => {
    logger.error(`Codegen failed: ${err.message}`);
    if (process.env.DEBUG) {
      console.error(err);
    }
    process.exit(1);
  });
}

import { loadAllSchemas } from './schema-loader';
import { generateRustTypes } from './rust-generator';
import { runPrep } from './schema-prep';
import { generateTSTypes } from './ts-gen';
import { logger } from './utils';

/**
 * Main codegen orchestrator.
 *
 * Pipeline:
 *   1. prep     — localize + dereference `schemas/` (shared by TS + Rust generation)
 *   2. ts-gen   — TypeScript via `json-schema-to-typescript` over the localized tree
 *   3. rust-gen — Rust wire types
 *
 * Stage 3 currently uses the legacy hand-tuned generator. P1 will replace it with a
 * typify-based generator that consumes the dereferenced tree (`.schemas-dereferenced/`,
 * produced by stage 1) — see `schema-prep.ts` `buildDereferencedSchemaTree`.
 */
export async function runCodegen(): Promise<void> {
  logger.info('Starting Nexus Codegen Pipeline');
  logger.info('==============================');

  // Stage 1: schema prep (localize + dereference)
  await runPrep();

  // Stage 2: TypeScript types → packages/nexus-contracts/src/generated/
  logger.info('\n--- Generating TypeScript Types ---');
  await generateTSTypes();

  // Stage 3: Rust types → crates/nexus-contracts/src/generated/
  // NOTE(P1): legacy generator below; will be superseded by typify over the deref tree.
  logger.info('\n--- Generating Rust Types ---');
  const schemas = loadAllSchemas();
  generateRustTypes(schemas);

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

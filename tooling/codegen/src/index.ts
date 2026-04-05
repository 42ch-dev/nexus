import { loadAllSchemas, validateSchemaStructure } from './schema-loader';
import { generateTSTypes } from './ts-generator';
import { generateRustTypes } from './rust-generator';
import { logger } from './utils';

/**
 * Main codegen orchestrator.
 *
 * Runs full pipeline:
 * 1. Load all schemas from schemas/
 * 2. Validate schema structure
 * 3. Generate TypeScript types → packages/nexus-contracts/src/generated/
 * 4. Generate Rust types → crates/nexus-contracts/src/generated/
 */
export async function runCodegen(): Promise<void> {
  logger.info('Starting Nexus Codegen Pipeline');
  logger.info('==============================');

  // Step 1: Load schemas
  const schemas = loadAllSchemas();

  if (schemas.length === 0) {
    logger.error('No schemas to generate');
    process.exit(1);
  }

  // Step 2: Validate schemas
  logger.info('Validating schemas...');
  const invalidSchemas = schemas.filter(s => !validateSchemaStructure(s));

  if (invalidSchemas.length > 0) {
    logger.error(`Found ${invalidSchemas.length} invalid schemas`);
    process.exit(1);
  }
  logger.success(`All ${schemas.length} schemas valid`);

  // Step 3: Generate TypeScript types
  logger.info('\n--- Generating TypeScript Types ---');
  generateTSTypes(schemas);

  // Step 4: Generate Rust types
  logger.info('\n--- Generating Rust Types ---');
  generateRustTypes(schemas);

  logger.success('\n✓ Codegen complete');
  logger.info(`Processed ${schemas.length} schemas → TypeScript + Rust`);
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

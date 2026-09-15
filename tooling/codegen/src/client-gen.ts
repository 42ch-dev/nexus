/**
 * Emit CoreSliceClient method declarations from core-service-operations metadata.
 */
import fs from 'fs';
import path from 'path';
import { resolveFromRoot, logger, readJSON } from './utils';

const BANNER = `/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/core/core-service-operations.schema.json
 * Generator: tooling/codegen/src/client-gen.ts
 */`;

interface OperationArg {
  name: string;
  type: string;
  optional?: boolean;
}

interface Operation {
  name: string;
  args?: OperationArg[];
  returns: string;
  async_iterable?: boolean;
}

interface OperationsDoc {
  operations: Operation[];
}

const IMPORT_TYPES = [
  'WorldKbGraphResponse',
  'WorldKbPatchEntityRequest',
  'WorldKbPatchEntityResponse',
  'WorldKbCandidatesResponse',
  'CoreChangesRequest',
  'CoreChangesResponse',
  'CreateSessionRequest',
  'AgentHostListSessionsQuery',
  'SessionListResponse',
  'SessionResponse',
  'ShutdownSessionResponse',
  'ExecuteOperationRequest',
  'OperationResponse',
  'CancelOperationResponse',
  'ProviderHostEvent',
];

function formatArgs(args: OperationArg[] | undefined): string {
  if (!args?.length) return '';
  return args
    .map(arg => `${arg.name}${arg.optional ? '?' : ''}: ${arg.type}`)
    .join(', ');
}

type OperationsSchema = OperationsDoc & {
  properties?: { operations?: { default?: Operation[] } };
};

export function generateCoreSliceClient(): void {
  const schemaPath = resolveFromRoot('schemas', 'core', 'core-service-operations.schema.json');
  const doc = readJSON(schemaPath) as OperationsSchema;
  const operations = doc.operations ?? doc.properties?.operations?.default;
  if (!operations?.length) {
    throw new Error(`no operations metadata in ${schemaPath}`);
  }
  const outPath = resolveFromRoot(
    'packages',
    'nexus-contracts',
    'src',
    'generated',
    'core',
    'CoreSliceClient.ts',
  );
  fs.mkdirSync(path.dirname(outPath), { recursive: true });

  const lines: string[] = [
    BANNER,
    '',
    `import type { ${IMPORT_TYPES.join(', ')} } from '../index';`,
    "import type { CoreStreamGap } from './provider-event-batch';",
    '',
  ];
  lines.push('export interface CoreSliceClient {');
  for (const op of operations) {
    const args = formatArgs(op.args);
    if (op.async_iterable) {
      lines.push(`  ${op.name}(${args}): AsyncIterable<${op.returns}>;`);
    } else {
      lines.push(`  ${op.name}(${args}): Promise<${op.returns}>;`);
    }
  }
  lines.push('}');
  lines.push('');
  fs.writeFileSync(outPath, lines.join('\n'));
  logger.success(`Generated CoreSliceClient → ${outPath}`);

  // `CoreSliceClient` is a hand-declared interface (not a schema-derived type),
  // so the ts-gen subdir barrel cannot emit it. Append the re-export here,
  // after the barrel exists and idempotently, so `@42ch/nexus-contracts` can
  // satisfy the locked `NexusClient extends CoreSliceClient` contract.
  const barrelPath = path.join(path.dirname(outPath), 'index.ts');
  const exportLine = "export type { CoreSliceClient } from './CoreSliceClient';";
  const barrel = fs.readFileSync(barrelPath, 'utf8');
  if (!barrel.includes(exportLine)) {
    fs.writeFileSync(barrelPath, `${barrel.replace(/\n*$/, '')}\n${exportLine}\n`);
    logger.success(`Exported CoreSliceClient from ${barrelPath}`);
  }
}

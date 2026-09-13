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
  'CoreStreamGap',
];

function formatArgs(args: OperationArg[] | undefined): string {
  if (!args?.length) return '';
  return args
    .map(arg => `${arg.name}${arg.optional ? '?' : ''}: ${arg.type}`)
    .join(', ');
}

/** Locked operation table — must match schemas/core/core-service-operations.schema.json default. */
const OPERATIONS: Operation[] = [
  {
    name: 'getWorldKbGraph',
    args: [
      { name: 'worldId', type: 'string' },
      { name: 'query', type: '{ includeSuggested?: boolean }', optional: true },
    ],
    returns: 'WorldKbGraphResponse',
  },
  {
    name: 'worldKbPatchEntity',
    args: [
      { name: 'worldId', type: 'string' },
      { name: 'request', type: 'WorldKbPatchEntityRequest' },
    ],
    returns: 'WorldKbPatchEntityResponse',
  },
  {
    name: 'getWorldKbCandidates',
    args: [
      { name: 'worldId', type: 'string' },
      { name: 'query', type: '{ limit?: number; cursor?: string }', optional: true },
    ],
    returns: 'WorldKbCandidatesResponse',
  },
  {
    name: 'getCoreChanges',
    args: [{ name: 'request', type: 'CoreChangesRequest' }],
    returns: 'CoreChangesResponse',
  },
  {
    name: 'createAgentHostSession',
    args: [{ name: 'request', type: 'CreateSessionRequest' }],
    returns: 'SessionResponse',
  },
  {
    name: 'listAgentHostSessions',
    args: [{ name: 'query', type: 'AgentHostListSessionsQuery', optional: true }],
    returns: 'SessionListResponse',
  },
  {
    name: 'getAgentHostSession',
    args: [{ name: 'sessionId', type: 'string' }],
    returns: 'SessionResponse',
  },
  {
    name: 'shutdownAgentHostSession',
    args: [{ name: 'sessionId', type: 'string' }],
    returns: 'ShutdownSessionResponse',
  },
  {
    name: 'executeAgentHostOperation',
    args: [
      { name: 'sessionId', type: 'string' },
      { name: 'request', type: 'ExecuteOperationRequest' },
    ],
    returns: 'OperationResponse',
  },
  {
    name: 'getAgentHostOperation',
    args: [{ name: 'operationId', type: 'string' }],
    returns: 'OperationResponse',
  },
  {
    name: 'cancelAgentHostOperation',
    args: [{ name: 'operationId', type: 'string' }],
    returns: 'CancelOperationResponse',
  },
  {
    name: 'subscribeAgentHostEvents',
    args: [
      { name: 'sessionId', type: 'string' },
      { name: 'signal', type: 'AbortSignal' },
    ],
    returns: 'ProviderHostEvent | CoreStreamGap',
    async_iterable: true,
  },
];

export function generateCoreSliceClient(): void {
  const operations = OPERATIONS;
  const outPath = resolveFromRoot(
    'packages',
    'nexus-contracts',
    'src',
    'generated',
    'core',
    'CoreSliceClient.ts',
  );
  fs.mkdirSync(path.dirname(outPath), { recursive: true });

  const lines: string[] = [BANNER, '', `import type { ${IMPORT_TYPES.join(', ')} } from '../index';`, ''];
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
}

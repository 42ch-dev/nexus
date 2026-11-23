/**
 * `{home}/agent-host/config.toml` is owned by the native core
 * (`load_config` / `save_config`); this family is the HTTP seam only. GET
 * returns the native defaults when the file is missing, PUT forwards the full
 * document to the native save path and returns the persisted document, so the
 * readiness probe at the next owner start reads exactly what the client wrote.
 */
import type { AgentHostConfig } from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import { HttpError, mapNativeError } from './errors.js';
import type { DomainRoute } from './routes.js';
import { withSelectionLock } from './workspaces.js';

const PROTOCOL_ACP = 'acp';
const PROTOCOL_NATIVE_CLI = 'native_cli';

/**
 * Allowed-key sets, derived from the generated contract: the `satisfies`
 * clauses make a schema/codegen rename or addition fail the validator's
 * compile instead of silently rejecting (or accepting) the new field. Exported
 * for the schema drift test.
 */
export const CONFIG_KEYS = [
  'max_sessions',
  'max_ops_per_session',
  'timeouts',
  'policy',
  'providers',
] as const satisfies readonly (keyof AgentHostConfig)[];
export const TIMEOUT_KEYS = [
  'launch_ms',
  'initialize_ms',
  'session_ms',
  'prompt_ms',
  'shutdown_ms',
] as const satisfies readonly (keyof NonNullable<AgentHostConfig['timeouts']>)[];
export const POLICY_KEYS = [
  'unknown_provider',
  'unknown_tool_risk',
  'allow_model_fallback',
] as const satisfies readonly (keyof NonNullable<AgentHostConfig['policy']>)[];
export const PROVIDER_KEYS = [
  'id',
  'protocol',
  'command',
  'args',
  'env',
  'enabled',
] as const satisfies readonly (keyof AgentHostConfig['providers'][number])[];

function invalidInput(message: string): HttpError {
  return new HttpError(400, 'invalid_input', message);
}

function requireRecord(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw invalidInput(`${path} must be an object`);
  }
  return value as Record<string, unknown>;
}

/** Schema `additionalProperties: false` semantics at every object level. */
function rejectUnknownKeys(
  value: Record<string, unknown>,
  allowed: readonly string[],
  path: string,
): void {
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) throw invalidInput(`unknown field '${key}' at ${path}`);
  }
}

function requirePositiveInteger(value: unknown, path: string): void {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 1) {
    throw invalidInput(`${path} must be an integer >= 1`);
  }
}

function parseAgentHostConfig(body: unknown): AgentHostConfig {
  const config = requireRecord(body, 'config');
  rejectUnknownKeys(config, CONFIG_KEYS, 'config');

  if (config.max_sessions !== undefined) {
    requirePositiveInteger(config.max_sessions, 'max_sessions');
  }
  if (config.max_ops_per_session !== undefined) {
    requirePositiveInteger(config.max_ops_per_session, 'max_ops_per_session');
  }

  if (config.timeouts !== undefined) {
    const timeouts = requireRecord(config.timeouts, 'timeouts');
    rejectUnknownKeys(timeouts, TIMEOUT_KEYS, 'timeouts');
    for (const key of Object.keys(timeouts)) {
      requirePositiveInteger(timeouts[key], `timeouts.${key}`);
    }
  }

  if (config.policy !== undefined) {
    const policy = requireRecord(config.policy, 'policy');
    rejectUnknownKeys(policy, POLICY_KEYS, 'policy');
    if (
      policy.unknown_provider !== undefined &&
      policy.unknown_provider !== 'deny' &&
      policy.unknown_provider !== 'allow'
    ) {
      throw invalidInput('policy.unknown_provider must be "deny" or "allow"');
    }
    if (
      policy.unknown_tool_risk !== undefined &&
      policy.unknown_tool_risk !== 'deny' &&
      policy.unknown_tool_risk !== 'ask' &&
      policy.unknown_tool_risk !== 'allow'
    ) {
      throw invalidInput('policy.unknown_tool_risk must be "deny", "ask" or "allow"');
    }
    if (policy.allow_model_fallback !== undefined && typeof policy.allow_model_fallback !== 'boolean') {
      throw invalidInput('policy.allow_model_fallback must be a boolean');
    }
  }

  const { providers } = config;
  if (!Array.isArray(providers)) {
    throw invalidInput('config must be an object with a providers array');
  }
  for (const [index, provider] of providers.entries()) {
    const path = `providers[${index}]`;
    const record = requireRecord(provider, path);
    rejectUnknownKeys(record, PROVIDER_KEYS, path);
    const { id, protocol, command, args, env, enabled } = record;
    if (typeof id !== 'string' || id.trim() === '') {
      throw invalidInput(`${path}.id is required`);
    }
    if (protocol !== PROTOCOL_ACP && protocol !== PROTOCOL_NATIVE_CLI) {
      throw invalidInput(`${path}.protocol must be "acp" or "native_cli"`);
    }
    if (command !== undefined && command !== null && typeof command !== 'string') {
      throw invalidInput(`${path}.command must be a string or null`);
    }
    if (args !== undefined) {
      if (!Array.isArray(args) || args.some((arg) => typeof arg !== 'string')) {
        throw invalidInput(`${path}.args must be an array of strings`);
      }
    }
    if (env !== undefined) {
      const envRecord = requireRecord(env, `${path}.env`);
      for (const [key, value] of Object.entries(envRecord)) {
        if (typeof value !== 'string') {
          throw invalidInput(`${path}.env.${key} must be a string`);
        }
      }
    }
    if (enabled !== undefined && typeof enabled !== 'boolean') {
      throw invalidInput(`${path}.enabled must be a boolean`);
    }
  }
  return body as AgentHostConfig;
}

/**
 *
 * Checked on the document the native save actually writes — i.e. after the
 * replacement transform, which disables the entries it replaced. Validating
 * the submitted set instead would let a stored bad entry block forever the
 * save that disables it.
 */
function assertLaunchableProviders(config: AgentHostConfig): void {
  for (const [index, provider] of config.providers.entries()) {
    if (
      provider.enabled !== false &&
      (typeof provider.command !== 'string' || provider.command.trim() === '')
    ) {
      throw invalidInput(
        `providers[${index}].command is required when provider '${provider.id}' is enabled`,
      );
    }
  }
}

export async function getAgentHostConfig(service: ServiceCore): Promise<AgentHostConfig> {
  try {
    return (await service.core.getAgentHostConfig()) as AgentHostConfig;
  } catch (error) {
    throw mapNativeError(error);
  }
}

function withReplacementSelected(config: AgentHostConfig): AgentHostConfig {
  const selected = config.providers.findLastIndex((provider) => provider.enabled !== false);
  if (selected < 0) {
    return config;
  }
  const changes = config.providers.some(
    (provider, index) => (provider.enabled !== false) !== (index === selected),
  );
  if (!changes) {
    return config;
  }
  return {
    ...config,
    providers: config.providers.map((provider, index) =>
      index === selected
        ? { ...provider, enabled: true }
        : { ...provider, enabled: false },
    ),
  };
}

/** The owner facts the restart publishes; the same shape the boot start returns. */
interface EngineOwnerFacts {
  engine_epoch: number | null;
  provider_ready: boolean;
}

export function putAgentHostConfig(
  service: ServiceCore,
  body: unknown,
): Promise<AgentHostConfig> {
  const config = parseAgentHostConfig(body);
  return withSelectionLock(service, async () => {
    const next = withReplacementSelected(config);
    assertLaunchableProviders(next);
    try {
      await service.core.saveAgentHostConfig(next);
    } catch (error) {
      throw mapNativeError(error);
    }

    let facts: EngineOwnerFacts;
    try {
      facts = (await service.core.restartExecutionOwner()) as EngineOwnerFacts;
    } catch {
      throw new HttpError(
        503,
        'engine_restart_failed',
        'Provider configuration was saved, but the engine owner could not be restarted',
      );
    }
    service.providerReady = facts.provider_ready;
    service.engineEpoch = facts.engine_epoch ?? 0;

    return getAgentHostConfig(service);
  });
}

export const AGENT_HOST_CONFIG_ROUTES: readonly DomainRoute[] = [
  {
    method: 'GET',
    pattern: /^\/v1\/daemon\/agent-host\/config$/,
    tier: 'tier1',
    family: 'host',
    handle: async (service) => ({ body: await getAgentHostConfig(service) }),
  },
  {
    method: 'PUT',
    pattern: /^\/v1\/daemon\/agent-host\/config$/,
    tier: 'tier1',
    family: 'host',
    handle: async (service, _params, _search, body) => ({
      body: await putAgentHostConfig(service, body),
    }),
  },
];

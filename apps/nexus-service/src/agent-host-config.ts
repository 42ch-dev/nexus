/**
 * Agent-host config family (I-000124 follow-up): the provider document behind
 * the engine repair page.
 *
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

const CONFIG_KEYS = [
  'max_sessions',
  'max_ops_per_session',
  'timeouts',
  'policy',
  'providers',
] as const;
const TIMEOUT_KEYS = [
  'launch_ms',
  'initialize_ms',
  'session_ms',
  'prompt_ms',
  'shutdown_ms',
] as const;
const POLICY_KEYS = [
  'unknown_provider',
  'unknown_tool_risk',
  'allow_model_fallback',
] as const;
const PROVIDER_KEYS = ['id', 'protocol', 'command', 'args', 'env', 'enabled'] as const;

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

/**
 * Enforce the generated `AgentHostConfig` wire schema before the native save
 * path re-validates the same document. Without it a malformed entry would
 * surface as the sanitized `internal` 500 instead of a 400 that names the
 * offending field.
 */
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

export async function getAgentHostConfig(service: ServiceCore): Promise<AgentHostConfig> {
  try {
    return (await service.core.getAgentHostConfig()) as AgentHostConfig;
  } catch (error) {
    throw mapNativeError(error);
  }
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
  // The ONE owner-lifecycle lock (shared with selectWorkspace): a provider
  // save+restart must never interleave with a first-run selection.
  return withSelectionLock(service, async () => {
    try {
      await service.core.saveAgentHostConfig(config);
    } catch (error) {
      throw mapNativeError(error);
    }

    // The saved document is only real once the engine has re-read it: settle and
    // re-establish the owner, then publish the fresh facts. A failed restart is
    // a typed 503 — the old facts stay untouched, so nothing claims readiness
    // the probe did not produce.
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

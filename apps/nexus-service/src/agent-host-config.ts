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
import type { ServiceCore } from './lifecycle.js';
import { HttpError, mapNativeError } from './errors.js';
import type { DomainRoute } from './routes.js';

/** One provider entry, matching `nexus_agent_host::config::ProviderConfig`. */
export interface AgentHostProviderConfig {
  id: string;
  protocol: 'acp' | 'native_cli';
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  enabled?: boolean;
}

/** The persisted document, matching `AgentHostConfig`'s serde shape. */
export interface AgentHostConfig {
  max_sessions?: number;
  max_ops_per_session?: number;
  timeouts?: Record<string, number>;
  policy?: { unknown_provider?: string; unknown_tool_risk?: string; allow_model_fallback?: boolean };
  providers: AgentHostProviderConfig[];
}

const PROTOCOL_ACP = 'acp';
const PROTOCOL_NATIVE_CLI = 'native_cli';

/**
 * Client-facing shape check before the native save path re-validates the same
 * document. Without it a malformed entry would surface as the sanitized
 * `internal` 500 instead of a 400 that names the offending field.
 */
function parseAgentHostConfig(body: unknown): AgentHostConfig {
  if (typeof body !== 'object' || body === null || Array.isArray(body)) {
    throw new HttpError(400, 'invalid_input', 'config must be an object with a providers array');
  }
  const { providers } = body as { providers?: unknown };
  if (!Array.isArray(providers)) {
    throw new HttpError(400, 'invalid_input', 'config must be an object with a providers array');
  }
  for (const [index, provider] of providers.entries()) {
    if (typeof provider !== 'object' || provider === null || Array.isArray(provider)) {
      throw new HttpError(400, 'invalid_input', `providers[${index}] must be an object`);
    }
    const { id, protocol } = provider as { id?: unknown; protocol?: unknown };
    if (typeof id !== 'string' || id.trim() === '') {
      throw new HttpError(400, 'invalid_input', `providers[${index}].id is required`);
    }
    if (protocol !== PROTOCOL_ACP && protocol !== PROTOCOL_NATIVE_CLI) {
      throw new HttpError(
        400,
        'invalid_input',
        `providers[${index}].protocol must be "acp" or "native_cli"`,
      );
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

export async function putAgentHostConfig(
  service: ServiceCore,
  body: unknown,
): Promise<AgentHostConfig> {
  const config = parseAgentHostConfig(body);
  try {
    await service.core.saveAgentHostConfig(config);
  } catch (error) {
    throw mapNativeError(error);
  }
  return getAgentHostConfig(service);
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

/**
 * Static outbound install/docs URL table for ACP agents (app layer).
 *
 * Spec: `.mstar/iterations/v1.101/specs/agent-picker-and-detection.md` §8 —
 * URLs live here (wizard/setup), not in schemas or `@42ch/nexus-ui`.
 * Missing entry → AgentPicker hides that link.
 */

export interface AgentOutboundUrls {
  installUrl?: string | null;
  docsUrl?: string | null;
}

/**
 * Keys are matched against `registry_agent_id` first, then `name`
 * (case-insensitive). Unknown agents get no outbound links.
 */
const AGENT_OUTBOUND_URLS: Record<string, AgentOutboundUrls> = {
  'claude-code': {
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    docsUrl: 'https://docs.anthropic.com/en/docs/claude-code',
  },
  claude: {
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    docsUrl: 'https://docs.anthropic.com/en/docs/claude-code',
  },
  codex: {
    installUrl: 'https://github.com/openai/codex',
    docsUrl: null,
  },
  'gemini-cli': {
    installUrl: 'https://github.com/google-gemini/gemini-cli',
    docsUrl: 'https://ai.google.dev/',
  },
  gemini: {
    installUrl: 'https://github.com/google-gemini/gemini-cli',
    docsUrl: 'https://ai.google.dev/',
  },
};

/** Look up outbound URLs for a scan entry by registry id or display name. */
export function lookupAgentOutboundUrls(
  registryAgentId: string | null | undefined,
  name: string,
): AgentOutboundUrls {
  const keys = [registryAgentId, name]
    .filter((k): k is string => Boolean(k && k.trim()))
    .map((k) => k.trim().toLowerCase());

  for (const key of keys) {
    const hit = AGENT_OUTBOUND_URLS[key];
    if (hit) return hit;
  }
  return {};
}

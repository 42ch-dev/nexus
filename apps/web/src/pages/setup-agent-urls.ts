/**
 * Static outbound install/docs URL table for ACP agents (app layer).
 *
 * Spec: `.mstar/iterations/v1.101/specs/agent-picker-and-detection.md` §8 —
 * URLs live here (wizard/setup), not in schemas or `@42ch/nexus-ui`.
 * Missing entry → AgentPicker hides that link.
 *
 * Primary keys match live ACP registry `id` values (`claude-acp`, `codex-acp`,
 * `gemini`, …). Display-name / legacy aliases are kept for resilience.
 */

export interface AgentOutboundUrls {
  installUrl?: string | null;
  docsUrl?: string | null;
}

const CLAUDE_URLS: AgentOutboundUrls = {
  installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
  docsUrl: 'https://docs.anthropic.com/en/docs/claude-code',
};

const CODEX_URLS: AgentOutboundUrls = {
  installUrl: 'https://github.com/openai/codex',
  docsUrl: null,
};

const GEMINI_URLS: AgentOutboundUrls = {
  installUrl: 'https://github.com/google-gemini/gemini-cli',
  docsUrl: 'https://ai.google.dev/',
};

/**
 * Keys are matched against `registry_agent_id` first, then `name`
 * (case-insensitive). Unknown agents get no outbound links.
 */
const AGENT_OUTBOUND_URLS: Record<string, AgentOutboundUrls> = {
  // Live ACP registry ids (sample + CDN).
  'claude-acp': CLAUDE_URLS,
  'codex-acp': CODEX_URLS,
  gemini: GEMINI_URLS,
  // Display-name / legacy aliases.
  'claude-code': CLAUDE_URLS,
  claude: CLAUDE_URLS,
  codex: CODEX_URLS,
  'gemini-cli': GEMINI_URLS,
  'openai/codex': CODEX_URLS,
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

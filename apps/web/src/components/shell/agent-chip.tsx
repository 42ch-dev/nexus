/**
 * Agent chip — topnav badge for the current AI agent (IA spec §3.1).
 *
 * Data sources are read-only (assignment T2): the saved agent profile resolved
 * by `useSavedAgentProfile` (desktop IPC profile, else the browser
 * `nexus-agent-profile:<creator_id>` snapshot owned by T3/onboarding), matched
 * onto the latest ACP scan exactly like the daemon status bar badge. Click
 * opens the existing agent settings surface; no new persistence here.
 */
import { useNavigate } from 'react-router';

import { resolveAgentBadge } from '@/components/layout/daemon-status-bar';
import { useSavedAgentProfile, useScanAgents } from '@/api/queries';
import { useTranslation } from 'react-i18next';

export function AgentChip() {
  const { t } = useTranslation('shell');
  const navigate = useNavigate();
  const scan = useScanAgents({ filter: 'all' });
  const profile = useSavedAgentProfile();

  const info = profile
    ? resolveAgentBadge(profile, scan.data?.agents ?? [])
    : null;

  return (
    <button
      type="button"
      data-testid="agent-chip"
      title={t('topnav.agentChip')}
      onClick={() => navigate('/settings/agent')}
      className="ml-1 flex h-8 items-center gap-2 rounded-control border border-gray-alpha-400 bg-background-200 px-3 text-label-14 text-gray-800 transition-colors duration-state ease-standard hover:border-blue-700 motion-reduce:transition-none"
    >
      <span className="h-[7px] w-[7px] rounded-full bg-green-700" aria-hidden="true" />
      <span className="max-w-32 truncate">{info?.displayName ?? t('topnav.noAgent')}</span>
      {info?.version && (
        <span className="text-gray-600" data-testid="agent-chip-version">
          {info.version}
        </span>
      )}
    </button>
  );
}

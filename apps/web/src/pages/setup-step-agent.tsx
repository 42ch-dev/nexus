import { useEffect, useMemo, useRef, useState } from 'react';
import { ChevronLeft } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import {
  AgentPicker,
  type AgentPickerItem,
  type AgentPickerStatus,
  type AgentVerifyStatus,
} from '@/components/setup/agent-picker';
import { useScanAgents, useVerifyAgent } from '@/api/queries';
import { errorMessage } from '@/lib/error-message';
import {
  defaultGridEntries,
  moreAgentsEntries,
  resolveCatalogItem,
  type AgentCatalogItem,
} from '@/lib/agent-catalog';
import type { AgentScanEntry } from '@42ch/nexus-contracts';
import type { WizardState } from '@/pages/setup-wizard-page';

interface SetupStepAgentProps {
  state: WizardState;
  onChange: (state: WizardState) => void;
  onNext: () => void;
  /** Hidden on first step (Agent); omit so Back is not shown. */
  onBack?: () => void;
}

/** Map a catalog item → picker view-model. */
function catalogItemToPickerItem(item: AgentCatalogItem): AgentPickerItem {
  return {
    id: item.id,
    name: item.name,
    displayName: item.displayName,
    version: item.version,
    description: item.description,
    iconUrl: item.iconUrl,
    installed: item.installed,
    installUrl: item.installUrl ?? null,
    docsUrl: item.docsUrl ?? null,
  };
}

/** Legacy: picker id for a scan entry. Kept for backward-compat tests. */
export function agentPickerId(agent: AgentScanEntry): string {
  return (agent.registry_agent_id?.trim() || agent.name).trim();
}

/**
 * Legacy: collision-safe picker ids. Kept for backward-compat tests.
 */
export function assignCollisionSafePickerIds(
  agents: AgentScanEntry[],
): string[] {
  const seen = new Map<string, number>();
  return agents.map((agent) => {
    const base = agentPickerId(agent);
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    return count === 0 ? base : `${base}#${count}`;
  });
}

/**
 * Legacy: map scan entries to picker items. Kept for backward-compat tests.
 */
export function mapScanEntriesToPickerItems(
  agents: AgentScanEntry[],
): AgentPickerItem[] {
  const ids = assignCollisionSafePickerIds(agents);
  return agents.map((agent, index) => {
    const item = resolveCatalogItem(agent);
    return {
      id: ids[index]!,
      name: agent.name,
      version: agent.version,
      description: agent.description,
      installed: agent.installed,
      installUrl: item.installUrl ?? null,
      docsUrl: item.docsUrl ?? null,
    };
  });
}

function resolvePickerStatus(
  isLoading: boolean,
  isError: boolean,
  agentCount: number,
): AgentPickerStatus {
  if (isLoading) return 'loading';
  if (isError) return 'error';
  if (agentCount === 0) return 'empty';
  return 'ready';
}

export function SetupStepAgent({ state, onChange, onNext, onBack }: SetupStepAgentProps) {
  const { t } = useTranslation('setup');
  const scan = useScanAgents({ filter: 'all', registry_refresh: true });
  const verifyAgent = useVerifyAgent();
  const agents = scan.data?.agents ?? [];
  const defaultGrid = useMemo(
    () => defaultGridEntries(agents).map(catalogItemToPickerItem),
    [agents],
  );
  const moreAgents = useMemo(
    () => moreAgentsEntries(agents).map(catalogItemToPickerItem),
    [agents],
  );
  const status = resolvePickerStatus(scan.isLoading, scan.isError, agents.length);

  const agentsByCatalogId = useMemo(() => {
    const map = new Map<string, AgentScanEntry>();
    for (const agent of agents) {
      const item = resolveCatalogItem(agent);
      map.set(item.id, agent);
    }
    return map;
  }, [agents]);

  const [verifyStatus, setVerifyStatus] = useState<AgentVerifyStatus>('idle');

  const firstInstalled = useMemo(
    () => agents.find((a) => a.installed) ?? null,
    [agents],
  );

  const selectedAgent = state.selectedAgent;
  const customLaunchCommand = state.customLaunchCommand;
  const workspaceRoot = state.workspaceRoot;
  const workspacePicked = state.workspacePicked;
  const profileDisplayName = state.profileDisplayName;

  const stateRef = useRef(state);
  stateRef.current = state;
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // Default to the first installed agent (profile path).
  // Narrow deps (QC B3 + R-V194QC1-S101): do not depend on the whole `state`
  // object or its pass-through fields. The effect only needs to react when a
  // first installed agent becomes available while no agent is selected.
  useEffect(() => {
    if (stateRef.current.selectedAgent || stateRef.current.customLaunchCommand.trim()) return;
    if (!firstInstalled) return;
    onChangeRef.current({
      ...stateRef.current,
      selectedAgent: firstInstalled,
      customLaunchCommand: '',
    });
  }, [firstInstalled]);

  function selectById(id: string) {
    const agent = agentsByCatalogId.get(id);
    if (!agent?.installed) return;
    onChange({
      workspaceRoot,
      workspacePicked,
      profileDisplayName,
      selectedAgent: agent,
      customLaunchCommand: '',
    });
  }

  function useCustom(command: string) {
    setVerifyStatus('idle');
    onChange({
      workspaceRoot,
      workspacePicked,
      profileDisplayName,
      selectedAgent: null,
      customLaunchCommand: command,
    });
  }

  async function handleVerify() {
    const command = customLaunchCommand.trim();
    if (!command) return;
    setVerifyStatus('loading');
    try {
      const ok = await verifyAgent.mutateAsync(command);
      // false = scan reached the daemon but no installed agent matched.
      // R-V1108P1QC2-S001: distinguish transport failure from no-match.
      setVerifyStatus(ok ? 'success' : 'no-match');
    } catch {
      // Transport/unreachable — could not reach the daemon at all.
      setVerifyStatus('error');
    }
  }

  const selectedId = useMemo(() => {
    if (!selectedAgent) return null;
    return resolveCatalogItem(selectedAgent).id;
  }, [selectedAgent]);

  // FB-UI-008: an installed agent continues directly (registry-validated).
  // A custom launch command must be verified before continuing so authors
  // test the command in-place rather than discovering it is broken later.
  const canContinue = Boolean(selectedAgent) || verifyStatus === 'success';

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto" data-testid="wizard-step-body">
        <div className="flex flex-col gap-2">
          <h2 className="text-heading-24 font-heading text-gray-1000">{t('step.agent.title')}</h2>
          <p className="text-copy-14 text-gray-900">{t('step.agent.description')}</p>
        </div>

        <AgentPicker
          status={status}
          defaultGrid={status === 'ready' ? defaultGrid : []}
          moreAgents={status === 'ready' ? moreAgents : []}
          selectedId={selectedId}
          onSelect={selectById}
          customLaunchValue={customLaunchCommand}
          onCustomLaunchChange={useCustom}
          onVerify={handleVerify}
          verifyStatus={verifyStatus}
          errorDescription={
            scan.isError
              ? errorMessage(scan.error) || t('agentPicker.error.fallbackDescription')
              : undefined
          }
          onRetry={scan.isError ? () => void scan.refetch() : undefined}
          emptyTitle={t('step.agent.emptyTitle')}
          emptyDescription={t('step.agent.emptyDescription')}
          density="compact"
        />
      </div>

      <div
        className="mt-auto flex shrink-0 items-center gap-setup-wizard-surface-cta-container-gap"
        data-testid="wizard-cta-row"
        data-layout="horizontal-adjacent"
      >
        {onBack && (
          <Button variant="tertiary" onClick={onBack} aria-label={t('action.back')} className="px-2">
            <ChevronLeft className="h-4 w-4" aria-hidden="true" />
          </Button>
        )}
        <Button
          variant="primary"
          onClick={onNext}
          disabled={!canContinue}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          {t('action.continue')}
        </Button>
      </div>
    </div>
  );
}

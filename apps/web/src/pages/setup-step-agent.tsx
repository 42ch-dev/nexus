import { useEffect, useMemo } from 'react';

import { Button } from '@/components/ui/button';
import {
  AgentPicker,
  type AgentPickerItem,
  type AgentPickerStatus,
} from '@/components/setup/agent-picker';
import { useScanAgents } from '@/api/queries';
import { errorMessage } from '@/lib/error-message';
import { lookupAgentOutboundUrls } from '@/pages/setup-agent-urls';
import type { AgentScanEntry } from '@42ch/nexus-contracts';
import type { WizardState } from '@/pages/setup-wizard-page';

interface SetupStepAgentProps {
  state: WizardState;
  onChange: (state: WizardState) => void;
  onNext: () => void;
  onBack: () => void;
}

/** Stable picker id for an scan entry (registry id preferred). */
export function agentPickerId(agent: AgentScanEntry): string {
  return (agent.registry_agent_id?.trim() || agent.name).trim();
}

/** Map wire scan entries → presentational picker items (+ static URL table). */
export function mapScanEntriesToPickerItems(
  agents: AgentScanEntry[],
): AgentPickerItem[] {
  return agents.map((agent) => {
    const urls = lookupAgentOutboundUrls(agent.registry_agent_id, agent.name);
    return {
      id: agentPickerId(agent),
      name: agent.name,
      version: agent.version,
      description: agent.description,
      installed: agent.installed,
      installUrl: urls.installUrl ?? null,
      docsUrl: urls.docsUrl ?? null,
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
  const scan = useScanAgents({ filter: 'all', registry_refresh: true });
  const agents = scan.data?.agents ?? [];
  const pickerItems = useMemo(() => mapScanEntriesToPickerItems(agents), [agents]);
  const status = resolvePickerStatus(scan.isLoading, scan.isError, agents.length);

  const agentsById = useMemo(() => {
    const map = new Map<string, AgentScanEntry>();
    for (const agent of agents) {
      map.set(agentPickerId(agent), agent);
    }
    return map;
  }, [agents]);

  const firstInstalled = useMemo(
    () => agents.find((a) => a.installed) ?? null,
    [agents],
  );

  // Default to the first installed agent (profile path).
  useEffect(() => {
    if (state.selectedAgent || state.customLaunchCommand) return;
    if (!firstInstalled) return;
    onChange({ ...state, selectedAgent: firstInstalled });
  }, [firstInstalled, state, onChange]);

  function selectById(id: string) {
    const agent = agentsById.get(id);
    if (!agent?.installed) return;
    onChange({ ...state, selectedAgent: agent, customLaunchCommand: '' });
  }

  function useCustom(command: string) {
    onChange({
      ...state,
      selectedAgent: null,
      customLaunchCommand: command,
    });
  }

  const selectedId = state.selectedAgent
    ? agentPickerId(state.selectedAgent)
    : null;

  const canContinue = Boolean(state.selectedAgent || state.customLaunchCommand.trim());

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <h2 className="text-heading-24 font-heading text-gray-1000">Choose an ACP agent</h2>
        <p className="text-copy-14 text-gray-900">
          Nexus uses an ACP-compatible agent to run strategies. Select a discovered agent or provide a custom launch command.
        </p>
      </div>

      <AgentPicker
        status={status}
        agents={status === 'ready' ? pickerItems : []}
        selectedId={selectedId}
        onSelect={selectById}
        customLaunchValue={state.customLaunchCommand}
        onCustomLaunchChange={useCustom}
        errorDescription={
          scan.isError
            ? errorMessage(scan.error) || 'The daemon did not respond to the agent scan request.'
            : undefined
        }
        onRetry={scan.isError ? () => void scan.refetch() : undefined}
        emptyTitle="No agents found on PATH."
      />

      <div className="flex flex-col gap-setup-wizard-surface-cta-container-gap mt-auto">
        <Button
          variant="primary"
          onClick={onNext}
          disabled={!canContinue}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          Continue
        </Button>
        <Button variant="tertiary" onClick={onBack} className="self-start">
          Back
        </Button>
      </div>
    </div>
  );
}

import { useEffect, useMemo } from 'react';
import { ChevronLeft } from 'lucide-react';

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
  /** Hidden on first step (Agent); omit so Back is not shown. */
  onBack?: () => void;
}

/** Base picker id for a scan entry (registry id preferred). */
export function agentPickerId(agent: AgentScanEntry): string {
  return (agent.registry_agent_id?.trim() || agent.name).trim();
}

/**
 * Assign collision-safe picker ids for a scan list.
 *
 * Duplicate `registry_agent_id` / name values get a `#n` suffix so map lookups
 * and React keys stay unique (QC B6).
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

/** Map wire scan entries → presentational picker items (+ static URL table). */
export function mapScanEntriesToPickerItems(
  agents: AgentScanEntry[],
): AgentPickerItem[] {
  const ids = assignCollisionSafePickerIds(agents);
  return agents.map((agent, index) => {
    const urls = lookupAgentOutboundUrls(agent.registry_agent_id, agent.name);
    return {
      id: ids[index]!,
      name: agent.name,
      version: agent.version,
      description: agent.description,
      installed: agent.installed,
      installUrl: urls.installUrl ?? null,
      docsUrl: urls.docsUrl ?? null,
    };
  });
}

/**
 * Build a collision-safe id → scan-entry map (same ids as picker items).
 */
export function buildAgentsByPickerId(
  agents: AgentScanEntry[],
): Map<string, AgentScanEntry> {
  const ids = assignCollisionSafePickerIds(agents);
  const map = new Map<string, AgentScanEntry>();
  agents.forEach((agent, index) => {
    map.set(ids[index]!, agent);
  });
  return map;
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

  const agentsById = useMemo(() => buildAgentsByPickerId(agents), [agents]);

  const firstInstalled = useMemo(
    () => agents.find((a) => a.installed) ?? null,
    [agents],
  );

  const selectedAgent = state.selectedAgent;
  const customLaunchCommand = state.customLaunchCommand;
  const workspaceRoot = state.workspaceRoot;
  const workspacePicked = state.workspacePicked;

  // Default to the first installed agent (profile path).
  // Narrow deps (QC B3): do not depend on the whole `state` object.
  useEffect(() => {
    if (selectedAgent || customLaunchCommand.trim()) return;
    if (!firstInstalled) return;
    onChange({
      workspaceRoot,
      workspacePicked,
      selectedAgent: firstInstalled,
      customLaunchCommand: '',
    });
  }, [
    firstInstalled,
    selectedAgent,
    customLaunchCommand,
    workspaceRoot,
    workspacePicked,
    onChange,
  ]);

  function selectById(id: string) {
    const agent = agentsById.get(id);
    if (!agent?.installed) return;
    onChange({
      workspaceRoot,
      workspacePicked,
      selectedAgent: agent,
      customLaunchCommand: '',
    });
  }

  function useCustom(command: string) {
    onChange({
      workspaceRoot,
      workspacePicked,
      selectedAgent: null,
      customLaunchCommand: command,
    });
  }

  const selectedId = useMemo(() => {
    if (!selectedAgent) return null;
    for (const [id, agent] of agentsById) {
      if (
        agent.registry_agent_id === selectedAgent.registry_agent_id &&
        agent.name === selectedAgent.name
      ) {
        return id;
      }
    }
    return agentPickerId(selectedAgent);
  }, [selectedAgent, agentsById]);

  const canContinue = Boolean(selectedAgent || customLaunchCommand.trim());

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
        customLaunchValue={customLaunchCommand}
        onCustomLaunchChange={useCustom}
        errorDescription={
          scan.isError
            ? errorMessage(scan.error) || 'The daemon did not respond to the agent scan request.'
            : undefined
        }
        onRetry={scan.isError ? () => void scan.refetch() : undefined}
        emptyTitle="No agents found on PATH."
      />

      <div
        className="mt-auto flex items-center gap-setup-wizard-surface-cta-container-gap"
        data-testid="wizard-cta-row"
        data-layout="horizontal-adjacent"
      >
        {onBack && (
          <Button variant="tertiary" onClick={onBack} aria-label="Back" className="px-2">
            <ChevronLeft className="h-4 w-4" aria-hidden="true" />
          </Button>
        )}
        <Button
          variant="primary"
          onClick={onNext}
          disabled={!canContinue}
          className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
        >
          Continue
        </Button>
      </div>
    </div>
  );
}

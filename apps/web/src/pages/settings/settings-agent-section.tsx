/**
 * Settings Agent section — V1.103 P0 scaffold (body from V1.102 thin host).
 *
 * Mounts app-shared AgentPicker under SettingsShellLayout outlet.
 * Persist via DesktopCapabilities.setAgentProfile (setup finish() parity).
 * Browser build: picker mounts; persist is a no-op toast.
 * P1 adds getAgentProfile preselect.
 */

import { useEffect, useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import {
  AgentPicker,
  type AgentPickerStatus,
} from '@/components/setup/agent-picker';
import { useScanAgents } from '@/api/queries';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';
import {
  agentPickerId,
  buildAgentsByPickerId,
  mapScanEntriesToPickerItems,
} from '@/pages/setup-step-agent';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

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

export function SettingsAgentSection() {
  const desktop = useDesktopCapabilities();
  const { toast } = useToast();
  const scan = useScanAgents({ filter: 'all', registry_refresh: true });
  const agents = scan.data?.agents ?? [];
  const pickerItems = useMemo(() => mapScanEntriesToPickerItems(agents), [agents]);
  const status = resolvePickerStatus(scan.isLoading, scan.isError, agents.length);
  const agentsById = useMemo(() => buildAgentsByPickerId(agents), [agents]);

  const [selectedAgent, setSelectedAgent] = useState<AgentScanEntry | null>(null);
  const [customLaunchCommand, setCustomLaunchCommand] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [didInitDefault, setDidInitDefault] = useState(false);

  const firstInstalled = useMemo(
    () => agents.find((a) => a.installed) ?? null,
    [agents],
  );

  // Default selection to first installed (setup parity). No getAgentProfile in Must P0.
  useEffect(() => {
    if (didInitDefault) return;
    if (scan.isLoading || scan.isError) return;
    if (selectedAgent || customLaunchCommand.trim()) {
      setDidInitDefault(true);
      return;
    }
    if (firstInstalled) {
      setSelectedAgent(firstInstalled);
    }
    setDidInitDefault(true);
  }, [
    didInitDefault,
    scan.isLoading,
    scan.isError,
    selectedAgent,
    customLaunchCommand,
    firstInstalled,
  ]);

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

  function selectById(id: string) {
    const agent = agentsById.get(id);
    if (!agent?.installed) return;
    setSelectedAgent(agent);
    setCustomLaunchCommand('');
  }

  function handleUseCustom(command: string) {
    setSelectedAgent(null);
    setCustomLaunchCommand(command);
  }

  const canSave = Boolean(selectedAgent || customLaunchCommand.trim());

  async function saveProfile() {
    if (!canSave || isSaving) return;

    if (!desktop) {
      toast({
        variant: 'info',
        title: 'Desktop only',
        description: 'Saving the agent profile requires the Nexus desktop app.',
      });
      return;
    }

    setIsSaving(true);
    try {
      const name = selectedAgent?.name ?? 'custom';
      const launchCommand =
        (selectedAgent?.launch_command ?? customLaunchCommand.trim()) || undefined;
      await desktop.setAgentProfile(name, launchCommand);
      toast({
        variant: 'success',
        title: 'Agent profile saved',
        description: `Using ${name} for local strategies.`,
      });
    } catch (err) {
      const description = errorMessage(err) || 'Failed to save agent profile.';
      toast({ variant: 'error', title: 'Could not save agent', description });
    } finally {
      setIsSaving(false);
    }
  }

  return (
    <div className="flex flex-col gap-6" data-testid="settings-agent-section">
      <div data-testid="settings-host-picker-region">
        <AgentPicker
          status={status}
          agents={status === 'ready' ? pickerItems : []}
          selectedId={selectedId}
          onSelect={selectById}
          customLaunchValue={customLaunchCommand}
          onCustomLaunchChange={handleUseCustom}
          errorDescription={
            scan.isError
              ? errorMessage(scan.error) ||
                'The daemon did not respond to the agent scan request.'
              : undefined
          }
          onRetry={scan.isError ? () => void scan.refetch() : undefined}
          emptyTitle="No agents found on PATH."
        />
      </div>

      <div className="flex items-center gap-3">
        <Button
          variant="primary"
          onClick={() => void saveProfile()}
          disabled={!canSave || isSaving}
          data-testid="settings-save-agent"
        >
          {isSaving ? 'Saving…' : 'Save Agent'}
        </Button>
      </div>
    </div>
  );
}

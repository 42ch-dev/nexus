/**
 * Settings Agent section — V1.103 P1 (G1 getAgentProfile preselect).
 *
 * Mounts app-shared AgentPicker under SettingsShellLayout outlet.
 * Desktop: after scan settles, preselect via getAgentProfile (match by name).
 * Persist via setAgentProfile on Save Agent (setup finish() parity).
 * Browser: picker mounts; skip saved-profile preselect; desktop-only save toast.
 */

import { useEffect, useMemo, useRef, useState } from 'react';

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

/** Locked by settings-agent-section.md — section body helper (sentence case). */
const AGENT_SECTION_HELPER =
  'Choose which local ACP agent Nexus uses for creative work.';

const BROWSER_ONLY_HELPER =
  'Agent selection is available on the desktop app only.';

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

/**
 * Apply a saved profile onto scan results: prefer name match among installed
 * agents; otherwise use launchCommand as the custom path. Returns whether a
 * selection was applied (caller falls back to first-installed when false).
 */
function applySavedProfile(
  agents: AgentScanEntry[],
  profile: { name: string; launchCommand?: string },
  setSelectedAgent: (agent: AgentScanEntry | null) => void,
  setCustomLaunchCommand: (command: string) => void,
): boolean {
  const match = agents.find((a) => a.name === profile.name && a.installed);
  if (match) {
    setSelectedAgent(match);
    setCustomLaunchCommand('');
    return true;
  }
  const launch = profile.launchCommand?.trim();
  if (launch) {
    setSelectedAgent(null);
    setCustomLaunchCommand(launch);
    return true;
  }
  return false;
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
  /** Author touched picker before async preselect finished — skip late apply. */
  const userTouchedRef = useRef(false);

  // G1: after scan settles, desktop preselects via getAgentProfile (match by
  // name). Null/unreadable → first-installed fallback. Browser skips read.
  // Depend on scan.data?.agents (stable when data unchanged), not a fresh
  // `agents ?? []` alias — that would cancel the async read every render.
  useEffect(() => {
    if (didInitDefault) return;
    if (scan.isLoading || scan.isError) return;
    const scanned = scan.data?.agents;
    if (scanned === undefined) return;

    let cancelled = false;

    async function initSelection() {
      if (desktop) {
        const profile = await desktop.getAgentProfile();
        if (cancelled) return;
        // qc2 F-002: do not overwrite a selection the author already made.
        if (userTouchedRef.current) {
          setDidInitDefault(true);
          return;
        }
        if (
          profile &&
          applySavedProfile(
            scanned,
            profile,
            setSelectedAgent,
            setCustomLaunchCommand,
          )
        ) {
          setDidInitDefault(true);
          return;
        }
      }

      if (cancelled) return;
      if (userTouchedRef.current) {
        setDidInitDefault(true);
        return;
      }
      const fallback = scanned.find((a) => a.installed) ?? null;
      if (fallback) {
        setSelectedAgent(fallback);
      }
      setDidInitDefault(true);
    }

    void initSelection();
    return () => {
      cancelled = true;
    };
  }, [
    didInitDefault,
    scan.isLoading,
    scan.isError,
    scan.data?.agents,
    desktop,
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
    userTouchedRef.current = true;
    setSelectedAgent(agent);
    setCustomLaunchCommand('');
  }

  function handleUseCustom(command: string) {
    userTouchedRef.current = true;
    setSelectedAgent(null);
    setCustomLaunchCommand(command);
  }

  const canSave = Boolean(selectedAgent || customLaunchCommand.trim());

  async function saveProfile() {
    if (!canSave || isSaving) return;

    if (!desktop) {
      toast({
        variant: 'info',
        title: 'Save agent on desktop',
        description: 'Open the Nexus desktop app to change your local agent.',
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
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Agent</h3>
        <p className="text-copy-14 text-gray-900">{AGENT_SECTION_HELPER}</p>
        {!desktop ? (
          <p className="text-copy-13 text-gray-700" data-testid="settings-agent-browser-helper">
            {BROWSER_ONLY_HELPER}
          </p>
        ) : null}
      </div>

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

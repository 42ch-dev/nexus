/**
 * Settings Agent section — V1.103 P1 (G1 preselect).
 *
 * Mounts app-shared AgentPicker as a Settings section body. The saved profile
 * comes from the shared `useSavedAgentProfile` seam (desktop IPC / browser
 * service), so this page preselects and saves through the same store the Agent
 * chip and the Harness profile bar read — in the browser too. Persist happens
 * instantly on card select or custom verify success (V1.125 AC-V1125-2).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';

import {
  AgentPicker,
  type AgentPickerItem,
  type AgentPickerStatus,
  type AgentVerifyStatus,
} from '@/components/setup/agent-picker';
import { Button } from '@/components/ui/button';
import {
  launchCommandMatches,
  useSavedAgentProfile,
  useScanAgents,
  useVerifyAgent,
} from '@/api/queries';
import { useDesktopCapabilities, useNexusClient } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';
import { queryKeys } from '@/lib/nexus/query-keys';
import {
  defaultGridEntries,
  moreAgentsEntries,
  buildPickerSelection,
  resolveAgentKey,
  type AgentCatalogItem,
} from '@/lib/agent-catalog';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

/** Map a catalog item → picker view-model. */
function catalogItemToPickerItem(item: AgentCatalogItem): AgentPickerItem {
  return {
    // Selection id is the collision-safe picker id (PR#148 Greptile P1), not
    // the shared catalog key, so each card maps to exactly one scan row.
    id: item.pickerId,
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
 * Apply a saved profile onto scan results: prefer launch-command match among
 * installed agents, then catalog-key match; otherwise use launchCommand as the
 * custom path. Returns whether a selection was applied.
 */
function applySavedProfile(
  agents: AgentScanEntry[],
  profile: { name: string; launchCommand?: string },
  setSelectedAgent: (agent: AgentScanEntry | null) => void,
  setCustomLaunchCommand: (command: string) => void,
): boolean {
  const launch = profile.launchCommand?.trim();
  if (launch) {
    const byCommand = agents.find(
      (a) =>
        a.installed &&
        !!a.launch_command &&
        launchCommandMatches(launch, a.launch_command),
    );
    if (byCommand) {
      setSelectedAgent(byCommand);
      setCustomLaunchCommand('');
      return true;
    }
  }

  const profileKey = profile.name.trim();
  const byKey = agents.find((a) => {
    if (!a.installed) return false;
    return resolveAgentKey(a) === profileKey || a.name === profileKey;
  });
  if (byKey) {
    setSelectedAgent(byKey);
    setCustomLaunchCommand('');
    return true;
  }

  if (launch) {
    setSelectedAgent(null);
    setCustomLaunchCommand(launch);
    return true;
  }
  return false;
}

export function SettingsAgentSection() {
  const { t } = useTranslation('settings');
  const { t: commonT } = useTranslation('common');
  const desktop = useDesktopCapabilities();
  const client = useNexusClient();
  const { toast } = useToast();
  const saved = useSavedAgentProfile();
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
  // Collision-safe picker-id ↔ scan-entry index. Replaces a catalog-key map
  // that silently collides when two scan rows resolve to the same key and
  // could save the wrong `launch_command` (PR#148 Greptile P1).
  const pickerSelection = useMemo(() => buildPickerSelection(agents), [agents]);

  const [selectedAgent, setSelectedAgent] = useState<AgentScanEntry | null>(null);
  const [customLaunchCommand, setCustomLaunchCommand] = useState('');
  const [didInitDefault, setDidInitDefault] = useState(false);
  const [verifyStatus, setVerifyStatus] = useState<AgentVerifyStatus>('idle');
  /** Author touched picker before async preselect finished — skip late apply. */
  const userTouchedRef = useRef(false);
  const qc = useQueryClient();

  const persistAgentProfile = useCallback(
    async (agent: AgentScanEntry | null, customCommand: string) => {
      const name = agent?.name ?? 'custom';
      const launchCommand =
        (agent?.launch_command ?? customCommand.trim()) ?? undefined;
      try {
        // Transport-coherent with the read seam: desktop keeps the IPC store,
        // the browser writes the service the same seam reads.
        if (desktop) {
          await desktop.setAgentProfile(name, launchCommand);
        } else {
          await client.saveAgentProfile(
            launchCommand === undefined
              ? { name }
              : { name, launch_command: launchCommand },
          );
        }
        void qc.invalidateQueries({ queryKey: queryKeys.agentProfile.detail() });
        void qc.invalidateQueries({
          queryKey: queryKeys.agentHost.scan({ filter: 'all' }),
        });
      } catch (err) {
        const description =
          errorMessage(err) || commonT('error.couldNotSaveAgentProfile');
        toast({
          variant: 'error',
          title: commonT('toast.couldNotSaveAgent'),
          description,
        });
      }
    },
    [client, commonT, desktop, qc, toast],
  );

  // G1: once both the scan and the shared profile read settle, preselect the
  // saved profile (match by command / catalog key); null/unreadable falls back
  // to the first installed agent. Works in the browser too — the seam reads the
  // service there.
  useEffect(() => {
    if (didInitDefault) return;
    // A failed profile read must not silently fall back to "first installed":
    // the error surface holds preselect until a refetch succeeds.
    if (scan.isLoading || scan.isError || saved.isLoading || saved.error) return;
    const scannedMaybe = scan.data?.agents;
    if (scannedMaybe === undefined) return;

    if (userTouchedRef.current) {
      setDidInitDefault(true);
      return;
    }
    if (
      saved.profile &&
      applySavedProfile(
        scannedMaybe,
        saved.profile,
        setSelectedAgent,
        setCustomLaunchCommand,
      )
    ) {
      setDidInitDefault(true);
      return;
    }
    const fallback = scannedMaybe.find((a) => a.installed) ?? null;
    if (fallback) {
      setSelectedAgent(fallback);
    }
    setDidInitDefault(true);
  }, [
    didInitDefault,
    scan.isLoading,
    scan.isError,
    scan.data?.agents,
    saved.isLoading,
    saved.error,
    saved.profile,
  ]);

  const selectedId = useMemo(() => {
    if (!selectedAgent) return null;
    return pickerSelection.byEntry.get(selectedAgent) ?? null;
  }, [selectedAgent, pickerSelection]);

  function selectById(id: string) {
    const agent = pickerSelection.byPickerId.get(id);
    if (!agent?.installed) return;
    userTouchedRef.current = true;
    setSelectedAgent(agent);
    setCustomLaunchCommand('');
    void persistAgentProfile(agent, '');
  }

  function handleUseCustom(command: string) {
    userTouchedRef.current = true;
    setSelectedAgent(null);
    setCustomLaunchCommand(command);
    setVerifyStatus('idle');
  }

  async function handleVerify() {
    const command = customLaunchCommand.trim();
    if (!command) return;
    setVerifyStatus('loading');
    try {
      const ok = await verifyAgent.mutateAsync(command);
      setVerifyStatus(ok ? 'success' : 'no-match');
      if (ok) {
        await persistAgentProfile(null, command);
      }
    } catch {
      setVerifyStatus('error');
    }
  }

  return (
    <div className="flex flex-col gap-6" data-testid="settings-agent-section">
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">{t('agent.title')}</h3>
        <p className="text-copy-14 text-gray-900">{t('agent.helper')}</p>
      </div>

      {saved.error ? (
        <div
          className="flex flex-col items-start gap-2"
          role="alert"
          data-testid="settings-agent-profile-error"
        >
          <p className="text-copy-13 text-red-900">
            {t('agent.profileError', {
              reason: errorMessage(saved.error) || commonT('error.requestFailed'),
            })}
          </p>
          <Button type="button" variant="tertiary" size="small" onClick={saved.refetch}>
            {t('agent.profileRetry')}
          </Button>
        </div>
      ) : (
        <div data-testid="settings-host-picker-region">
          <AgentPicker
            status={status}
            defaultGrid={status === 'ready' ? defaultGrid : []}
            moreAgents={status === 'ready' ? moreAgents : []}
            selectedId={selectedId}
            onSelect={selectById}
            customLaunchValue={customLaunchCommand}
            onCustomLaunchChange={handleUseCustom}
            onVerify={handleVerify}
            verifyStatus={verifyStatus}
            errorDescription={
              scan.isError
                ? errorMessage(scan.error) || t('agent.scanError')
                : undefined
            }
            onRetry={scan.isError ? () => void scan.refetch() : undefined}
            emptyTitle={t('agent.emptyTitle')}
            desktop={desktop ?? undefined}
            onExternalUrlError={() => {
              toast({ variant: 'error', title: commonT('error.openExternalFailed') });
            }}
          />
        </div>
        )}
    </div>
  );
}

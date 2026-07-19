/**
 * Settings Agent section — V1.103 P1 (G1 getAgentProfile preselect).
 *
 * Mounts app-shared AgentPicker under SettingsShellLayout outlet.
 * Desktop: after scan settles, preselect via getAgentProfile (match by command
 * / catalog key). Persist via instant `setAgentProfile` on card select or
 * custom verify success (V1.125 AC-V1125-2).
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
import { launchCommandMatches, useScanAgents, useVerifyAgent } from '@/api/queries';
import { useDesktopCapabilities } from '@/lib/client-context';
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
  const { toast } = useToast();
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
      if (!desktop) {
        toast({
          variant: 'info',
          title: commonT('toast.saveAgentOnDesktop'),
          description: commonT('toast.saveAgentOnDesktopDescription'),
        });
        return;
      }

      const name = agent?.name ?? 'custom';
      const launchCommand =
        (agent?.launch_command ?? customCommand.trim()) || undefined;
      try {
        await desktop.setAgentProfile(name, launchCommand);
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
    [commonT, desktop, qc, toast],
  );

  // G1: after scan settles, desktop preselects via getAgentProfile (match by
  // command / catalog key). Null/unreadable → first-installed fallback. Browser
  // skips read.
  useEffect(() => {
    if (didInitDefault) return;
    if (scan.isLoading || scan.isError) return;
    const scannedMaybe = scan.data?.agents;
    if (scannedMaybe === undefined) return;
    const scanned: AgentScanEntry[] = scannedMaybe;

    let cancelled = false;

    async function initSelection() {
      if (desktop) {
        const profile = await desktop.getAgentProfile();
        if (cancelled) return;
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
        {!desktop ? (
          <p className="text-copy-13 text-gray-700" data-testid="settings-agent-browser-helper">
            {t('agent.browserOnly')}
          </p>
        ) : null}
      </div>

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
    </div>
  );
}

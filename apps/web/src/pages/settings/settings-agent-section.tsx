/**
 * Settings Agent section — V1.103 P1 (G1 getAgentProfile preselect).
 *
 * Mounts app-shared AgentPicker under SettingsShellLayout outlet.
 * Desktop: after scan settles, preselect via getAgentProfile (match by name).
 * Persist via setAgentProfile on Save Agent (setup finish() parity).
 * Browser: picker mounts; skip saved-profile preselect; desktop-only save toast.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import {
  AgentPicker,
  type AgentPickerItem,
  type AgentPickerStatus,
  type AgentVerifyStatus,
} from '@/components/setup/agent-picker';
import { useScanAgents, useVerifyAgent } from '@/api/queries';
import { useDesktopCapabilities } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { useToast } from '@/lib/use-toast';
import {
  defaultGridEntries,
  moreAgentsEntries,
  resolveCatalogItem,
  type AgentCatalogItem,
} from '@/lib/agent-catalog';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

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
  const agentsByCatalogId = useMemo(() => {
    const map = new Map<string, AgentScanEntry>();
    for (const agent of agents) {
      const item = resolveCatalogItem(agent);
      map.set(item.id, agent);
    }
    return map;
  }, [agents]);

  const [selectedAgent, setSelectedAgent] = useState<AgentScanEntry | null>(null);
  const [customLaunchCommand, setCustomLaunchCommand] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [didInitDefault, setDidInitDefault] = useState(false);
  const [verifyStatus, setVerifyStatus] = useState<AgentVerifyStatus>('idle');
  /** Author touched picker before async preselect finished — skip late apply. */
  const userTouchedRef = useRef(false);

  // G1: after scan settles, desktop preselects via getAgentProfile (match by
  // name). Null/unreadable → first-installed fallback. Browser skips read.
  // Depend on scan.data?.agents (stable when data unchanged), not a fresh
  // `agents ?? []` alias — that would cancel the async read every render.
  useEffect(() => {
    if (didInitDefault) return;
    if (scan.isLoading || scan.isError) return;
    const scannedMaybe = scan.data?.agents;
    if (scannedMaybe === undefined) return;
    // Re-bind after the guard — TS does not narrow across nested async closures.
    const scanned: AgentScanEntry[] = scannedMaybe;

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
    return resolveCatalogItem(selectedAgent).id;
  }, [selectedAgent]);

  function selectById(id: string) {
    const agent = agentsByCatalogId.get(id);
    if (!agent?.installed) return;
    userTouchedRef.current = true;
    setSelectedAgent(agent);
    setCustomLaunchCommand('');
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
      // false = scan reached the daemon but no installed agent matched.
      // R-V1108P1QC2-S001: distinguish transport failure from no-match.
      setVerifyStatus(ok ? 'success' : 'no-match');
    } catch {
      // Transport/unreachable — could not reach the daemon at all.
      setVerifyStatus('error');
    }
  }

  const canSave = Boolean(selectedAgent || customLaunchCommand.trim());

  async function saveProfile() {
    if (!canSave || isSaving) return;

    if (!desktop) {
      toast({
        variant: 'info',
        title: commonT('toast.saveAgentOnDesktop'),
        description: commonT('toast.saveAgentOnDesktopDescription'),
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
        title: commonT('toast.agentProfileSaved'),
        description: commonT('toast.agentProfileSavedDescription', { name }),
      });
    } catch (err) {
      const description = errorMessage(err) || commonT('error.couldNotSaveAgentProfile');
      toast({ variant: 'error', title: commonT('toast.couldNotSaveAgent'), description });
    } finally {
      setIsSaving(false);
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
        />
      </div>

      <div className="flex items-center gap-3">
        <Button
          type="button"
          variant="primary"
          onClick={() => void saveProfile()}
          disabled={!canSave || isSaving}
          data-testid="settings-save-agent"
        >
          {isSaving ? t('agent.saving') : t('agent.save')}
        </Button>
      </div>
    </div>
  );
}

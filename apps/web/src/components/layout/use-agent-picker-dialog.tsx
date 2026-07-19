import { useCallback, useMemo, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import {
  AgentPicker,
  type AgentPickerItem,
  type AgentPickerStatus,
} from '@/components/setup/agent-picker';
import { useScanAgents } from '@/api/queries';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useDesktopCapabilities } from '@/lib/client-context';
import { queryKeys } from '@/lib/nexus/query-keys';
import {
  defaultGridEntries,
  moreAgentsEntries,
  buildPickerSelection,
} from '@/lib/agent-catalog';

function catalogItemToPickerItem(item: {
  pickerId: string;
  name: string;
  displayName?: string | null;
  version?: string | null;
  description?: string | null;
  iconUrl?: string | null;
  installed: boolean;
  installUrl?: string | null;
  docsUrl?: string | null;
}): AgentPickerItem {
  return {
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

export interface AgentPickerDialogHandle {
  open: boolean;
  setOpen: (open: boolean) => void;
  dialog: React.ReactNode;
}

export function useAgentPickerDialog(): AgentPickerDialogHandle {
  const [open, setOpen] = useState(false);
  const { t } = useTranslation('settings');
  const desktop = useDesktopCapabilities();
  const qc = useQueryClient();
  const scan = useScanAgents({ filter: 'all', registry_refresh: true });
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
  const pickerSelection = useMemo(() => buildPickerSelection(agents), [agents]);

  const [selectedId, setSelectedId] = useState<string | null>(null);

  const handleSelect = useCallback(
    (id: string) => {
      const agent = pickerSelection.byPickerId.get(id);
      if (!agent?.installed || !desktop) return;
      setSelectedId(id);
      const name = agent.name;
      const launchCommand = agent.launch_command ?? undefined;
      void desktop.setAgentProfile(name, launchCommand).then(() => {
        void qc.invalidateQueries({ queryKey: queryKeys.agentProfile.detail() });
        void qc.invalidateQueries({
          queryKey: queryKeys.agentHost.scan({ filter: 'all' }),
        });
        setOpen(false);
      });
    },
    [pickerSelection, desktop, qc],
  );

  const dialog = (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent title={t('agent.title')}>
        <AgentPicker
          status={status}
          defaultGrid={status === 'ready' ? defaultGrid : []}
          moreAgents={status === 'ready' ? moreAgents : []}
          selectedId={selectedId}
          onSelect={handleSelect}
          errorDescription={
            scan.isError
              ? (scan.error instanceof Error ? scan.error.message : undefined)
              : undefined
          }
          onRetry={scan.isError ? () => void scan.refetch() : undefined}
          emptyTitle={t('agent.emptyTitle')}
          desktop={desktop ?? undefined}
        />
      </DialogContent>
    </Dialog>
  );

  return { open, setOpen, dialog };
}
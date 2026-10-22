/**
 * Agent discovery + selection for the onboarding wizard (plan v1219-onboarding
 * T2, compass D24/D27; shared by the creator AI-partner step and the developer
 * agent/workspace step).
 *
 * Discovery reuses the daemon scan (`POST /v1/daemon/agent-host/scan`, filter
 * `installed`) resolved through the existing catalog helpers. The selected
 * profile is persisted as `{ name, launchCommand? }` — the same snapshot the
 * shell's agent chip reads:
 *   - browser: `localStorage` under `nexus-agent-profile:<creator_id>`
 *   - desktop: the `set_agent_profile` IPC (unchanged, `config.toml`)
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { User } from 'lucide-react';

import { useAgentProfile, useScanAgents } from '@/api/queries';
import { useActiveCreatorId } from '@/lib/active-creator-context';
import { useDesktopCapabilities } from '@/lib/client-context';
import { resolveCatalogItems, type AgentCatalogItem } from '@/lib/agent-catalog';
import { errorMessage } from '@/lib/error-message';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';
import { cn } from '@/lib/utils';
import { Badge, Button, Input, Label } from '@/components/ui';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

export const AGENT_PROFILE_PREFIX = 'nexus-agent-profile:';

export interface AgentProfile {
  name: string;
  launchCommand?: string;
}

function readStoredProfile(creatorId: string | null): AgentProfile | null {
  if (!creatorId || typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage.getItem(`${AGENT_PROFILE_PREFIX}${creatorId}`);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (parsed === null || typeof parsed !== 'object') return null;
    const { name, launchCommand } = parsed as Partial<AgentProfile>;
    if (typeof name !== 'string' || name.trim() === '') return null;
    return { name, launchCommand: typeof launchCommand === 'string' ? launchCommand : undefined };
  } catch {
    return null;
  }
}

function writeStoredProfile(creatorId: string | null, profile: AgentProfile): void {
  // No creator → no profile key to scope the entry by (`<creator_id>` is part
  // of the contract); the chip ignores a creator-less entry anyway.
  if (!creatorId || typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(
      `${AGENT_PROFILE_PREFIX}${creatorId}`,
      JSON.stringify(profile),
    );
  } catch {
    // Storage may be disabled in some environments; persistence is best-effort.
  }
}

/** One selectable scan row with its collision-safe picker id. */
export interface AgentChoice {
  pickerId: string;
  entry: AgentScanEntry;
  item: AgentCatalogItem;
}

/** Installed, picker-visible scan rows in scan order (recommended = first). */
export function buildAgentChoices(entries: AgentScanEntry[]): AgentChoice[] {
  const items = resolveCatalogItems(entries);
  return items
    .map((item, index) => ({ pickerId: item.pickerId, entry: entries[index]!, item }))
    .filter((choice) => choice.entry.installed && !choice.item.excludeFromPicker);
}

function matchChoice(choices: AgentChoice[], profile: AgentProfile): AgentChoice | null {
  const byName = choices.find(
    (choice) => choice.item.displayName === profile.name || choice.item.name === profile.name,
  );
  if (byName) return byName;
  const command = profile.launchCommand?.trim();
  if (!command) return null;
  return choices.find((choice) => choice.item.launchCommand?.trim() === command) ?? null;
}

function profileOfChoice(choice: AgentChoice): AgentProfile {
  // The scan entry name, not the catalog display name: `resolveAgentBadge`
  // matches a saved profile against `AgentScanEntry.name`.
  const launchCommand = choice.item.launchCommand?.trim();
  return {
    name: choice.entry.name,
    launchCommand: launchCommand?.length ? launchCommand : undefined,
  };
}

export type AgentScanStatus = 'loading' | 'ready' | 'empty' | 'error';

export interface AgentSelection {
  status: AgentScanStatus;
  /** Human-readable scan failure ('' when the error carries no message). */
  errorReason: string;
  choices: AgentChoice[];
  /** First installed row — preselected when nothing was saved yet. */
  recommendedId: string | null;
  selectedId: string | null;
  select: (pickerId: string) => void;
  manual: string;
  setManual: (command: string) => void;
  retry: () => void;
  /**
   * Persist the current profile (a manual command overrides the selected row).
   * Resolves `true` when the selection was saved; `false` when the desktop
   * write failed (one error toast shown) — callers must not advance.
   */
  commit: () => Promise<boolean>;
  canContinue: boolean;
}

export function useAgentSelection(): AgentSelection {
  const { t: commonT } = useTranslation('common');
  const { toast } = useToast();
  const desktop = useDesktopCapabilities();
  const qc = useQueryClient();
  const creatorId = useActiveCreatorId();
  const savedProfile = useAgentProfile();
  const scan = useScanAgents({ filter: 'installed' });

  const choices = useMemo(
    () => (scan.data ? buildAgentChoices(scan.data.agents) : []),
    [scan.data],
  );
  const recommendedId = choices.find((choice) => choice.entry.installed)?.pickerId ?? null;

  const stored = useMemo(() => readStoredProfile(creatorId), [creatorId]);
  const saved = savedProfile.data ?? stored;
  // Browser reads localStorage synchronously; the desktop profile arrives over
  // IPC and may land after the scan. Locking the restore before that read
  // settles would leave a late profile unmapped, so wait for it here.
  const profileSettled = desktop ? savedProfile.isFetched : true;

  const [pickedId, setPickedId] = useState<string | null>(null);
  const [manual, setManual] = useState('');
  const restored = useRef(false);

  // Restore the saved profile once the scan and the desktop read have settled.
  // A row-backed profile re-selects its row; a saved command that differs from
  // the row's own is a manual override and must come back into the field so a
  // later commit does not overwrite it. With zero scan rows (empty/error) the
  // saved command is the only usable path, so it still fills the field and
  // lets the step continue.
  const scanSettled = !scan.isLoading;
  useEffect(() => {
    if (restored.current || !profileSettled || !scanSettled) return;
    restored.current = true;
    if (!saved) return;
    const match = matchChoice(choices, saved);
    if (match) {
      setPickedId(match.pickerId);
      const savedCommand = saved.launchCommand?.trim();
      if (savedCommand && savedCommand !== match.item.launchCommand?.trim()) {
        setManual(savedCommand);
      }
      return;
    }
    setManual(saved.launchCommand ?? '');
  }, [choices, saved, profileSettled, scanSettled]);

  const selectedId = pickedId ?? recommendedId;
  const selectedChoice = choices.find((choice) => choice.pickerId === selectedId) ?? null;

  const status: AgentScanStatus = scan.isLoading
    ? 'loading'
    : scan.isError
      ? 'error'
      : choices.length === 0
        ? 'empty'
        : 'ready';

  function persist(profile: AgentProfile): Promise<boolean> {
    if (!desktop) {
      writeStoredProfile(creatorId, profile);
      return Promise.resolve(true);
    }
    return desktop
      .setAgentProfile(profile.name, profile.launchCommand)
      .then(() => {
        // The status bar and agent chip read the saved profile + scan.
        void qc.invalidateQueries({ queryKey: queryKeys.agentProfile.detail() });
        void qc.invalidateQueries({ queryKey: queryKeys.agentHost.scan({ filter: 'all' }) });
        return true;
      })
      .catch((error) => {
        // Logged here; the toast is shown by commit() so a failed row click
        // does not double-report — advance retries the same write.
        console.error('setAgentProfile failed', error);
        return false;
      });
  }

  function select(pickerId: string) {
    const choice = choices.find((candidate) => candidate.pickerId === pickerId);
    if (!choice) return;
    setPickedId(pickerId);
    setManual('');
    void persist(profileOfChoice(choice));
  }

  async function commit(): Promise<boolean> {
    const command = manual.trim();
    const profile = command
      ? { name: selectedChoice?.entry.name ?? command, launchCommand: command }
      : selectedChoice
        ? profileOfChoice(selectedChoice)
        : null;
    if (!profile) return true;
    const persisted = await persist(profile);
    if (!persisted) {
      toast({ variant: 'error', title: commonT('error.couldNotSaveAgentProfile') });
    }
    return persisted;
  }

  const canContinue = status !== 'loading' && (manual.trim() !== '' || selectedId !== null);

  return {
    status,
    errorReason: errorMessage(scan.error),
    choices,
    recommendedId,
    selectedId,
    select,
    manual,
    setManual,
    retry: () => {
      void scan.refetch();
    },
    commit,
    canContinue,
  };
}

interface AgentRowProps {
  choice: AgentChoice;
  description: string;
  selected: boolean;
  onSelect: () => void;
  /** Optional protocol pill (developer surface only). */
  protocol?: string;
}

/** One selectable agent row: icon, name, version, description, ✓ when selected. */
export function AgentRow({ choice, description, selected, onSelect, protocol }: AgentRowProps) {
  const { item } = choice;
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      data-testid={`agent-row-${choice.pickerId}`}
      className={cn(
        'flex w-full items-center gap-4 rounded-card border-2 bg-background-100 px-5 py-4 text-left transition-colors duration-state ease-standard motion-reduce:transition-none',
        selected ? 'border-blue-700' : 'border-gray-alpha-400 hover:border-blue-700',
      )}
    >
      {item.iconUrl ? (
        <img src={item.iconUrl} alt="" className="h-7 w-7 shrink-0 rounded-sm object-contain" />
      ) : (
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-sm bg-gray-alpha-200">
          <User className="h-4 w-4 text-gray-500" aria-hidden />
        </span>
      )}
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="flex items-center gap-2">
          <span className="truncate text-heading-16 font-heading text-gray-1000">
            {item.displayName}
          </span>
          {item.version ? (
            <span className="text-label-12 text-gray-700">{item.version}</span>
          ) : null}
          {protocol ? (
            <Badge variant="neutral" tone="soft">
              {protocol}
            </Badge>
          ) : null}
        </span>
        <span className="mt-1 text-copy-13 text-gray-900">{description}</span>
      </span>
      <span
        aria-hidden="true"
        className={cn('text-heading-16 text-blue-700', selected ? '' : 'invisible')}
      >
        ✓
      </span>
    </button>
  );
}

interface ManualCommandFieldProps {
  id: string;
  value: string;
  onChange: (command: string) => void;
  placeholder?: string;
  label?: string;
}

/** Collapsed-link manual launch command entry shared by both steps. */
export function ManualCommandField({
  id,
  value,
  onChange,
  placeholder,
  label,
}: ManualCommandFieldProps) {
  return (
    <div className="flex flex-col gap-2">
      {label ? (
        <Label htmlFor={id} className="text-label-14 font-medium text-gray-1000">
          {label}
        </Label>
      ) : null}
      <Input
        id={id}
        type="text"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        aria-label={label ? undefined : '启动命令'}
      />
    </div>
  );
}

/** Scan-error inline surface shared by both steps. */
export function ScanErrorNotice({
  reason,
  onRetry,
}: {
  reason: string;
  onRetry: () => void;
}) {
  return (
    <div role="alert" className="flex flex-col items-center gap-3">
      <p className="text-copy-14 text-gray-900">
        {reason ? `暂时无法扫描本机 AI（${reason}）` : '暂时无法扫描本机 AI'}
      </p>
      <Button variant="secondary" size="small" onClick={onRetry}>
        重试扫描
      </Button>
    </div>
  );
}

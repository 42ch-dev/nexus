/**
 *
 * One instance at a time: full-screen while the wizard owns the screen (first
 * run, `/setup`), a top banner inside the shell. Repeated failures update this
 * same node — the probe never raises a toast.
 *
 * The engine is the one subsystem the product takes over the whole screen for:
 * no provider means nothing can run, and the fix is a concrete CLI step, so it
 * gets a repair page rather than a banner over a dead app. Other causes keep
 * the full-screen/banner split.
 *
 * Copy lives in the `setup` namespace (`degraded.*`): the zh-CN body is the
 * product-owned string verbatim, and the EN namespace is the fallback.
 */
import type { TFunction } from 'i18next';
import type { AgentScanEntry } from '@42ch/nexus-contracts';
import { useEffect, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useLocation } from 'react-router';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select } from '@/components/ui/select';
import { useNexusClient } from '@/lib/client-context';
import { catalogPriority } from '@/lib/agent-catalog';
import type { AgentHostCatalogEntry, AgentHostProviderConfig } from '@/lib/nexus/types';
import type { ServiceDegradedReason } from '@/lib/service-status-context';
import { useOptionalServiceStatus, useServiceStatus } from '@/lib/service-status-context';
import { useSetupCompleted } from '@/lib/setup-completed-context';

function reasonText(
  reason: Exclude<ServiceDegradedReason, { kind: 'provider_unselected' }>,
  t: TFunction<'setup'>,
): string {
  switch (reason.kind) {
    case 'unreachable':
      return t('degraded.reason.unreachable');
    case 'server':
      return reason.message;
    case 'subsystems':
      return reason.subsystems
        .map((name) => t(`degraded.subsystem.${name}`, { defaultValue: name }))
        .join(t('degraded.separator'));
  }
}

type EngineFailure = Exclude<ServiceDegradedReason, { kind: 'unreachable' | 'server' }>;

/** The engine is the only degraded subsystem: the workspace cannot run tasks. */
function isEngineFailure(reason: ServiceDegradedReason): reason is EngineFailure {
  return (
    reason.kind === 'provider_unselected' ||
    (reason.kind === 'subsystems' &&
      reason.subsystems.length === 1 &&
      reason.subsystems[0] === 'engine')
  );
}

const SUPPORTED_NATIVE_LAUNCH: Record<string, true> = {
  'dsh-native': true,
  'codex-native': true,
  'claude-native': true,
};

/** One actionable candidate: catalog id/protocol, scan install state/command. */
interface ProviderCandidate {
  id: string;
  displayName: string;
  protocol: AgentHostProviderConfig['protocol'];
  command: string | null;
  /** Scan-provided launch arguments (e.g. ["acp"] for `omp acp`). */
  args: string[];
  installed: boolean;
}

function ProviderSetupForm({ onSaved }: { onSaved: () => Promise<void> }) {
  const { t } = useTranslation('setup');
  const client = useNexusClient();
  const [catalog, setCatalog] = useState<AgentHostCatalogEntry[] | null>(null);
  const [scanEntries, setScanEntries] = useState<AgentScanEntry[]>([]);
  const [scanFailed, setScanFailed] = useState(false);
  const [catalogFailed, setCatalogFailed] = useState(false);
  const [savedProviders, setSavedProviders] = useState<
    Map<string, AgentHostProviderConfig>
  >(new Map());
  const [id, setId] = useState('');
  const [protocol, setProtocol] = useState<AgentHostProviderConfig['protocol']>('acp');
  const [command, setCommand] = useState('');
  const [args, setArgs] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void client.listAgentHostProviders().then(
      (response) => {
        if (!cancelled) setCatalog(response.providers);
      },
      () => {
        if (!cancelled) {
          setCatalog([]);
          setCatalogFailed(true);
        }
      },
    );
    void client.getAgentHostConfig().then(
      (config) => {
        if (!cancelled) {
          setSavedProviders(new Map(config.providers.map((p) => [p.id, p])));
        }
      },
      () => {
      },
    );
    void client.scanAgents().then(
      (response) => {
        if (!cancelled) setScanEntries(response.agents);
      },
      () => {
        if (!cancelled) {
          setScanEntries([]);
          setScanFailed(true);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [client]);

  const choose = (candidate: ProviderCandidate) => {
    const configured = savedProviders.get(candidate.id);
    setId(candidate.id);
    setProtocol(candidate.protocol);
    setCommand(candidate.command ?? configured?.command ?? '');
    setArgs(candidate.args.length > 0 ? candidate.args : (configured?.args ?? []));
  };

  const save = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      const current = await client.getAgentHostConfig();
      const provider: AgentHostProviderConfig = {
        id: id.trim(),
        protocol,
        command: command.trim(),
        args,
        enabled: true,
      };
      // Known-good entries stay in the document (disabled) so settings can
      // re-enable them later; they are never deleted here.
      const exists = current.providers.some((entry) => entry.id === provider.id);
      const providers = current.providers.map((entry) =>
        entry.id === provider.id ? { ...entry, ...provider } : { ...entry, enabled: false },
      );
      if (!exists) providers.push(provider);
      await client.saveAgentHostConfig({ ...current, providers });
      await onSaved();
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const scanKey = (providerId: string, protocol: string | null | undefined) =>
    `${providerId}\u0000${protocol ?? 'unknown'}`;
  const scanByKey = new Map<string, AgentScanEntry>();
  for (const entry of scanEntries) {
    const providerId = entry.registry_agent_id;
    if (!providerId) continue;
    const key = scanKey(providerId, entry.protocol_kind);
    const existing = scanByKey.get(key);
    const existingHasRecipe = (existing?.launch_args ?? []).length > 0;
    const entryHasRecipe = (entry.launch_args ?? []).length > 0;
    if (!existing || (!existingHasRecipe && entryHasRecipe)) {
      scanByKey.set(key, entry);
    }
  }
  const catalogById = new Map(
    (catalog ?? []).map((entry) => [entry.provider_id, entry] as const),
  );
  const providerIdOf = (key: string) => key.slice(0, key.indexOf('\u0000'));
  const candidateKeys: string[] = [];
  const seenCandidateKeys = new Set<string>();
  const addCandidateKey = (key: string) => {
    if (seenCandidateKeys.has(key)) return;
    seenCandidateKeys.add(key);
    candidateKeys.push(key);
  };
  for (const [providerId, row] of catalogById) {
    addCandidateKey(scanKey(providerId, row.protocol_kind));
  }
  for (const key of scanByKey.keys()) {
    const providerId = providerIdOf(key);
    if (savedProviders.has(providerId)) {
      const savedProtocol = savedProviders.get(providerId)?.protocol;
      const authoritativeProtocol =
        catalogById.get(providerId)?.protocol_kind ?? savedProtocol;
      if (catalogById.has(providerId)) continue;
      const keyProtocol = key.slice(key.indexOf('\u0000') + 1);
      if (keyProtocol !== authoritativeProtocol) continue;
      if (scanByKey.get(key)?.installed !== true) continue;
      addCandidateKey(key);
      continue;
    }
    const protocol = key.slice(key.indexOf('\u0000') + 1);
    if (protocol === 'unknown' && catalogById.has(providerId)) continue;
    addCandidateKey(key);
  }
  const candidates: ProviderCandidate[] = candidateKeys.flatMap((key) => {
    const providerId = providerIdOf(key);
    const row = catalogById.get(providerId);
    const scanned =
      scanByKey.get(key) ?? scanByKey.get(scanKey(providerId, undefined));
    const candidateProtocol = savedProviders.has(providerId)
      ? (row?.protocol_kind ?? scanned?.protocol_kind)
      : (scanned?.protocol_kind ?? row?.protocol_kind);
    if (candidateProtocol !== 'acp' && candidateProtocol !== 'native_cli') return [];
    return [
      {
        id: providerId,
        displayName: row?.display_name ?? scanned?.name ?? providerId,
        protocol: candidateProtocol,
        command: scanned?.launch_command ?? null,
        args: scanned?.launch_args ?? [],
        installed: scanned?.installed ?? false,
      },
    ];
  });
  const representedIds = new Set(candidates.map((candidate) => candidate.id));
  for (const [providerId, saved] of savedProviders) {
    if (representedIds.has(providerId)) continue;
    candidates.push({
      id: providerId,
      displayName: providerId,
      protocol: saved.protocol,
      command: saved.command ?? null,
      args: saved.args ?? [],
      installed: false,
    });
  }
  const configured = (c: ProviderCandidate) => (savedProviders.has(c.id) ? 1 : 0);
  const orderedCandidates = [...candidates].sort((left, right) => {
    return (
      Number(right.installed) - Number(left.installed) ||
      configured(right) - configured(left) ||
      (catalogPriority(left.id) ?? Number.POSITIVE_INFINITY) -
        (catalogPriority(right.id) ?? Number.POSITIVE_INFINITY) ||
      left.displayName.localeCompare(right.displayName)
    );
  });

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-copy-16 text-gray-1000">{t('degraded.repair.form.title')}</h2>
      <div className="flex flex-col gap-2">
        <h3 className="text-copy-14 text-gray-1000">{t('degraded.repair.form.candidatesTitle')}</h3>
        {catalogFailed && orderedCandidates.length === 0 && (
          <p data-testid="provider-catalog-failed" className="text-copy-13 text-red-1000">
            {t('degraded.repair.form.candidatesError')}
          </p>
        )}
        {orderedCandidates.length > 0 && (
          <ul className="flex max-h-56 list-none flex-col gap-1 overflow-y-auto p-0">
            {orderedCandidates.map((candidate) => {
              const configured = savedProviders.has(candidate.id);
              const launchable =
                candidate.protocol === 'acp' || SUPPORTED_NATIVE_LAUNCH[candidate.id] === true;
              const installState = scanFailed
                ? 'unknown'
                : candidate.installed
                  ? 'installed'
                  : 'missing';
              return (
                <li
                  key={`${candidate.id}:${candidate.protocol}`}
                  className="flex items-center justify-between gap-3 rounded-md bg-background-200 px-3 py-2"
                >
                  <span className="min-w-0 truncate text-copy-13 text-gray-1000">
                    {candidate.displayName}
                    {!launchable && !configured ? (
                      <span
                        data-testid="detection-only-badge"
                        className="ml-2 text-label-12 text-gray-500"
                      >
                        {t('degraded.repair.form.detectionOnly')}
                      </span>
                    ) : installState !== 'missing' || configured ? (
                      <span className="ml-2 text-label-12 text-gray-700">
                        {installState === 'installed'
                          ? t('degraded.repair.form.installed')
                          : installState === 'unknown'
                            ? t('degraded.repair.form.installUnknown')
                            : t('degraded.repair.form.configured')}
                      </span>
                    ) : (
                      <span className="ml-2 text-label-12 text-gray-500">
                        {t('degraded.repair.form.notInstalled')}
                      </span>
                    )}
                  </span>
                  <Button
                    type="button"
                    variant="secondary"
                    size="small"
                    disabled={!configured && (installState === 'missing' || !launchable)}
                    title={
                      !configured && !launchable
                        ? t('degraded.repair.form.detectionOnly')
                        : undefined
                    }
                    onClick={() => choose(candidate)}
                  >
                    {t('degraded.repair.form.enable')}
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="flex flex-col gap-3">
        <h3 className="text-copy-14 text-gray-1000">{t('degraded.repair.form.manualTitle')}</h3>
        <div className="flex flex-col gap-1">
          <label htmlFor="repair-provider-id" className="text-copy-13 text-gray-900">
            {t('degraded.repair.form.idLabel')}
          </label>
          <Input
            id="repair-provider-id"
            value={id}
            placeholder={t('degraded.repair.form.idPlaceholder')}
            onChange={(event) => {
              setId(event.target.value);
              setArgs([]);
            }}
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor="repair-provider-protocol" className="text-copy-13 text-gray-900">
            {t('degraded.repair.form.protocolLabel')}
          </label>
          <Select
            id="repair-provider-protocol"
            value={protocol}
            onChange={(event) => {
              setProtocol(event.target.value as AgentHostProviderConfig['protocol']);
              setArgs([]);
            }}
          >
            <option value="acp">{t('degraded.repair.form.protocolAcp')}</option>
            <option value="native_cli">{t('degraded.repair.form.protocolNativeCli')}</option>
          </Select>
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor="repair-provider-command" className="text-copy-13 text-gray-900">
            {t('degraded.repair.form.commandLabel')}
          </label>
          <Input
            id="repair-provider-command"
            value={command}
            placeholder={t('degraded.repair.form.commandPlaceholder')}
            onChange={(event) => setCommand(event.target.value)}
          />
        </div>
        {saveError && (
          <p data-testid="provider-save-failed" className="text-copy-13 text-red-1000">
            {t('degraded.repair.form.saveFailed', { message: saveError })}
          </p>
        )}
        <Button
          type="button"
          variant="primary"
          disabled={saving || id.trim() === '' || command.trim() === ''}
          onClick={() => void save()}
        >
          {saving ? t('degraded.repair.form.saving') : t('degraded.repair.form.save')}
        </Button>
      </div>
    </section>
  );
}

/**
 *
 * No provider selected: step 1 is the in-page configuration form (scan
 * candidates + manual fields), step 2 is the readiness recheck. A selected
 * provider that failed readiness keeps the failure line ABOVE the same form —
 * the saved profile alone cannot satisfy readiness, so hiding the form would
 * leave a recheck that can never succeed.
 */
function EngineRepair({ reason }: { reason: EngineFailure }) {
  const { t } = useTranslation('setup');
  const { checking, retry } = useServiceStatus();

  return (
    <div
      role="alert"
      data-testid="service-degraded-surface"
      data-variant="full"
      data-state={reason.kind === 'provider_unselected' ? 'unconfigured' : 'failed'}
      className="flex min-h-screen items-center justify-center bg-background-100 p-6"
    >
      <div className="flex w-full max-w-lg flex-col gap-6">
        <h1 className="font-display text-display-24 text-gray-1000">
          {t('degraded.repair.title')}
        </h1>
        <p className="text-copy-14 text-gray-900">{t('degraded.repair.description')}</p>
        {reason.kind !== 'provider_unselected' && (
          <p data-testid="engine-repair-failure" className="text-copy-13 text-red-1000">
            {t('degraded.repair.failure', { reason: reasonText(reason, t) })}
          </p>
        )}
        <ProviderSetupForm onSaved={retry} />
        <section className="flex flex-col items-start gap-2">
          <h2 className="text-copy-16 text-gray-1000">{t('degraded.repair.stepRecheck.title')}</h2>
          <p className="text-copy-14 text-gray-900">{t('degraded.repair.stepRecheck.body')}</p>
          <Button
            type="button"
            variant="secondary"
            disabled={checking}
            onClick={() => void retry()}
          >
            {t('degraded.repair.recheck')}
          </Button>
        </section>
      </div>
    </div>
  );
}

function ServiceUnavailable({
  reason,
  variant,
}: {
  reason: Exclude<ServiceDegradedReason, { kind: 'provider_unselected' }>;
  variant: 'full' | 'banner';
}) {
  const { t } = useTranslation('setup');
  const { checking, retry } = useServiceStatus();
  const body = t('degraded.body', { reason: reasonText(reason, t) });
  const action = (
    <Button
      type="button"
      variant="secondary"
      size={variant === 'banner' ? 'small' : 'default'}
      disabled={checking}
      onClick={() => void retry()}
    >
      {t('degraded.retry')}
    </Button>
  );

  if (variant === 'full') {
    return (
      <div
        role="alert"
        data-testid="service-degraded-surface"
        data-variant="full"
        className="flex min-h-screen items-center justify-center bg-background-100 p-6"
      >
        <div className="flex w-full max-w-md flex-col items-start gap-4">
          <p className="text-copy-14 text-gray-1000">{body}</p>
          {action}
        </div>
      </div>
    );
  }

  return (
    <div
      role="alert"
      data-testid="service-degraded-surface"
      data-variant="banner"
      className="flex flex-wrap items-center justify-between gap-3 border-b border-red-700/30 bg-red-700/10 px-4 py-2 text-copy-13 text-red-1000"
    >
      <p className="min-w-0">{body}</p>
      {action}
    </div>
  );
}

/**
 * Route wrapper: an engine failure takes over every route (nothing can run);
 * the wizard takes the full-screen variant for other causes, and the shell
 * keeps its chrome and renders {@link ServiceDegradedBanner} itself. An
 * unresolved first-run gate means the wizard is the next screen, so it takes
 * the full-screen variant too.
 */
export function ServiceDegradedGate({ children }: { children: ReactNode }) {
  const { reason } = useServiceStatus();
  const { completed } = useSetupCompleted();
  const { pathname } = useLocation();

  if (reason !== null) {
    if (isEngineFailure(reason)) return <EngineRepair reason={reason} />;
    if (!completed || pathname === '/setup') {
      return <ServiceUnavailable reason={reason} variant="full" />;
    }
  }
  return <>{children}</>;
}

/** Shell-level variant; renders nothing while the service is healthy. */
export function ServiceDegradedBanner() {
  const status = useOptionalServiceStatus();
  const reason = status?.reason;
  return reason == null || isEngineFailure(reason) ? null : (
    <ServiceUnavailable reason={reason} variant="banner" />
  );
}

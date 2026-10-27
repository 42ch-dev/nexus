/**
 * Service-unavailable surface (plan v1219-onboarding T4).
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

/**
 * The `{reason}` slot. A server-provided message is already human copy, so it
 * is kept as-is; transport and subsystem reasons are localized.
 */
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

/**
 * Native provider ids that have a real launch adapter in this build. Every
 * other native_cli detection row is detection-only: rendered for visibility,
 * never offered for launch (the daemon would honestly refuse at admission).
 */
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
  // A failed scan means install state is UNKNOWN, not "not installed":
  // candidates stay selectable so a transient scan outage never locks the
  // user out of the catalog.
  const [scanFailed, setScanFailed] = useState(false);
  const [catalogFailed, setCatalogFailed] = useState(false);
  // Enabled provider rows from the on-disk config, by id. A configured
  // provider is the thing being repaired: it stays selectable even when the
  // scan has no row for it (the scan skips enabled configured providers, and
  // catalog-only targets like gemini have no detection row at all), and its
  // stored command/args back the form when the scan cannot supply them.
  const [configuredRows, setConfiguredRows] = useState<
    Map<string, AgentHostProviderConfig>
  >(new Map());
  const [id, setId] = useState('');
  const [protocol, setProtocol] = useState<AgentHostProviderConfig['protocol']>('acp');
  const [command, setCommand] = useState('');
  // Scan-provided launch args for the chosen candidate (e.g. ["acp"]); the
  // manual path keeps them empty. Without them an ACP subcommand agent
  // (omp/hermes) would be spawned bare and never speak ACP.
  const [args, setArgs] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    // The catalog is authoritative for provider id + protocol; the scan only
    // annotates install state and the launch command. A scan failure therefore
    // costs the badges, never the list.
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
          setConfiguredRows(
            new Map(
              config.providers
                .filter((p) => p.enabled)
                .map((p) => [p.id, p]),
            ),
          );
        }
      },
      () => {
        // A failed config read only costs the configured-row affordance.
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
    // Prefer scan-provided launch data; fall back to what the user already
    // had configured, so choosing a configured row without a scan row never
    // blanks the stored command/args.
    const configured = configuredRows.get(candidate.id);
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
      // Replacement semantics: the saved provider is the ONE enabled entry.
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

  // Candidates are the union of the machine scan (discoverable agents on an
  // unconfigured home, with install state + launch recipe) and the provider
  // catalog (authoritative display name/protocol + curated priority for rows
  // the scan does not emit, e.g. a configured target without a detection row).
  // One row per provider id: a curated native row carries the launch recipe
  // (e.g. ["acp"]) and must win over a bare registry row for the same id —
  // keeping the bare one would save a binary that never speaks ACP.
  const scanById = new Map<string, AgentScanEntry>();
  for (const entry of scanEntries) {
    const providerId = entry.registry_agent_id;
    if (!providerId) continue;
    const existing = scanById.get(providerId);
    const existingHasRecipe = (existing?.launch_args ?? []).length > 0;
    const entryHasRecipe = (entry.launch_args ?? []).length > 0;
    if (!existing || (!existingHasRecipe && entryHasRecipe)) {
      scanById.set(providerId, entry);
    }
  }
  const catalogById = new Map(
    (catalog ?? []).map((entry) => [entry.provider_id, entry] as const),
  );
  const candidateIds = [...new Set([...scanById.keys(), ...catalogById.keys()])];
  const candidates: ProviderCandidate[] = candidateIds.flatMap((candidateId) => {
    const scanned = scanById.get(candidateId);
    const row = catalogById.get(candidateId);
    const candidateProtocol = scanned?.protocol_kind ?? row?.protocol_kind;
    if (candidateProtocol !== 'acp' && candidateProtocol !== 'native_cli') return [];
    return [
      {
        id: candidateId,
        displayName: row?.display_name ?? scanned?.name ?? candidateId,
        protocol: candidateProtocol,
        command: scanned?.launch_command ?? null,
        args: scanned?.launch_args ?? [],
        installed: scanned?.installed ?? false,
      },
    ];
  });
  const configured = (c: ProviderCandidate) => (configuredRows.has(c.id) ? 1 : 0);
  // Sort: installed first, then configured (the repair target), then curated
  // priority (unranked last, display name as the final tie-break); the
  // catalog's registry order falls out as the stable default.
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
              // A configured provider is the repair target itself: it stays
              // selectable regardless of scan state (the scan skips enabled
              // configured providers, so its row is missing by design).
              const configured = configuredRows.has(candidate.id);
              // Generic ACP recipes launch through the shared ACP adapter;
              // only the built-in trio has native launch adapters. Other
              // native ids are detection-only: shown, never offered.
              const launchable =
                candidate.protocol === 'acp' || SUPPORTED_NATIVE_LAUNCH[candidate.id] === true;
              const installState = scanFailed
                ? 'unknown'
                : candidate.installed
                  ? 'installed'
                  : 'missing';
              return (
                <li
                  key={candidate.id}
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
                    // Disabled for a scan-confirmed absent target and for
                    // detection-only native rows without a launch adapter
                    // (unless the user configured them — the repair target
                    // itself must stay editable).
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
            // Manual edits leave the chosen scan row's recipe: the hidden
            // launch args belong to that row and must not leak into a
            // different manual provider.
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
            // Correcting the executable path of the SAME recipe keeps the
            // scan-provided args (omp → /opt/homebrew/bin/omp still needs
            // "acp"); only switching id or protocol invalidates them.
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
 * Full-screen repair page for an engine that cannot serve the workspace.
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
  // Engine failures never reach the shell: the gate owns the whole screen.
  return reason == null || isEngineFailure(reason) ? null : (
    <ServiceUnavailable reason={reason} variant="banner" />
  );
}

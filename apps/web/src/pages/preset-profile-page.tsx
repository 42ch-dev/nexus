/**
 * Per-preset profile drill-down (V1.171 P1 — PL-13, AR-27).
 *
 * Selecting a preset from the strategy catalog opens this view at
 * `/strategies/:presetId/profile`. It renders the P0 preset profile
 * (`GET /v1/daemon/orchestration/presets/:id/profile`) consumed through
 * `NexusClient` only (AR-27): outer state-machine states (enter /
 * exit-when / next), roles + recommended skills, and required capabilities
 * deep-linked to the existing capability-schema browser (PL-13).
 *
 * A missing profile renders a graceful summary — preset id + list facts
 * (source) — never a hard error implying the preset is gone when the list
 * already showed it (PL-13). The trigger-lane classification renders with
 * the locked vocabulary (PL-11 — cron vs wall-clock poller vs `scheduled_at`)
 * and declared `signals` render with the PL-10 honesty copy ("Declared, not
 * delivered" + lifecycle pointer). No next-run value exists in the profile,
 * so none is fabricated (PL-12).
 *
 * Route: the canvas stays at `/strategies/:presetId` (PL-14 write-boundary
 * preserved); this view is a sibling at a trailing `/profile` segment
 * (AR-20 literal-segment precedent). The existing develop-only +
 * `allowDeepLink` entrance rule for `/strategies/:presetId` covers the deep
 * link (AR-28 — no registry change needed).
 */
import { ArrowLeft } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate, useParams } from 'react-router';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, LoadingState, UnavailableState } from '@/components/ui/states';
import { usePresetProfile, usePresets } from '@/api/queries';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';
import type {
  PresetProfileExitWhen,
  PresetProfileNext,
} from '@/lib/nexus';

/** Human-readable exit-when summary for a state (locked vocabulary, PL-3). */
function exitWhenSummary(exitWhen: PresetProfileExitWhen): string {
  if (exitWhen.kind === 'llm_judge') {
    const parts = ['llm_judge'];
    if (exitWhen.judgeCapability) parts.push(`judge: ${exitWhen.judgeCapability}`);
    if (exitWhen.templateFile) parts.push(`template: ${exitWhen.templateFile}`);
    if (exitWhen.minInterval) parts.push(`min interval: ${exitWhen.minInterval}`);
    return parts.join(' · ');
  }
  if (exitWhen.kind === 'timer' && exitWhen.duration) {
    return `timer · ${exitWhen.duration}`;
  }
  return exitWhen.kind;
}

/**
 * Trigger-lane classification row (PL-3 locked vocabulary; PL-11/PL-12).
 *
 * Renders the lane's presence honestly (Yes/No from the profile flags) with
 * the locked vocabulary detail. The profile carries no next-fire value, so
 * no next-run clock is shown or derived.
 */
function LaneRow({
  testId,
  label,
  present,
  detail,
}: {
  testId: string;
  label: string;
  present: boolean;
  detail: string;
}) {
  const { t } = useTranslation('strategies');
  return (
    <li
      className="flex flex-col gap-1 rounded-card border border-gray-alpha-400 p-3"
      data-testid={testId}
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-copy-13-mono text-gray-1000">{label}</span>
        <Badge
          variant={present ? 'running' : 'neutral'}
          data-testid={`${testId}-${present ? 'yes' : 'no'}`}
        >
          {present ? t('profile.laneYes') : t('profile.laneNo')}
        </Badge>
      </div>
      <span className="text-copy-12 text-gray-700">{detail}</span>
    </li>
  );
}

/** Human-readable next-transition summary for a state (never a fabricated
 * next-fire time — PL-12: only the declared transition form is shown). */
function nextSummary(next: PresetProfileNext): string {
  switch (next.kind) {
    case 'linear':
      return `linear → ${next.target ?? '—'}`;
    case 'goNogo':
      return `goNogo → go: ${next.go ?? '—'} / nogo: ${next.nogo ?? '—'}`;
    case 'labeled': {
      const edges = (next.labeled ?? []).map((e) => `${e.label} → ${e.target}`);
      return `labeled → ${edges.length > 0 ? edges.join(', ') : '—'}`;
    }
    case 'conditional':
    case 'branches': {
      const rules = (next.kind === 'branches' ? next.branches : next.rules) ?? [];
      const parts = rules.map((r) => `${r.when} → ${r.target}`);
      if (next.default) parts.push(`default → ${next.default}`);
      return `${next.kind} → ${parts.length > 0 ? parts.join(', ') : '—'}`;
    }
    default:
      return next.kind;
  }
}

export function PresetProfilePage() {
  const { t } = useTranslation('strategies');
  const { presetId } = useParams<{ presetId: string }>();
  const navigate = useNavigate();
  const profile = usePresetProfile(presetId);
  // List facts (id + source) for the graceful missing-profile summary
  // (PL-13) — the list already showed the preset, so the summary must not
  // imply it is gone. Same query key as the catalog page: cached.
  const presets = usePresets();

  function handleBack() {
    navigate('/strategies');
  }

  if (profile.isLoading) {
    return <LoadingState label={t('profile.loading')} />;
  }

  if (profile.isError) {
    const retry = () => void profile.refetch();
    const backButton = (
      <Button type="button" variant="secondary" size="small" onClick={handleBack}>
        <ArrowLeft className="h-4 w-4" aria-hidden />
        {t('profile.back')}
      </Button>
    );

    if (isOrchestrationEngineUnavailable(profile.error)) {
      return (
        <UnavailableState
          title={t('engineUnavailableTitle')}
          description={t('engineUnavailableDescription')}
          onRetry={retry}
          action={backButton}
        />
      );
    }

    // Graceful summary: id + list facts (source) + honest copy. The preset
    // may still exist in the list — never a hard "not found" error (PL-13).
    const all = presets.data
      ? [...presets.data.user, ...presets.data.system, ...presets.data.embedded]
      : [];
    const summary = all.find((p) => p.id === presetId);
    const sourceLabel = summary
      ? summary.source === 'user'
        ? t('catalog.sourceUser')
        : summary.source === 'embedded'
          ? t('catalog.sourceEmbedded')
          : t('profile.sourceSystem')
      : null;

    return (
      <div className="flex flex-col gap-4">
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={handleBack}
          className="self-start"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden />
          {t('profile.back')}
        </Button>
        <Card className="shadow-card" data-testid="preset-profile-unavailable">
          <CardHeader>
            <CardTitle>{t('profile.unavailableTitle')}</CardTitle>
            <CardDescription>
              {t('profile.unavailableDescription', { name: presetId ?? '' })}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-col gap-2">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-copy-13-mono text-gray-1000">{presetId}</span>
                {sourceLabel && (
                  <Badge variant="preset" data-testid="profile-unavailable-source">
                    {sourceLabel}
                  </Badge>
                )}
              </div>
              <div className="flex gap-2">
                <Button type="button" variant="primary" size="small" onClick={retry}>
                  {t('profile.retry')}
                </Button>
                {backButton}
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  const data = profile.data;
  if (!data) {
    return <EmptyState title={t('profile.unavailableTitle')} />;
  }

  const { lanes, states, roles, requiredCapabilities, signals } = data;

  return (
    <div className="flex flex-col gap-4">
      <div>
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={handleBack}
          className="mb-2"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden />
          {t('profile.back')}
        </Button>
        <h1 className="text-heading-24 font-heading text-gray-1000">{t('profile.title')}</h1>
        <p className="text-copy-14 text-gray-900">{t('profile.description')}</p>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <span className="text-copy-13-mono text-gray-1000" data-testid="profile-id">
            {data.id}
          </span>
          <Badge variant="neutral" data-testid="profile-version">
            v{data.version}
          </Badge>
        </div>
      </div>

      <Card className="shadow-card" data-testid="profile-lanes">
        <CardHeader>
          <CardTitle>{t('profile.lanesTitle')}</CardTitle>
          <CardDescription>{t('profile.lanesDescription')}</CardDescription>
        </CardHeader>
        <CardContent>
          <ul className="flex flex-col gap-2">
            <LaneRow
              testId="profile-lane-cron"
              label={t('profile.laneCron')}
              present={lanes.cron}
              detail={t('profile.laneCronDetail')}
            />
            <LaneRow
              testId="profile-lane-wallclock"
              label={t('profile.laneWallClock')}
              present={lanes.wallClock}
              detail={t('profile.laneWallClockDetail')}
            />
            <LaneRow
              testId="profile-lane-session"
              label={t('profile.laneSession')}
              present={lanes.session}
              detail={t('profile.laneSessionDetail')}
            />
            <LaneRow
              testId="profile-lane-direct"
              label={t('profile.laneDirect')}
              present={lanes.direct}
              detail={t('profile.laneDirectDetail')}
            />
          </ul>
        </CardContent>
      </Card>

      <Card className="shadow-card" data-testid="profile-states">
        <CardHeader>
          <CardTitle>{t('profile.statesTitle')}</CardTitle>
          <CardDescription>{t('profile.statesDescription')}</CardDescription>
        </CardHeader>
        <CardContent>
          {states.length === 0 ? (
            <p className="text-copy-13 text-gray-900">{t('profile.noStates')}</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {states.map((state) => (
                <li
                  key={state.id}
                  className="flex flex-col gap-1 rounded-card border border-gray-alpha-400 p-3"
                  data-testid={`profile-state-${state.id}`}
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-copy-13-mono text-gray-1000">{state.id}</span>
                    {state.terminal && <Badge variant="neutral">{t('profile.terminal')}</Badge>}
                  </div>
                  {state.description && (
                    <p className="text-copy-12 text-gray-700">{state.description}</p>
                  )}
                  {state.enter && state.enter.length > 0 && (
                    <div className="flex flex-wrap items-center gap-1">
                      <span className="text-label-12 text-gray-700">{t('profile.stateEnter')}</span>
                      {state.enter.map((action) => (
                        <Badge key={`${action.kind}:${action.name}`} variant="preset">
                          {action.kind}: {action.name}
                        </Badge>
                      ))}
                    </div>
                  )}
                  {state.exitWhen && (
                    <div className="flex flex-wrap items-center gap-1">
                      <span className="text-label-12 text-gray-700">
                        {t('profile.stateExitWhen')}
                      </span>
                      <span className="text-copy-13 text-gray-900" data-testid={`profile-exit-${state.id}`}>
                        {exitWhenSummary(state.exitWhen)}
                      </span>
                    </div>
                  )}
                  {state.next && (
                    <div className="flex flex-wrap items-center gap-1">
                      <span className="text-label-12 text-gray-700">{t('profile.stateNext')}</span>
                      <span className="text-copy-13 text-gray-900" data-testid={`profile-next-${state.id}`}>
                        {nextSummary(state.next)}
                      </span>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <Card className="shadow-card" data-testid="profile-roles">
        <CardHeader>
          <CardTitle>{t('profile.rolesTitle')}</CardTitle>
          <CardDescription>{t('profile.rolesDescription')}</CardDescription>
        </CardHeader>
        <CardContent>
          {!roles || roles.length === 0 ? (
            <p className="text-copy-13 text-gray-900">{t('profile.noRoles')}</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {roles.map((role) => (
                <li
                  key={role.id}
                  className="flex flex-col gap-1 rounded-card border border-gray-alpha-400 p-3"
                  data-testid={`profile-role-${role.id}`}
                >
                  <span className="text-copy-13-mono text-gray-1000">{role.id}</span>
                  <p className="text-copy-12 text-gray-700">{role.description}</p>
                  <div className="flex flex-wrap items-center gap-1">
                    <span className="text-label-12 text-gray-700">{t('profile.roleSystemPrompt')}</span>
                    <span className="text-copy-13-mono text-gray-900">{role.systemPromptFile}</span>
                  </div>
                  <div className="flex flex-wrap items-center gap-1">
                    <span className="text-label-12 text-gray-700">{t('profile.roleSkills')}</span>
                    {role.recommendedSkills && role.recommendedSkills.length > 0 ? (
                      role.recommendedSkills.map((skill) => (
                        <Badge key={skill} variant="preset" data-testid={`profile-skill-${role.id}-${skill}`}>
                          {skill}
                        </Badge>
                      ))
                    ) : (
                      <span className="text-copy-12 text-gray-700">{t('profile.noSkills')}</span>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <Card className="shadow-card" data-testid="profile-capabilities">
        <CardHeader>
          <CardTitle>{t('profile.capabilitiesTitle')}</CardTitle>
          <CardDescription>{t('profile.capabilitiesDescription')}</CardDescription>
        </CardHeader>
        <CardContent>
          {!requiredCapabilities || requiredCapabilities.length === 0 ? (
            <p className="text-copy-13 text-gray-900">{t('profile.noCapabilities')}</p>
          ) : (
            <ul className="flex flex-col gap-1">
              {requiredCapabilities.map((name) => (
                <li key={name}>
                  {/* Deep-link to the capability-schema browser (PL-13); the
                      browser seeds its filter from `?filter=` so the linked
                      schema is visible on arrival. */}
                  <Link
                    to={`/capabilities?filter=${encodeURIComponent(name)}`}
                    className="text-copy-13 text-blue-800 underline-offset-2 hover:underline"
                    data-testid={`profile-capability-${name}`}
                  >
                    {name}
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <Card className="shadow-card" data-testid="profile-signals">
        <CardHeader>
          <div className="flex flex-wrap items-center gap-2">
            <CardTitle>{t('profile.signalsTitle')}</CardTitle>
            {signals && signals.length > 0 && (
              <Badge variant="warning" data-testid="profile-signals-not-delivered">
                {t('profile.signalsNotDelivered')}
              </Badge>
            )}
          </div>
          <CardDescription>{t('profile.signalsDescription')}</CardDescription>
        </CardHeader>
        <CardContent>
          {!signals || signals.length === 0 ? (
            <p className="text-copy-13 text-gray-900">{t('profile.noSignals')}</p>
          ) : (
            <ul className="flex flex-col gap-1">
              {signals.map((signal, index) => (
                <li
                  key={`${signal.name}-${signal.action}-${index}`}
                  className="flex flex-wrap items-center gap-1"
                  data-testid={`profile-signal-${signal.name}`}
                >
                  <span className="text-copy-13-mono text-gray-1000">{signal.name}</span>
                  <span className="text-copy-13 text-gray-700">
                    · {t('profile.signalAction')}: {signal.action}
                    {signal.target ? ` · ${t('profile.signalTarget')}: ${signal.target}` : ''}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

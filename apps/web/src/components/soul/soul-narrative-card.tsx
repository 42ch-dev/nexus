import { Button } from '@/components/ui/button';
import { EmptyState, Spinner } from '@/components/ui/states';
import { formatDate } from '@/lib/format';
import type { SoulNarrativeResponse } from '@42ch/nexus-contracts';
import { useTranslation } from 'react-i18next';

/**
 * Creator-SOUL Narrative card (V1.81 SP-1 — web-ui.md §26.1).
 *
 * The headline surface of V1.81: an on-demand, cached LLM synthesis of "who you
 * are becoming" as a creative identity, drawn from accumulated fragment themes,
 * temporal shifts, and preoccupations. The narrative is world-agnostic
 * (Creator-level whole); the world selector does not re-scope it.
 *
 * Five UX states, each testable (plan §2.1 / web-ui.md §26.1 SP-1):
 *  - `insufficient-data` — the whole SOUL is below the server's quality
 *    threshold (current counts < min_*). Encouraging empty state; no CTA.
 *  - `ungenerated`       — narrative never generated, but enough fragments
 *    exist. CTA card: "Reflect on my SOUL".
 *  - `generating`        — reflect mutation in flight. Pulse skeleton; the
 *    action button is disabled with a spinner + "Reflecting…".
 *  - `current`           — cached narrative returned, not stale. Prose block
 *    with the `generated_at` timestamp + a subtle "Re-reflect" secondary action.
 *  - `stale`             — cached narrative exists but new fragments arrived.
 *    The cached prose stays visible under a stale banner with a "Re-reflect" CTA.
 *
 * The prose body consumes the DESIGN.md `soul-narrative-prose` token
 * (`--color-soul-narrative-prose`, copy-16 typography) so the reflective voice
 * reads at a calmer measure than UI copy. The card is stateless beyond its
 * props — the SOUL section wires the read query + reflect mutation.
 */
export function SoulNarrativeCard({
  narrative,
  isLoading,
  isReflecting,
  onReflect,
  scope = 'creator',
}: {
  narrative: SoulNarrativeResponse | undefined;
  isLoading: boolean;
  isReflecting: boolean;
  onReflect: () => void;
  /** V1.82: `world` scopes copy to the selected world's per-World narrative. */
  scope?: 'creator' | 'world';
}) {
  const { t } = useTranslation('memory');

  // `generating` (client-side, mutation in flight) takes precedence over the
  // server-reported state so the CTA shows active progress regardless of the
  // pre-reflect cache shape.
  if (isReflecting) {
    return <GeneratingState t={t} />;
  }

  // Initial read in flight and no cached data yet: a neutral loading skeleton.
  // (When a cache already exists, `narrative` is defined and we render the
  // server state below instead of flashing a skeleton on every poll refetch.)
  if (isLoading && !narrative) {
    return (
      <div data-testid="soul-narrative-loading" className="flex items-center gap-2 py-6 text-copy-14 text-gray-700">
        <Spinner />
        <span>{t('narrative.loading')}</span>
      </div>
    );
  }

  if (!narrative) {
    return null;
  }

  if (narrative.state === 'insufficient_data') {
    return <InsufficientDataState narrative={narrative} scope={scope} t={t} />;
  }

  if (narrative.state === 'ungenerated') {
    return <UngeneratedState onReflect={onReflect} t={t} />;
  }

  // `current` or `stale` both have a cached narrative to show. `stale` overlays
  // a growth banner above the cached prose with a primary re-reflect CTA.
  const stale = narrative.state === 'stale' || narrative.stale;
  if (narrative.narrative) {
    return (
      <CachedNarrativeState
        narrative={narrative}
        stale={stale}
        onReflect={onReflect}
        t={t}
      />
    );
  }

  // Defensive: a `current`/`stale` state without narrative text should not
  // happen (the server returns text for those states), but degrade to the
  // ungenerated CTA rather than a blank card.
  return <UngeneratedState onReflect={onReflect} t={t} />;
}

/** Distance from the quality threshold, clamped at 0 (for "X more to go"). */
function remainingToFragmentsThreshold(narrative: SoulNarrativeResponse): number {
  return Math.max(0, narrative.min_fragment_count - narrative.current_fragment_count);
}

function GeneratingState({ t }: { t: TFunction }) {
  return (
    <div data-testid="soul-narrative-generating" className="flex flex-col gap-3">
      <div className="flex items-center gap-2 text-copy-14 text-gray-700">
        <Spinner />
        <span>{t('narrative.reflecting')}</span>
      </div>
      <div
        aria-hidden
        className="flex flex-col gap-2"
      >
        <span className="h-4 w-3/4 animate-pulse rounded-control bg-background-300" />
        <span className="h-4 w-full animate-pulse rounded-control bg-background-300" />
        <span className="h-4 w-5/6 animate-pulse rounded-control bg-background-300" />
      </div>
      <Button variant="primary" disabled data-testid="soul-narrative-reflect">
        <Spinner className="text-current" />
        {t('narrative.reflecting')}
      </Button>
    </div>
  );
}

function UngeneratedState({ onReflect, t }: { onReflect: () => void; t: TFunction }) {
  return (
    <div data-testid="soul-narrative-ungenerated" className="flex flex-col items-start gap-3">
      <div>
        <p className="text-heading-16 font-heading text-gray-1000">{t('narrative.ungeneratedTitle')}</p>
        <p className="mt-1 text-copy-14 text-gray-700">{t('narrative.ungeneratedDescription')}</p>
      </div>
      <Button variant="primary" onClick={onReflect} data-testid="soul-narrative-reflect">
        {t('narrative.reflectCta')}
      </Button>
    </div>
  );
}

function InsufficientDataState({
  narrative,
  scope,
  t,
}: {
  narrative: SoulNarrativeResponse;
  scope: 'creator' | 'world';
  t: TFunction;
}) {
  const remaining = remainingToFragmentsThreshold(narrative);
  const isWorld = scope === 'world';
  return (
    <div data-testid="soul-narrative-insufficient">
      <EmptyState
        title={t(isWorld ? 'narrative.insufficientTitle_world' : 'narrative.insufficientTitle_creator')}
        description={t(isWorld ? 'narrative.insufficientDescription_world' : 'narrative.insufficientDescription_creator')}
      />
      <p className="mt-3 text-copy-13 text-gray-700">
        {t('narrative.capturedCount', { count: narrative.current_fragment_count })}
        {remaining > 0 ? ` — ${t('narrative.moreToGo', { count: remaining })}` : ''}.
      </p>
    </div>
  );
}

function CachedNarrativeState({
  narrative,
  stale,
  onReflect,
  t,
}: {
  narrative: SoulNarrativeResponse;
  stale: boolean;
  onReflect: () => void;
  t: TFunction;
}) {
  return (
    <div
      data-testid="soul-narrative-current"
      className="flex flex-col gap-3"
      data-stale={stale ? 'true' : undefined}
    >
      {stale && (
        <div
          className="flex flex-wrap items-center justify-between gap-3 rounded-control border border-amber-700/30 bg-amber-700/10 px-3 py-2"
          data-testid="soul-narrative-stale-banner"
        >
          <p className="text-copy-14 text-gray-1000">{t('narrative.staleBanner')}</p>
          <Button variant="primary" size="small" onClick={onReflect}>
            {t('narrative.reReflect')}
          </Button>
        </div>
      )}
      <p
        className="whitespace-pre-wrap text-copy-16"
        style={{ color: 'var(--color-soul-narrative-prose)', lineHeight: '1.6' }}
        data-testid="soul-narrative-prose"
      >
        {narrative.narrative}
      </p>
      <div className="flex items-center justify-between">
        <p className="text-copy-13 text-gray-700">
          {t('narrative.reflectedOn', { date: formatDate(narrative.generated_at) })}
        </p>
        {!stale && (
          <Button variant="tertiary" size="small" onClick={onReflect}>
            {t('narrative.reReflect')}
          </Button>
        )}
      </div>
    </div>
  );
}

type TFunction = (key: string, options?: Record<string, unknown>) => string;

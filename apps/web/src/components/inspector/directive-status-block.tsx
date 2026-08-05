/**
 * Moment Directive status block — P1 T2 (DF-76). Read-only.
 *
 * Renders `moment_directive` **status/metadata only** — the directive body is
 * never on the wire and never rendered here (AC-I3; the DTO has no body field
 * by construction). `status: "none"` renders "No active directive".
 *
 * Batch B extension point: the directive set/clear form mounts through the
 * `actions` slot (right-aligned in the section header). Nothing passes it
 * this batch — the panel stays read-only (AC-I4).
 */
import { Badge } from '@/components/ui/badge';
import type { MomentInspectResponse } from '@42ch/nexus-contracts';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

export interface DirectiveStatusBlockProps {
  directive: MomentInspectResponse['moment_directive'];
  /** Batch B (T4) extension point — the directive set/clear form mounts here. */
  actions?: ReactNode;
}

export function DirectiveStatusBlock({ directive, actions }: DirectiveStatusBlockProps) {
  const { t } = useTranslation('inspector');
  const active = directive.status !== 'none';

  return (
    <section aria-labelledby="inspector-directive-title" data-testid="directive-status-block">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 id="inspector-directive-title" className="text-heading-16 font-heading text-gray-1000">
          {t('directive.title')}
        </h3>
        {actions}
      </div>
      <p className="text-copy-13 text-gray-700">{t('directive.description')}</p>
      {!active ? (
        <p className="mt-2 text-copy-13 text-gray-700" data-testid="directive-none">
          {t('directive.noActive')}
        </p>
      ) : (
        <dl className="mt-2 flex flex-col gap-1.5 text-copy-13" data-testid="directive-status">
          <div className="flex items-center justify-between gap-4">
            <dt className="text-gray-900">{t('directive.statusLabel')}</dt>
            <dd>
              <Badge variant="running" tone="soft" data-testid="directive-status-active">
                {t(`directive.status.${directive.status === 'active' ? 'active' : 'none'}`)}
              </Badge>
            </dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-gray-900">{t('directive.scopeLabel')}</dt>
            <dd className="text-gray-1000" data-testid="directive-scope">
              {directive.scope ?? '—'}
            </dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-gray-900">{t('directive.scopeIdLabel')}</dt>
            <dd className="text-copy-13-mono text-gray-1000" data-testid="directive-scope-id">
              {directive.scope_id ?? '—'}
            </dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-gray-900">{t('directive.insertDepthLabel')}</dt>
            <dd className="text-gray-1000" data-testid="directive-depth">
              {directive.insert_depth ?? '—'}
            </dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-gray-900">{t('directive.ttlLabel')}</dt>
            <dd className="text-gray-1000" data-testid="directive-ttl">
              {directive.ttl_kind ?? '—'}
              {directive.ttl_remaining !== null && directive.ttl_kind
                ? ` · ${t('directive.ttlRemaining', { count: directive.ttl_remaining })}`
                : ''}
            </dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-gray-900">{t('directive.clearOnSceneChangeLabel')}</dt>
            <dd className="text-gray-1000" data-testid="directive-clear-on-scene-change">
              {directive.clear_on_scene_change ? t('directive.yes') : t('directive.no')}
            </dd>
          </div>
        </dl>
      )}
    </section>
  );
}

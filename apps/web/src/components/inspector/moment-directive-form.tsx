/**
 * Moment Directive set/clear form — P1 T4 (DF-76). Thin author surface.
 *
 * Mounts through `AssemblyInspectorPanel#directiveActions` into the
 * `DirectiveStatusBlock` header slot. A native `<details>` disclosure keeps
 * the header compact; the expanded card holds the CLI-mirroring controls
 * (scope Work/World, body, insert depth head/mid/tail, exactly one TTL kind,
 * `ttl_remaining >= 1`, clear-on-scene-change, replace). **Not** a
 * multi-slot Prompt-Manager-style editor (clean-room, PD-10).
 *
 * Writes go through `POST /v1/daemon/moment-directive` (`useMomentDirective`),
 * which invalidates the inspector query on success so the panel's
 * directive-status block refreshes (set → active; clear → none; AC-I5).
 * Client-side validation mirrors the CLI `handle_set`; a 409 conflict
 * (active directive without `replace`) prompts the author to enable replace —
 * never a silent overwrite.
 */
import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useMomentDirective } from '@/api/queries';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { NexusClientError } from '@/lib/nexus';
import type { MomentDirectiveRequest, MomentInspectResponse } from '@42ch/nexus-contracts';

export interface MomentDirectiveFormProps {
  /** Work scope id (`scope.kind === 'work'`). */
  workId: string;
  /** The Work's bound world id (`scope.kind === 'world'` override). The
   *  page gates on it — the form only mounts when the Work is bound. */
  worldId: string;
  /** The inspector packet's `moment_directive` section (status/metadata only;
   *  body never on the wire — AC-I3). Clear targets the **active** directive's
   *  scope from this section — never the form's default Work scope — so Clear
   *  always clears what the status block shows (QC W-1). Disabled when
   *  `status === 'none'`. */
  momentDirective: MomentInspectResponse['moment_directive'];
}

type ScopeKind = MomentDirectiveRequest['scope']['kind'];
type InsertDepth = NonNullable<MomentDirectiveRequest['insert_depth']>;
type TtlKind = NonNullable<MomentDirectiveRequest['ttl_kind']>;

export function MomentDirectiveForm({ workId, worldId, momentDirective }: MomentDirectiveFormProps) {
  const { t } = useTranslation('inspector');
  const directive = useMomentDirective();
  const uid = useId();

  const [scopeKind, setScopeKind] = useState<ScopeKind>('work');
  const [body, setBody] = useState('');
  const [insertDepth, setInsertDepth] = useState<InsertDepth>('tail');
  const [ttlKind, setTtlKind] = useState<TtlKind | null>(null);
  const [ttlRemaining, setTtlRemaining] = useState('5');
  const [clearOnSceneChange, setClearOnSceneChange] = useState(false);
  const [replace, setReplace] = useState(false);
  const [attempted, setAttempted] = useState(false);

  const scopeId = scopeKind === 'world' ? worldId : workId;

  // The active directive's scope from the inspector packet (null when none).
  // Clear targets this — never the form's selected scope — so clearing a
  // World-scoped active directive actually clears it (QC W-1).
  const activeDirectiveScope: { kind: 'work' | 'world'; id: string } | null =
    momentDirective.status !== 'none' &&
    (momentDirective.scope === 'work' || momentDirective.scope === 'world') &&
    momentDirective.scope_id !== null
      ? { kind: momentDirective.scope, id: momentDirective.scope_id }
      : null;

  const errors: string[] = [];
  if (body.trim().length === 0) errors.push(t('directive.form.validation.bodyRequired'));
  if (ttlKind === null) errors.push(t('directive.form.validation.ttlKindRequired'));
  const ttlValue = Number(ttlRemaining);
  // Safe integer caps at 2^53-1 — fits the daemon's i64 while rejecting
  // values that would overflow the JSON number on the wire (QC3 S-2).
  const ttlInvalid = !Number.isSafeInteger(ttlValue) || ttlValue < 1;
  if (ttlInvalid) {
    errors.push(t('directive.form.validation.ttlRemainingInvalid'));
  }

  // 409 = an active directive already exists for the scope without `replace`
  // (CLI `--replace` discipline — no silent overwrite).
  const conflict =
    directive.error instanceof NexusClientError && directive.error.status === 409;
  const showErrors = (attempted || directive.isError) && errors.length > 0;
  const pending = directive.isPending;

  // Back to pristine state after a successful set/clear — repeat sets after
  // clear stay clean and the disclosure never shows a "ghost" directive (QC3 S-3).
  const resetForm = () => {
    setScopeKind('work');
    setBody('');
    setInsertDepth('tail');
    setTtlKind(null);
    setTtlRemaining('5');
    setClearOnSceneChange(false);
    setReplace(false);
    setAttempted(false);
  };

  const submitSet = (replaceOverride?: boolean) => {
    setAttempted(true);
    if (errors.length > 0 || ttlKind === null) return;
    const useReplace = replaceOverride ?? replace;
    directive.mutate(
      {
        action: 'set',
        scope: { kind: scopeKind, id: scopeId },
        body: body.trim(),
        insert_depth: insertDepth,
        ttl_kind: ttlKind,
        ttl_remaining: ttlValue,
        clear_on_scene_change: clearOnSceneChange,
        ...(useReplace ? { replace: true } : {}),
      },
      { onSuccess: resetForm },
    );
  };

  const submitClear = () => {
    if (!activeDirectiveScope) return;
    directive.mutate({ action: 'clear', scope: activeDirectiveScope }, { onSuccess: resetForm });
  };

  const enableReplaceAndRetry = () => {
    setReplace(true);
    submitSet(true);
  };

  return (
    <div className="flex flex-wrap items-center gap-2" data-testid="directive-form">
      <details className="group" data-testid="directive-form-details">
        <summary className="inline-flex cursor-pointer list-none items-center gap-1.5 rounded-control px-2 py-1 text-button-12 text-gray-1000 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2 [&::-webkit-details-marker]:hidden">
          <span
            aria-hidden
            className="inline-block transition-transform group-open:rotate-90"
          >
            ▸
          </span>
          {t('directive.form.summary')}
        </summary>
        <div className="mt-3 w-80 rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-elevation-2 sm:w-96">
          <div className="flex flex-col gap-4">
            {/* Scope — Work default / World override */}
            <fieldset>
              <legend className="text-label-12 text-gray-900">
                {t('directive.form.scopeLegend')}
              </legend>
              <div className="mt-1.5 flex gap-4">
                <label className="flex items-center gap-1.5 text-copy-13 text-gray-1000">
                  <input
                    type="radio"
                    name={`${uid}-scope`}
                    checked={scopeKind === 'work'}
                    onChange={() => setScopeKind('work')}
                  />
                  {t('directive.form.scope.work')}
                </label>
                <label className="flex items-center gap-1.5 text-copy-13 text-gray-1000">
                  <input
                    type="radio"
                    name={`${uid}-scope`}
                    checked={scopeKind === 'world'}
                    onChange={() => setScopeKind('world')}
                  />
                  {t('directive.form.scope.world')}
                </label>
              </div>
            </fieldset>

            {/* Body — non-empty after trim */}
            <div className="flex flex-col gap-1">
              <Label htmlFor={`${uid}-body`}>{t('directive.form.bodyLabel')}</Label>
              <Textarea
                id={`${uid}-body`}
                rows={4}
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder={t('directive.form.bodyPlaceholder')}
                invalid={showErrors && body.trim().length === 0}
                aria-describedby={showErrors && body.trim().length === 0 ? `${uid}-body-error` : undefined}
              />
              {showErrors && body.trim().length === 0 ? (
                <p id={`${uid}-body-error`} className="text-copy-13 text-red-1000">
                  {t('directive.form.validation.bodyRequired')}
                </p>
              ) : null}
            </div>

            {/* Insert depth — head / mid / tail */}
            <div className="flex flex-col gap-1">
              <Label htmlFor={`${uid}-depth`}>{t('directive.form.insertDepthLabel')}</Label>
              <Select
                id={`${uid}-depth`}
                value={insertDepth}
                onChange={(e) => setInsertDepth(e.target.value as InsertDepth)}
                className="w-36"
              >
                <option value="head">{t('directive.form.depth.head')}</option>
                <option value="mid">{t('directive.form.depth.mid')}</option>
                <option value="tail">{t('directive.form.depth.tail')}</option>
              </Select>
            </div>

            {/* TTL — exactly one kind required */}
            <fieldset>
              <legend className="text-label-12 text-gray-900">
                {t('directive.form.ttlLegend')}
              </legend>
              <div className="mt-1.5 flex gap-4">
                <label className="flex items-center gap-1.5 text-copy-13 text-gray-1000">
                  <input
                    type="radio"
                    name={`${uid}-ttl-kind`}
                    checked={ttlKind === 'generations'}
                    onChange={() => setTtlKind('generations')}
                  />
                  {t('directive.form.ttl.generations')}
                </label>
                <label className="flex items-center gap-1.5 text-copy-13 text-gray-1000">
                  <input
                    type="radio"
                    name={`${uid}-ttl-kind`}
                    checked={ttlKind === 'chapters'}
                    onChange={() => setTtlKind('chapters')}
                  />
                  {t('directive.form.ttl.chapters')}
                </label>
              </div>
              {showErrors && ttlKind === null ? (
                <p id={`${uid}-ttl-error`} className="mt-1 text-copy-13 text-red-1000">
                  {t('directive.form.validation.ttlKindRequired')}
                </p>
              ) : null}
            </fieldset>

            {/* TTL count — integer >= 1 */}
            <div className="flex flex-col gap-1">
              <Label htmlFor={`${uid}-ttl-remaining`}>
                {t('directive.form.ttlRemainingLabel')}
              </Label>
              <Input
                id={`${uid}-ttl-remaining`}
                type="number"
                min={1}
                max={Number.MAX_SAFE_INTEGER}
                step={1}
                inputMode="numeric"
                value={ttlRemaining}
                onChange={(e) => setTtlRemaining(e.target.value)}
                invalid={showErrors && ttlInvalid}
                aria-describedby={
                  showErrors && ttlInvalid ? `${uid}-ttl-remaining-error` : undefined
                }
                className="w-28"
              />
              {showErrors && ttlInvalid ? (
                <p id={`${uid}-ttl-remaining-error`} className="text-copy-13 text-red-1000">
                  {t('directive.form.validation.ttlRemainingInvalid')}
                </p>
              ) : null}
            </div>

            {/* Flags */}
            <label className="flex items-center gap-2 text-copy-13 text-gray-1000">
              <input
                type="checkbox"
                checked={clearOnSceneChange}
                onChange={(e) => setClearOnSceneChange(e.target.checked)}
              />
              {t('directive.form.clearOnSceneChangeLabel')}
            </label>
            <label className="flex flex-col gap-0.5 text-copy-13 text-gray-1000">
              <span className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={replace}
                  onChange={(e) => setReplace(e.target.checked)}
                />
                {t('directive.form.replaceLabel')}
              </span>
              <span className="pl-6 text-copy-13 text-gray-700">
                {t('directive.form.replaceHint')}
              </span>
            </label>

            {/* Errors — validation list + mutation failure */}
            {showErrors ? (
              <ul
                className="flex flex-col gap-1 rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
                aria-live="polite"
                data-testid="directive-form-errors"
              >
                {errors.map((err) => (
                  <li key={err}>{err}</li>
                ))}
              </ul>
            ) : null}

            {conflict ? (
              <div
                role="alert"
                className="flex flex-col gap-2 rounded-card border border-amber-700/30 bg-amber-700/10 p-3 text-copy-13"
                data-testid="directive-form-conflict"
              >
                <p className="font-medium text-amber-1000">
                  {t('directive.form.conflictTitle')}
                </p>
                <p className="text-gray-900">{t('directive.form.conflictDescription')}</p>
                <Button
                  type="button"
                  variant="secondary"
                  size="small"
                  onClick={enableReplaceAndRetry}
                  disabled={pending || errors.length > 0}
                  className="self-start"
                  data-testid="directive-form-enable-replace"
                >
                  {t('directive.form.conflictEnableReplace')}
                </Button>
              </div>
            ) : directive.isError ? (
              <p
                role="alert"
                className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
                data-testid="directive-form-mutation-error"
              >
                {directive.error instanceof Error
                  ? directive.error.message
                  : t('directive.form.genericError')}
              </p>
            ) : null}

            <div className="flex justify-end">
              <Button
                type="button"
                variant="primary"
                size="small"
                onClick={() => submitSet()}
                disabled={pending}
                data-testid="directive-form-set"
              >
                {pending ? t('directive.form.setting') : t('directive.form.set')}
              </Button>
            </div>
          </div>
        </div>
      </details>

      <Button
        type="button"
        variant="secondary"
        size="small"
        onClick={submitClear}
        disabled={!activeDirectiveScope || pending}
        data-testid="directive-form-clear"
      >
        {pending ? t('directive.form.clearing') : t('directive.form.clear')}
      </Button>
    </div>
  );
}

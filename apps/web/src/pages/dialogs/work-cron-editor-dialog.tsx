import { useEffect, useState } from 'react';

import { useTranslation } from 'react-i18next';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { usePutWorkCron, useWorkCron } from '@/api/queries';
import { NexusClientError } from '@/lib/nexus';
import type { WorkCronResponse, WorkCronRoles } from '@/lib/nexus';

/**
 * Per-Work cron editor dialog — GET/PUT /v1/daemon/works/{work_id}/cron
 * (V1.171 P2 — PL-16/AR-29).
 *
 * Loads the effective config via `getWorkCron` and shows the "using defaults"
 * marker when `is_default` (AR-30 honesty: the payload is the spec defaults,
 * not a stored config). Saves via `putWorkCron` with the GET'd config as the
 * CAS pre-image (`expected_current_json`): the stored blob is the
 * `{ tz, roles }` shape only, so the client reconstructs it from the GET
 * response (dropping the `is_default` marker); when the config is unset the
 * pre-image is the empty string ("must currently be unset").
 *
 * The pre-image is frozen at the SAME snapshot the form was built from (first
 * load, or an explicit Reload). A background refetch may advance the live GET
 * data, but it must never advance the pre-image — otherwise a stale edit could
 * silently overwrite a concurrent change instead of 409-ing (Bugbot Medium).
 *
 * A 409 CAS conflict surfaces as a visible alert prompting a reload (re-GET +
 * re-apply). A 400 surfaces the daemon message with its stable code
 * (`E_CRON_INVALID_EXPR` / `E_CRON_INVALID_TZ`) inline.
 *
 * Honesty (AR-30): this is config-only — cron firing into the auto-chain is
 * not shipped. The UI shows the cron expression + defaults marker and never
 * promises a firing cadence or a computed next occurrence.
 */

/** The editable form shape — the `WorkSchedule` wire body (no `is_default`). */
interface WorkCronForm {
  tz: string;
  roles: WorkCronRoles;
}

const ROLE_KEYS = ['brainstorm', 'write', 'review'] as const;

function formFromResponse(resp: WorkCronResponse): WorkCronForm {
  return {
    tz: resp.tz,
    roles: {
      brainstorm: { ...resp.roles.brainstorm },
      write: { ...resp.roles.write },
      review: { ...resp.roles.review },
    },
  };
}

/**
 * Reconstruct the stored `schedule_json` blob from a GET response for use as
 * the CAS pre-image. The stored blob is the `WorkSchedule` shape only
 * (`tz` + `roles`); `is_default` is a GET-only marker and never part of the
 * stored JSON. When the config is unset (defaults), the pre-image is the
 * empty string — the daemon treats it as "must currently be unset".
 */
function workCronPreimage(resp: WorkCronResponse): string {
  if (resp.is_default) return '';
  return JSON.stringify({ tz: resp.tz, roles: resp.roles });
}

export function WorkCronEditorDialog({
  workId,
  workTitle,
  open,
  onOpenChange,
}: {
  workId: string;
  workTitle: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation('schedule');
  const cron = useWorkCron(workId);
  const put = usePutWorkCron();
  const [form, setForm] = useState<WorkCronForm | null>(null);
  // The CAS pre-image, frozen at the SAME snapshot the form was built from.
  // A background refetch may advance `cron.data`, but the pre-image must not
  // follow: it guards the stored blob the user's frozen edits are based on.
  // (Bugbot Medium: a refetched pre-image lets a CAS PUT silently overwrite a
  // concurrent change instead of 409-ing.)
  const [preimage, setPreimage] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Initialize the form + pre-image from the first successful load. The
  // explicit Reload action (below) re-baselines both from the latest data; a
  // background refetch never clobbers in-progress edits — and never advances
  // the pre-image either.
  useEffect(() => {
    if (cron.data && form === null) {
      setForm(formFromResponse(cron.data));
      setPreimage(workCronPreimage(cron.data));
    }
  }, [cron.data, form]);

  function setRole(
    role: (typeof ROLE_KEYS)[number],
    patch: Partial<{ cron: string; enabled: boolean }>,
  ) {
    setForm((prev) =>
      prev
        ? { ...prev, roles: { ...prev.roles, [role]: { ...prev.roles[role], ...patch } } }
        : prev,
    );
  }

  async function handleReload() {
    setConflict(false);
    setError(null);
    const res = await cron.refetch();
    if (res.data) {
      setForm(formFromResponse(res.data));
      setPreimage(workCronPreimage(res.data));
    }
  }

  async function handleSave() {
    if (!form || preimage === null) return;
    setError(null);
    setConflict(false);
    try {
      await put.mutateAsync({
        workId,
        request: {
          tz: form.tz.trim(),
          roles: form.roles,
          // The frozen snapshot pre-image — NOT the live `cron.data`. If a
          // background refetch advanced the stored blob, the CAS still
          // compares against the blob this form was built from, so a
          // concurrent write surfaces as a 409 instead of being overwritten.
          expected_current_json: preimage,
        },
      });
      onOpenChange(false);
    } catch (err) {
      if (err instanceof NexusClientError && err.status === 409) {
        // CAS mismatch — another writer changed the config between our GET
        // and PUT. Prompt the user to reload and re-apply.
        setConflict(true);
      } else if (err instanceof NexusClientError && err.status === 400) {
        // Daemon validation failure; the message carries the stable code
        // (E_CRON_INVALID_EXPR / E_CRON_INVALID_TZ).
        setError(err.message);
      } else {
        setError(t('workCron.genericError'));
      }
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('workCron.title')}
        description={t('workCron.description', { work: workTitle })}
      >
        {cron.isLoading && !form ? (
          <p className="text-copy-13 text-gray-700">{t('workCron.loading')}</p>
        ) : cron.isError ? (
          <div className="flex flex-col gap-3">
            <p className="text-copy-13 text-red-1000">{t('workCron.loadError')}</p>
            <Button
              type="button"
              variant="secondary"
              size="small"
              onClick={() => {
                // Retry resets the form on success so the error branch is
                // reachable on the first failure too (F-008: the initial GET
                // error previously dead-ended in the loading branch).
                setConflict(false);
                setError(null);
                void cron.refetch().then((res) => {
                  if (res.data) {
                    setForm(formFromResponse(res.data));
                    setPreimage(workCronPreimage(res.data));
                  }
                });
              }}
              className="self-start"
            >
              {t('workCron.retry')}
            </Button>
          </div>
        ) : form ? (
          <div className="flex flex-col gap-4">
            {cron.data?.is_default && (
              <p
                className="rounded-card border border-amber-700/30 bg-amber-700/10 p-3 text-copy-13 text-amber-1000"
                data-testid="work-cron-defaults-marker"
              >
                {t('workCron.usingDefaults')}
              </p>
            )}

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="work-cron-tz">{t('workCron.tzLabel')}</Label>
              <Input
                id="work-cron-tz"
                value={form.tz}
                onChange={(e) => setForm((prev) => (prev ? { ...prev, tz: e.target.value } : prev))}
                placeholder="UTC"
              />
            </div>

            <div className="flex flex-col gap-3">
              {ROLE_KEYS.map((role) => (
                <div
                  key={role}
                  className="flex flex-col gap-1.5 rounded-card border border-gray-alpha-400 p-3"
                >
                  <div className="flex items-center justify-between gap-2">
                    <Label htmlFor={`work-cron-${role}`} className="capitalize">
                      {t(`workCron.roles.${role}`)}
                    </Label>
                    <label className="flex items-center gap-2 text-copy-13 text-gray-1000">
                      <input
                        type="checkbox"
                        checked={form.roles[role].enabled}
                        onChange={(e) => setRole(role, { enabled: e.target.checked })}
                      />
                      {t('workCron.enabledLabel')}
                    </label>
                  </div>
                  <Input
                    id={`work-cron-${role}`}
                    value={form.roles[role].cron}
                    onChange={(e) => setRole(role, { cron: e.target.value })}
                    placeholder="0 3,9,15,21 * * *"
                    className="font-mono"
                  />
                </div>
              ))}
            </div>

            <p className="text-copy-13 text-gray-700">{t('workCron.honestyHint')}</p>

            {conflict && (
              <div
                role="alert"
                className="flex flex-col gap-2 rounded-card border border-amber-700/30 bg-amber-700/10 p-3 text-copy-13"
                data-testid="work-cron-conflict"
              >
                <p className="font-medium text-amber-1000">{t('workCron.conflictTitle')}</p>
                <p className="text-gray-900">{t('workCron.conflictDescription')}</p>
                <Button
                  type="button"
                  variant="secondary"
                  size="small"
                  onClick={handleReload}
                  disabled={put.isPending}
                  className="self-start"
                >
                  {t('workCron.reload')}
                </Button>
              </div>
            )}

            {error && (
              <p
                role="alert"
                className="rounded-card border border-red-700/30 bg-red-700/10 p-3 text-copy-13 text-red-1000"
                data-testid="work-cron-error"
              >
                {error}
              </p>
            )}

            <div className="flex justify-end gap-2 pt-2">
              <Button
                type="button"
                variant="tertiary"
                size="small"
                onClick={() => onOpenChange(false)}
              >
                {t('common:action.cancel')}
              </Button>
              <Button
                type="button"
                variant="primary"
                size="small"
                onClick={handleSave}
                disabled={put.isPending}
              >
                {put.isPending ? t('workCron.saving') : t('workCron.save')}
              </Button>
            </div>
          </div>
        ) : (
          // Unreachable in practice: form initializes from the first
          // successful GET (F-008 keeps the error branch reachable), so this
          // only renders while data is settling without a pending fetch.
          <p className="text-copy-13 text-gray-700">{t('workCron.loading')}</p>
        )}
      </DialogContent>
    </Dialog>
  );
}

import { useEffect, useMemo, useState, type FormEvent } from 'react';

import { useTranslation } from 'react-i18next';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { ErrorState } from '@/components/ui/states';
import { Input, Label, Select, Textarea } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useToast } from '@/lib/use-toast';
import { WORK_PROFILES, isWorkProfile, type WorkProfile } from '@/lib/work-profiles';
import { useCreateWork, useNarrativeWorlds } from '@/api/queries';

/**
 * Create Work dialog — POST /v1/daemon/works.
 *
 * The contract `CreateWorkRequest` requires title + long_term_goal +
 * initial_idea + world_id (F-01 W1: the daemon rejects a missing `world_id`
 * with 400 `world_id_required`, and the schema now marks it required), and
 * accepts an optional `work_profile` (V1.67 G1). The World selector is fed by
 * the narrative-worlds read model; with exactly one World it preselects it,
 * and create stays blocked until a World is chosen.
 * The selector defaults to `novel` for display, but `work_profile` is only
 * sent when the author explicitly chooses a profile — an untouched form
 * omits the field (daemon stores NULL), preserving the V1.66 wire shape
 * (qc1 W1). Work-profile values + labels live in the SSOT module
 * `@/lib/work-profiles` (R-V167P1-QC1-S2); the selector state is narrowed
 * to the `WorkProfile` literal union (R-V167P1-QC1-S1). DESIGN.md §Voice &
 * Content: Verb + Noun action ("Create Work"); loading state uses present
 * participle.
 */
export function CreateWorkDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated?: (workId: string) => void;
}) {
  const create = useCreateWork();
  const { toast } = useToast();
  const { t } = useTranslation('shell');
  // F-01 W1: Work creation is World-scoped — the daemon 400s without a
  // `world_id`. The selector is fed by the narrative-worlds read model.
  const narrativeWorlds = useNarrativeWorlds();
  const worldOptions = useMemo(
    () =>
      (narrativeWorlds.data ?? []).map((world) => ({
        value: world.world_id,
        label: world.title,
      })),
    [narrativeWorlds.data],
  );
  // PR #372 (P2): when the World list request fails before any data is
  // cached, distinguish the read error from the genuine empty state —
  // show a retry instead of "No Worlds available", and keep create blocked
  // until worlds load (worldId stays '' so `valid` is false either way).
  const worldReadError = narrativeWorlds.isError && worldOptions.length === 0;
  const [worldId, setWorldId] = useState('');
  const [title, setTitle] = useState('');
  const [longTermGoal, setLongTermGoal] = useState('');
  const [initialIdea, setInitialIdea] = useState('');
  const [workProfile, setWorkProfile] = useState<WorkProfile>(WORK_PROFILES[0].value);
  // W1: track whether the author explicitly touched the selector. Untouched
  // forms omit `work_profile` so the daemon stores NULL (V1.66 semantics).
  const [workProfileTouched, setWorkProfileTouched] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset the form whenever the dialog opens.
  useEffect(() => {
    if (open) {
      setTitle('');
      setLongTermGoal('');
      setInitialIdea('');
      setWorldId('');
      setWorkProfile(WORK_PROFILES[0].value);
      setWorkProfileTouched(false);
      setError(null);
    }
  }, [open]);

  // Single-world preselect (F-01 W1): exactly one World makes the choice
  // unambiguous — select it automatically instead of forcing the picker.
  useEffect(() => {
    if (worldId === '' && worldOptions.length === 1) {
      setWorldId(worldOptions[0].value);
    }
  }, [worldOptions, worldId]);

  const valid =
    title.trim().length > 0 &&
    longTermGoal.trim().length > 0 &&
    initialIdea.trim().length > 0 &&
    worldId !== '';

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!valid) {
      setError(t('workCreate.validationError'));
      return;
    }
    try {
      const res = await create.mutateAsync({
        title: title.trim(),
        long_term_goal: longTermGoal.trim(),
        initial_idea: initialIdea.trim(),
        world_id: worldId,
        ...(workProfileTouched ? { work_profile: workProfile } : {}),
      });
      toast({ variant: 'success', title: t('workCreate.toastCreated'), description: res.work_id });
      onOpenChange(false);
      onCreated?.(res.work_id);
    } catch {
      // Error toast already fired by the mutation's onError callback.
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('workCreate.title')}
        description={t('workCreate.description')}
      >
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-title">{t('workCreate.titleLabel')}</Label>
            <Input
              id="work-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder={t('workCreate.titlePlaceholder')}
              invalid={Boolean(error) && title.trim().length === 0}
              autoFocus
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-goal">{t('workCreate.goalLabel')}</Label>
            <Textarea
              id="work-goal"
              value={longTermGoal}
              onChange={(e) => setLongTermGoal(e.target.value)}
              placeholder={t('workCreate.goalPlaceholder')}
              invalid={Boolean(error) && longTermGoal.trim().length === 0}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-idea">{t('workCreate.ideaLabel')}</Label>
            <Textarea
              id="work-idea"
              value={initialIdea}
              onChange={(e) => setInitialIdea(e.target.value)}
              placeholder={t('workCreate.ideaPlaceholder')}
              invalid={Boolean(error) && initialIdea.trim().length === 0}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-world">{t('workCreate.worldLabel')}</Label>
            {worldReadError ? (
              <ErrorState
                title={t('workCreate.worldErrorTitle')}
                description={t('workCreate.worldErrorDescription')}
                onRetry={() => narrativeWorlds.refetch()}
              />
            ) : (
              <Select
                id="work-world"
                value={worldId}
                onChange={(e) => setWorldId(e.target.value)}
                disabled={narrativeWorlds.isLoading || create.isPending}
              >
                {worldOptions.length === 0 ? (
                  <option value="">{t('workCreate.worldEmpty')}</option>
                ) : (
                  <>
                    <option value="">{t('workCreate.worldPlaceholder')}</option>
                    {worldOptions.map((world) => (
                      <option key={world.value} value={world.value}>
                        {world.label}
                      </option>
                    ))}
                  </>
                )}
              </Select>
            )}
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="work-profile">{t('workCreate.profileLabel')}</Label>
            <Select
              id="work-profile"
              value={workProfile}
              onChange={(e) => {
                // Reject invalid values at the type boundary (R-V167P1-QC1-S1):
                // the Select only emits known profiles, but the guard keeps the
                // typed state from ever accepting an out-of-set string.
                if (isWorkProfile(e.target.value)) {
                  setWorkProfile(e.target.value);
                  setWorkProfileTouched(true);
                }
              }}
            >
              {WORK_PROFILES.map((profile) => (
                <option key={profile.value} value={profile.value}>
                  {profile.label}
                </option>
              ))}
            </Select>
          </div>
          {error && <p className="text-copy-13 text-red-700">{error}</p>}
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => onOpenChange(false)}>
              {t('common:action.cancel')}
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={!valid || create.isPending}>
              {create.isPending ? t('workCreate.creating') : t('workCreate.create')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

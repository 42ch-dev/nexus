/**
 * Harness agent-profile bar (plan v1220 T2, I-000064).
 *
 * Transport-coherent with the shared read seam: on desktop both the read and
 * the save go through the IPC (`get_agent_profile` / `set_agent_profile`); in
 * the browser both go through the service surface. A save never writes a
 * different store than the one being displayed.
 *
 * One explicit success state (`已保存`) and one inline failure state; the prior
 * saved profile stays displayed when a save fails. A selected agent that
 * reports no launch command keeps the stored custom command for the same
 * profile (client-side merge — the desktop IPC store does not merge).
 */
import { useEffect, useMemo, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { useSavedAgentProfile, useScanAgents } from '@/api/queries';
import { useDesktopCapabilities, useNexusClient } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { queryKeys } from '@/lib/nexus/query-keys';
import { Button } from '@/components/ui/button';
import { Select } from '@/components/ui/select';

export function ProfileBar() {
  const { t } = useTranslation('strategies');
  const { t: tCommon } = useTranslation('common');
  const desktop = useDesktopCapabilities();
  const client = useNexusClient();
  const qc = useQueryClient();
  const saved = useSavedAgentProfile();
  const scan = useScanAgents({ filter: 'installed' });

  const options = useMemo(
    () =>
      (scan.data?.agents ?? [])
        .filter((agent) => agent.installed)
        .map((agent) => ({
          name: agent.name,
          launchCommand: agent.launch_command ?? undefined,
        })),
    [scan.data],
  );

  const [selected, setSelected] = useState('');
  const [pending, setPending] = useState(false);
  const [savedState, setSavedState] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // Preselect the saved profile; fall back to the first installed agent.
  useEffect(() => {
    if (selected.length > 0) return;
    const initial = saved.profile?.name ?? options[0]?.name;
    if (initial) setSelected(initial);
  }, [saved.profile, options, selected]);

  async function handleSave() {
    if (selected.length === 0) return;
    const option = options.find((candidate) => candidate.name === selected);
    // Preserve the stored custom command when the selected agent reports none
    // and it is the same stored profile.
    const command =
      option?.launchCommand ??
      (saved.profile?.name === selected ? saved.profile.launchCommand : undefined);
    setPending(true);
    setSaveError(null);
    setSavedState(false);
    try {
      if (desktop) {
        await desktop.setAgentProfile(selected, command);
      } else {
        await client.saveAgentProfile(
          command !== undefined
            ? { name: selected, launch_command: command }
            : { name: selected },
        );
      }
      await qc.invalidateQueries({ queryKey: queryKeys.agentProfile.detail() });
      setSavedState(true);
    } catch (error) {
      setSaveError(errorMessage(error) || tCommon('error.requestFailed'));
    } finally {
      setPending(false);
    }
  }

  return (
    <div
      data-testid="harness-profile-bar"
      className="flex flex-wrap items-center gap-3 rounded-card border border-gray-alpha-400 bg-background-100 px-4 py-3"
    >
      <span className="text-label-12 text-gray-900">{t('harness.profile.label')}</span>

      {saved.isLoading ? (
        <span className="text-copy-13 text-gray-700" data-testid="harness-profile-loading">
          {t('harness.profile.loading')}
        </span>
      ) : saved.error ? (
        <span
          className="flex items-center gap-2 text-copy-13 text-red-900"
          role="alert"
          data-testid="harness-profile-error"
        >
          {t('harness.profile.error', {
            reason: errorMessage(saved.error) || tCommon('error.requestFailed'),
          })}
          <Button type="button" variant="tertiary" size="small" onClick={saved.refetch}>
            {t('harness.profile.retry')}
          </Button>
        </span>
      ) : (
        <span className="text-copy-13 text-gray-1000" data-testid="harness-profile-current">
          {saved.profile
            ? t('harness.profile.current', { name: saved.profile.name })
            : t('harness.profile.none')}
        </span>
      )}

      <Select
        aria-label={t('harness.profile.label')}
        className="w-56"
        value={selected}
        onChange={(event) => {
          setSelected(event.target.value);
          setSavedState(false);
          setSaveError(null);
        }}
      >
        {selected.length === 0 && (
          <option value="" disabled>
            {t('harness.profile.selectPlaceholder')}
          </option>
        )}
        {options.map((option) => (
          <option key={option.name} value={option.name}>
            {option.name}
          </option>
        ))}
        {selected.length > 0 && !options.some((option) => option.name === selected) && (
          <option value={selected}>{selected}</option>
        )}
      </Select>

      <Button
        type="button"
        variant="primary"
        size="small"
        onClick={handleSave}
        disabled={selected.length === 0 || pending}
      >
        {pending ? t('harness.profile.saving') : t('harness.profile.save')}
      </Button>

      {savedState && (
        <span
          className="text-copy-13 text-green-700"
          role="status"
          data-testid="harness-profile-saved"
        >
          {t('harness.profile.saved')}
        </span>
      )}
      {saveError && (
        <span
          className="text-copy-13 text-red-900"
          role="alert"
          data-testid="harness-profile-save-error"
        >
          {t('harness.profile.saveFailed', { reason: saveError })}
        </span>
      )}
    </div>
  );
}

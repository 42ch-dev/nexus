/**
 * Harness agent-profile bar (plan v1220 T2, I-000064).
 *
 * Transport-coherent with the shared read seam: on desktop both the read and
 * the save go through the IPC (`get_agent_profile` / `set_agent_profile`); in
 * the browser both go through the service surface. A save never writes a
 * different store than the one being displayed.
 *
 * Command origin: the launch-command field shows either the stored custom
 * command (`stored`), the scan's default for the selected agent (`scan`), or
 * what the user typed (`edited`). Only an `edited` command is sent as an
 * override — a scan default is the agent's natural launch command, not user
 * intent, so the browser save omits it and the service preserves the stored
 * command. The desktop IPC store replaces the command wholesale, so a `stored`
 * command is re-sent there to preserve it; a switch to another agent never
 * carries the previous custom command over.
 *
 * Scan-first initialization: the saved profile always wins over the
 * scan-recommended default, whenever it arrives, until the user touches the
 * controls. Save stays disabled while the profile read is pending so no scan
 * default is committed as if it were the persisted choice.
 */
import { useEffect, useMemo, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { useSavedAgentProfile, useScanAgents } from '@/api/queries';
import { useDesktopCapabilities, useNexusClient } from '@/lib/client-context';
import { errorMessage } from '@/lib/error-message';
import { queryKeys } from '@/lib/nexus/query-keys';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select } from '@/components/ui/select';

/** Where the command currently shown in the field came from. */
type CommandOrigin = 'stored' | 'scan' | 'edited';

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
  const [command, setCommand] = useState('');
  const [commandOrigin, setCommandOrigin] = useState<CommandOrigin>('scan');
  const [touched, setTouched] = useState(false);
  const [pending, setPending] = useState(false);
  const [savedState, setSavedState] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const profileSettled = saved.isFetched;

  // Converge on the persisted profile whenever it arrives (it wins over the
  // scan-recommended default), as long as the user has not touched the
  // controls. Before the read settles the scan default may show; Save is
  // gated meanwhile, so it is never committed as the persisted choice.
  useEffect(() => {
    if (touched) return;
    const stored = saved.profile;
    const nextName = stored?.name ?? options[0]?.name;
    if (!nextName) return;
    const option = options.find((candidate) => candidate.name === nextName);
    setSelected(nextName);
    if (stored?.name === nextName && stored.launchCommand) {
      setCommand(stored.launchCommand);
      setCommandOrigin('stored');
      return;
    }
    setCommand(option?.launchCommand ?? '');
    setCommandOrigin('scan');
  }, [touched, saved.profile, options]);

  function handleAgentChange(name: string) {
    setTouched(true);
    setSelected(name);
    // A switch never inherits the previous custom command: show the new
    // agent's scan default (or nothing) as a non-override origin.
    setCommand(options.find((candidate) => candidate.name === name)?.launchCommand ?? '');
    setCommandOrigin('scan');
    setSavedState(false);
    setSaveError(null);
  }

  function handleCommandChange(value: string) {
    setTouched(true);
    setCommand(value);
    setCommandOrigin('edited');
    setSavedState(false);
    setSaveError(null);
  }

  async function handleSave() {
    if (selected.length === 0 || !profileSettled || pending) return;
    const trimmed = command.trim();
    setPending(true);
    setSaveError(null);
    setSavedState(false);
    try {
      if (desktop) {
        // The IPC store replaces the command wholesale: send it for a stored or
        // edited value (preserve/apply), never for a scan default.
        const ipcCommand =
          commandOrigin === 'scan' || trimmed.length === 0 ? undefined : trimmed;
        await desktop.setAgentProfile(selected, ipcCommand);
      } else {
        // Service preserves an omitted command; only an explicit edit is an
        // override (a blank edit explicitly clears it).
        await client.saveAgentProfile(
          commandOrigin === 'edited'
            ? { name: selected, launch_command: trimmed }
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
        onChange={(event) => handleAgentChange(event.target.value)}
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

      <Input
        aria-label={t('harness.profile.commandLabel')}
        className="w-72"
        value={command}
        placeholder={t('harness.profile.commandPlaceholder')}
        onChange={(event) => handleCommandChange(event.target.value)}
      />

      <Button
        type="button"
        variant="primary"
        size="small"
        onClick={handleSave}
        disabled={selected.length === 0 || !profileSettled || pending}
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

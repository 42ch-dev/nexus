/**
 * Strategy canvas data hooks — TanStack Query bindings for the Strategy read
 * surface and Idea steering affordance.
 *
 * Write mutations (state, transition, prompt template) live next to the
 * inspector sections that own them so each section can save independently and
 * surface its own partial-failure UI (R-V171P0-QC1-004).
 */
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';

import { parsePresetYaml } from './preset-yaml';

/** Calm overlay refresh cadence (A3 bounded overlay — session-level status). */
const OVERLAY_POLL_MS = 5_000;

/**
 * Parsed preset + parse problems for one preset (A2 read projection).
 *
 * V1.115 P0 T2 (W001): graph projection (`buildStrategyGraph`) moved into the
 * Strategy canvas adapter's `projectGraph`. This hook returns only the parsed
 * preset + revision — the adapter owns projection so the contract is honest.
 */
export function usePresetGraph(presetId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.presets.detail(presetId ?? ''),
    queryFn: async () => {
      const res = await client.getPreset(presetId!);
      const parsed = parsePresetYaml(res.yaml);
      return { preset: res, parsed, revision: parsed.revision ?? 0 };
    },
    enabled: Boolean(presetId),
    staleTime: 30_000,
  });
}

/** Sessions for a preset (A3 overlay source), polled for live status. */
export function usePresetSessions(presetId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: [...queryKeys.sessions.all, 'by-preset', presetId ?? ''],
    queryFn: async () => {
      const res = await client.listSessions();
      return res.items.filter((s) => !presetId || s.preset_id === presetId);
    },
    enabled: Boolean(presetId),
    refetchInterval: OVERLAY_POLL_MS,
  });
}

/** Schedules for a preset (A4 steer/resume targets). */
export function usePresetSchedules(presetId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: [...queryKeys.schedules.all, 'by-preset', presetId ?? ''],
    queryFn: async () => {
      const res = await client.listSchedules();
      return res.items.filter((s) => !presetId || s.preset_id === presetId);
    },
    enabled: Boolean(presetId),
    refetchInterval: OVERLAY_POLL_MS,
  });
}

/** The most recently active session for a preset (drives the live overlay). */
export function useActiveSession(presetId: string | undefined) {
  const sessions = usePresetSessions(presetId);
  return useMemo(() => {
    const items = sessions.data ?? [];
    return items.find((s) => !s.status.toLowerCase().includes('complete'));
  }, [sessions.data]);
}

/** A usable creator_id for a new Run, derived from existing schedules/sessions. */
export function useDerivedCreatorId(presetId: string | undefined): string | undefined {
  const sessions = usePresetSessions(presetId);
  const schedules = usePresetSchedules(presetId);
  return useMemo(
    () => sessions.data?.[0]?.creator_id ?? schedules.data?.[0]?.creator_id,
    [sessions.data, schedules.data],
  );
}

function useErrorToast() {
  const { toast } = useToast();
  const { t } = useTranslation('common');
  /**
   * `descriptionOverride` replaces the error's own message when the mutation
   * knows more than the raw failure (Steer's partial append, W5).
   */
  return (error: unknown, key: string, descriptionOverride?: string) => {
    const description =
      descriptionOverride ??
      (error instanceof NexusClientError
        ? error.message
        : error instanceof Error
          ? error.message
          : t('error.unexpected'));
    toast({ variant: 'error', title: t(key, { defaultValue: key }), description });
  };
}

export interface RunIdeaArgs {
  creatorId: string;
  presetId: string;
  idea: string;
  label?: string;
}

/** Idea → Run: create a new schedule with the Idea as the seed. */
export function useRunStrategy() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('canvas');
  const { toast } = useToast();
  return useMutation({
    mutationFn: (args: RunIdeaArgs) =>
      client.addSchedule({
        creator_id: args.creatorId,
        preset_id: args.presetId,
        seed: args.idea,
        label: args.label ?? `Steer · ${new Date().toLocaleString()}`,
        reason: 'canvas-strategy-idea',
      }),
    onSuccess: (_data, args) => {
      toast({ variant: 'success', title: t('strategy.toast.runQueued'), description: args.presetId });
      void qc.invalidateQueries({ queryKey: queryKeys.schedules.all });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
    },
    onError: (error) => errorToast(error, 'error.couldNotStartStrategyRun'),
  });
}

export interface SteerIdeaArgs {
  scheduleId: string;
  idea: string;
}

/**
 * Steer partial failure (W5, `current-host-contracts.md` §3.3): the Idea was
 * committed as a durable core context version, but the follow-up resume signal
 * was refused. The committed version is never rolled back or re-appended, so
 * the caller must report the resume refusal and refresh the visible version.
 */
class SteerResumeRefusedError extends Error {
  readonly appendedVersion: number;
  readonly refusal: unknown;

  constructor(appendedVersion: number, refusal: unknown) {
    super(`Core context version ${appendedVersion} was appended, but resume was refused`);
    this.name = 'SteerResumeRefusedError';
    this.appendedVersion = appendedVersion;
    this.refusal = refusal;
  }
}

/**
 * True if the schedule refused the signal as its own wait/state conflict (409).
 *
 * The core carries those codes in `details.wire_code` — the public `code` stays
 * the generic `invalid_input` (crates/nexus-core-node/src/core_error.rs,
 * `DomainError::Coded`) — so the coded detail is what the UI must read to
 * present a lawful manual-wait conflict as a conflict.
 */
function isScheduleConflictRefusal(error: unknown): boolean {
  if (!(error instanceof NexusClientError)) return false;
  const wireCode = (error.details as { wire_code?: unknown } | null | undefined)?.wire_code;
  return (
    error.code === 'workflow_wait_conflict'
    || error.code === 'workflow_state_conflict'
    || wireCode === 'workflow_wait_conflict'
    || wireCode === 'workflow_state_conflict'
  );
}

/** Idea → Steer: append the Idea to core context, then signal resume (W5). */
export function useSteerStrategy() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('canvas');
  const { toast } = useToast();
  return useMutation({
    mutationFn: async (args: SteerIdeaArgs) => {
      // Append first: the Idea must be durable before resume counts as success.
      // These are two sequential calls, not one transaction — a refused resume
      // leaves this committed version in place and is never retried.
      const appended = await client.editCoreContext(args.scheduleId, {
        op: 'append',
        body: args.idea,
      });
      try {
        await client.signalSchedule(args.scheduleId, { signal: 'resume' });
      } catch (refusal) {
        throw new SteerResumeRefusedError(appended.new_version, refusal);
      }
      return appended;
    },
    onSuccess: (_data, args) => {
      toast({ variant: 'success', title: t('strategy.toast.ideaSent'), description: args.scheduleId });
      void qc.invalidateQueries({ queryKey: queryKeys.schedules.all });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
    },
    onError: (error, args) => {
      if (error instanceof SteerResumeRefusedError) {
        // The append is durable: refresh the schedule rows so the visible core
        // context version is not stale. The Idea is not re-appended.
        void qc.invalidateQueries({ queryKey: queryKeys.schedules.all });
        errorToast(
          error,
          'error.couldNotSteerStrategy',
          t(
            isScheduleConflictRefusal(error.refusal)
              ? 'strategy.toast.steerResumeConflict'
              : 'strategy.toast.steerResumeRefused',
            { version: error.appendedVersion, scheduleId: args.scheduleId },
          ),
        );
        return;
      }
      errorToast(error, 'error.couldNotSteerStrategy');
    },
  });
}

/** Resume a paused/waiting schedule. */
export function useResumeStrategy() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { t } = useTranslation('canvas');
  const { toast } = useToast();
  return useMutation({
    mutationFn: (scheduleId: string) => client.signalSchedule(scheduleId, { signal: 'resume' }),
    onSuccess: (_data, scheduleId) => {
      toast({ variant: 'success', title: t('strategy.toast.resumed'), description: scheduleId });
      void qc.invalidateQueries({ queryKey: queryKeys.schedules.all });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
    },
    onError: (error) => errorToast(error, 'error.couldNotResumeStrategy'),
  });
}

/** True if the error is a Strategy revision conflict (HTTP 409). */
export function isStrategyConflictError(
  error: unknown,
): error is NexusClientError & { details: { current_revision?: number } } {
  return error instanceof NexusClientError && error.code === 'strategy_conflict';
}

/**
 * Strategy canvas data hooks — TanStack Query bindings for the Strategy read
 * surface and Idea steering affordance.
 *
 * Write mutations (state, transition, prompt template) live next to the
 * inspector sections that own them so each section can save independently and
 * surface its own partial-failure UI (R-V171P0-QC1-004).
 */
import { useMemo } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';

import { buildStrategyGraph, type StrategyGraph } from './strategy-graph';
import { parsePresetYaml } from './preset-yaml';

/** Calm overlay refresh cadence (A3 bounded overlay — session-level status). */
const OVERLAY_POLL_MS = 5_000;

/** Parsed graph + parse problems for one preset (A2 read projection). */
export function usePresetGraph(presetId: string | undefined) {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.presets.detail(presetId ?? ''),
    queryFn: async () => {
      const res = await client.getPreset(presetId!);
      const parsed = parsePresetYaml(res.yaml);
      const graph: StrategyGraph = buildStrategyGraph(parsed);
      return { preset: res, parsed, graph, revision: parsed.revision ?? 0 };
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
  return (error: unknown, title: string) => {
    const description =
      error instanceof NexusClientError
        ? error.message
        : error instanceof Error
          ? error.message
          : 'Unexpected error.';
    toast({ variant: 'error', title, description });
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
      toast({ variant: 'success', title: 'Strategy run queued', description: args.presetId });
      void qc.invalidateQueries({ queryKey: queryKeys.schedules.all });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
    },
    onError: (error) => errorToast(error, 'Could not start Strategy run'),
  });
}

export interface SteerIdeaArgs {
  scheduleId: string;
  idea: string;
}

/** Idea → Steer: signal resume first, then append the Idea to core context. */
export function useSteerStrategy() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { toast } = useToast();
  return useMutation({
    mutationFn: async (args: SteerIdeaArgs) => {
      await client.signalSchedule(args.scheduleId, { signal: 'resume' });
      return client.editCoreContext(args.scheduleId, { op: 'append', body: args.idea });
    },
    onSuccess: (_data, args) => {
      toast({ variant: 'success', title: 'Idea sent to Strategy', description: args.scheduleId });
      void qc.invalidateQueries({ queryKey: queryKeys.schedules.all });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
    },
    onError: (error) => errorToast(error, 'Could not steer Strategy'),
  });
}

/** Resume a paused/waiting schedule. */
export function useResumeStrategy() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { toast } = useToast();
  return useMutation({
    mutationFn: (scheduleId: string) => client.signalSchedule(scheduleId, { signal: 'resume' }),
    onSuccess: (_data, scheduleId) => {
      toast({ variant: 'success', title: 'Strategy resumed', description: scheduleId });
      void qc.invalidateQueries({ queryKey: queryKeys.schedules.all });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
    },
    onError: (error) => errorToast(error, 'Could not resume Strategy'),
  });
}

/** True if the error is a Strategy revision conflict (HTTP 409). */
export function isStrategyConflictError(
  error: unknown,
): error is NexusClientError & { details: { current_revision?: number } } {
  return error instanceof NexusClientError && error.code === 'strategy_conflict';
}

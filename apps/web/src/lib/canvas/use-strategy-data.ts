/**
 * Strategy canvas data hooks — TanStack Query bindings for the α scope
 * (read projection + bounded overlay + Idea→Run/Resume/Steer).
 *
 * A5 verdict (option a): no new read route. The graph comes from
 * `getPreset(id).yaml` parsed client-side (`parsePresetYaml`); the overlay
 * comes from `listSessions` filtered by `preset_id` and polled at a calm
 * cadence (bounded to current-node/status per A5 — completed-path history +
 * child-session progress are V1.71).
 *
 * A4 steering reuses existing schedule/orchestration endpoints (no new DTO):
 *   • Run     → `addSchedule({ creator_id, preset_id, seed, label })`
 *   • Steer   → `editCoreContext(scheduleId, { op:'append', body: idea })` + signal resume
 *   • Resume  → `signalSchedule(scheduleId, { signal:'resume' })`
 *
 * simplify: `creator_id` for a new Run is derived from the most recent
 * schedule/session for the preset (or any schedule). The NexusClient does not
 * yet expose an active-creator endpoint, so a brand-new daemon with zero
 * schedules has no creator to attribute — the Run button is disabled with a
 * helper in that case. Upgrade path: promote an active-creator read method
 * (V1.67 G2 pattern) when the canvas needs first-run author attribution.
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
      return { preset: res, parsed, graph };
    },
    enabled: Boolean(presetId),
    // Preset YAML is immutable unless reloaded; cache generously.
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
    if (items.length === 0) return undefined;
    // Prefer a non-completed session; fall back to the most recent.
    const active = items.find((s) => !s.status.toLowerCase().includes('complete'));
    return active ?? items[0];
  }, [sessions.data]);
}

/** A usable creator_id for a new Run, derived from existing schedules/sessions. */
export function useDerivedCreatorId(presetId: string | undefined): string | undefined {
  const sessions = usePresetSessions(presetId);
  const schedules = usePresetSchedules(presetId);
  return useMemo(() => {
    const fromSession = sessions.data?.[0]?.creator_id;
    const fromSchedule = schedules.data?.[0]?.creator_id;
    return fromSession ?? fromSchedule;
  }, [sessions.data, schedules.data]);
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

/** Idea → Steer: append the Idea to a running schedule's core context, then signal resume. */
export function useSteerStrategy() {
  const client = useNexusClient();
  const qc = useQueryClient();
  const errorToast = useErrorToast();
  const { toast } = useToast();
  return useMutation({
    mutationFn: async (args: SteerIdeaArgs) => {
      await client.editCoreContext(args.scheduleId, {
        op: 'append',
        body: args.idea,
      });
      return client.signalSchedule(args.scheduleId, { signal: 'resume' });
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
    mutationFn: (scheduleId: string) =>
      client.signalSchedule(scheduleId, { signal: 'resume' }),
    onSuccess: (_data, scheduleId) => {
      toast({ variant: 'success', title: 'Strategy resumed', description: scheduleId });
      void qc.invalidateQueries({ queryKey: queryKeys.schedules.all });
      void qc.invalidateQueries({ queryKey: queryKeys.sessions.all });
    },
    onError: (error) => errorToast(error, 'Could not resume Strategy'),
  });
}

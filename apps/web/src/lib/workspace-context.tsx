/**
 * Workspace context (plan v1219 § Shared interfaces 2).
 *
 * Wraps the additive workspace client methods (`listWorkspaces` /
 * `getActiveWorkspace` / `setActiveWorkspace` over the frozen workspace
 * family; create deliberately omitted this iteration — compass D23). Holds the
 * active {@link WorkspaceSummary} and invalidates workspace-scoped queries on
 * switch. Content-creator surfaces only: the developer shell never mounts
 * this provider (IA spec §3.3).
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createContext, useContext, useMemo } from 'react';
import type { ReactNode } from 'react';

import { useActiveCreatorId } from '@/lib/active-creator-context';
import { useNexusClient } from '@/lib/client-context';
import { NexusClientError } from '@/lib/nexus';
import type { ActiveWorkspaceResponse, WorkspaceSummary } from '@/lib/nexus/types';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';

interface WorkspaceContextValue {
  /** The active creator's workspaces from `GET /v1/daemon/workspaces`. */
  workspaces: WorkspaceSummary[];
  /** The workspace whose creator_id + slug match the daemon's active workspace; null while unknown. */
  activeWorkspace: WorkspaceSummary | null;
  /** Switch the daemon's active workspace; refetches workspace-scoped content. */
  switchWorkspace: (workspaceSlug: string) => void;
  /** True while a set-active mutation is in flight. */
  isSwitching: boolean;
}

const WorkspaceContext = createContext<WorkspaceContextValue | null>(null);

/**
 * Active workspace read (`GET /v1/daemon/workspace`), mapping the fresh-home
 * 409 `uninitialized` to null: that is a normal "no active workspace yet"
 * state, not a failure — a thrown 409 would also fire the global query-error
 * toast (409 bypasses the 401-only suppression in main.tsx).
 *
 * Neutral (no provider required): the onboarding wizard's developer step reads
 * the reported `creative_root` without mounting the creator-only provider.
 */
export function useActiveWorkspaceQuery() {
  const client = useNexusClient();
  return useQuery({
    queryKey: queryKeys.workspaces.active(),
    queryFn: async (): Promise<ActiveWorkspaceResponse | null> => {
      try {
        return await client.getActiveWorkspace();
      } catch (error) {
        if (error instanceof NexusClientError && error.status === 409) return null;
        throw error;
      }
    },
  });
}

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const { toast } = useToast();
  const creatorId = useActiveCreatorId();

  const listQuery = useQuery({
    // Creator-scoped cache + fetch: two creators can both own a `default`
    // workspace, so the creator id is part of the key and the query param.
    queryKey: queryKeys.workspaces.list({ creator_id: creatorId }),
    queryFn: async () =>
      client.listWorkspaces({ creator_id: creatorId ?? undefined, limit: 100 }),
  });
  const activeQuery = useActiveWorkspaceQuery();

  const switchMutation = useMutation({
    mutationFn: async (workspaceSlug: string) =>
      client.setActiveWorkspace({
        workspace_slug: workspaceSlug,
        creator_id: creatorId ?? undefined,
      }),
    onSuccess: (_data, workspaceSlug) => {
      // A workspace switch changes the content domain: every cached list/detail
      // is workspace-scoped now, so drop all of it rather than tracking which
      // families happen to be mounted.
      void qc.invalidateQueries();
      const name =
        listQuery.data?.items.find((w) => w.workspace_slug === workspaceSlug)?.display_name ??
        workspaceSlug;
      toast({ title: `已切换到工作区「${name}」`, variant: 'info' });
    },
  });

  const value = useMemo<WorkspaceContextValue>(() => {
    const workspaces: WorkspaceSummary[] = listQuery.data?.items ?? [];
    const active = activeQuery.data ?? null;
    return {
      workspaces,
      // Match creator_id AND slug: the `default` slug exists per creator, so
      // a slug-only match would light the wrong creator's chip (QC W).
      activeWorkspace:
        workspaces.find(
          (w) =>
            active !== null &&
            w.creator_id === active.creator_id &&
            w.workspace_slug === active.workspace_slug,
        ) ?? null,
      switchWorkspace: (workspaceSlug: string) => switchMutation.mutate(workspaceSlug),
      isSwitching: switchMutation.isPending,
    };
  }, [listQuery.data, activeQuery.data, switchMutation]);

  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}

export function useWorkspace(): WorkspaceContextValue {
  const value = useContext(WorkspaceContext);
  if (value === null) {
    throw new Error('useWorkspace must be used within WorkspaceProvider');
  }
  return value;
}

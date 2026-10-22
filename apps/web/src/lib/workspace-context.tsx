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

import { useNexusClient } from '@/lib/client-context';
import type { WorkspaceSummary } from '@/lib/nexus/types';
import { queryKeys } from '@/lib/nexus/query-keys';
import { useToast } from '@/lib/use-toast';

interface WorkspaceContextValue {
  /** The creator's workspaces from `GET /v1/daemon/workspaces`. */
  workspaces: WorkspaceSummary[];
  /** The workspace whose slug matches `active_workspace_slug`; null while unknown. */
  activeWorkspace: WorkspaceSummary | null;
  /** Switch the daemon's active workspace; refetches workspace-scoped content. */
  switchWorkspace: (workspaceSlug: string) => void;
  /** True while a set-active mutation is in flight. */
  isSwitching: boolean;
}

const WorkspaceContext = createContext<WorkspaceContextValue | null>(null);

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const client = useNexusClient();
  const qc = useQueryClient();
  const { toast } = useToast();

  const listQuery = useQuery({
    queryKey: queryKeys.workspaces.list(),
    queryFn: async () => client.listWorkspaces({ limit: 100 }),
  });
  const activeQuery = useQuery({
    queryKey: queryKeys.workspaces.active(),
    queryFn: async () => client.getActiveWorkspace(),
  });

  const switchMutation = useMutation({
    mutationFn: async (workspaceSlug: string) => client.setActiveWorkspace({ workspace_slug: workspaceSlug }),
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
    const activeSlug = activeQuery.data?.workspace_slug;
    return {
      workspaces,
      activeWorkspace: workspaces.find((w) => w.workspace_slug === activeSlug) ?? null,
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

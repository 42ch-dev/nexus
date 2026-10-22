/**
 * Workspace chip row (plan v1219 T3; prototype `onboarding-v3.html` group-tabs).
 *
 * One chip per workspace `display_name`; the active chip reflects
 * `active_workspace_slug`. Switching calls set-active through the workspace
 * context (which invalidates workspace-scoped queries and toasts the § Copy
 * switch toast). No `＋ 新建工作区` chip this iteration — creation is deferred
 * (compass D23, issue I-000063) and this component never calls createWorkspace.
 */
import { useWorkspace } from '@/lib/workspace-context';
import { cn } from '@/lib/utils';

export function WorkspaceChips() {
  const { workspaces, activeWorkspace, switchWorkspace, isSwitching } = useWorkspace();

  if (workspaces.length === 0) {
    return null;
  }

  return (
    <div
      data-testid="workspace-chips"
      role="group"
      aria-label="工作区"
      title="工作区（workspace）— 创作内容的容器，每个有自己的根目录"
      className="flex flex-wrap items-center gap-2"
    >
      {workspaces.map((workspace) => {
        // creator_id + slug: `default` exists per creator, so a slug-only
        // comparison would light the wrong creator's chip (QC W).
        const active =
          activeWorkspace !== null &&
          workspace.creator_id === activeWorkspace.creator_id &&
          workspace.workspace_slug === activeWorkspace.workspace_slug;
        return (
          <button
            key={`${workspace.creator_id}:${workspace.workspace_slug}`}
            type="button"
            aria-pressed={active}
            disabled={isSwitching}
            onClick={() => {
              if (!active) {
                switchWorkspace(workspace.workspace_slug);
              }
            }}
            className={cn(
              'rounded-full border px-3 py-1 text-label-12 font-medium transition-colors duration-state ease-standard motion-reduce:transition-none',
              active
                ? 'border-gray-1000 bg-gray-1000 text-background-100'
                : 'border-gray-alpha-400 text-gray-800 hover:border-gray-1000 hover:text-gray-1000',
            )}
          >
            {workspace.display_name ?? workspace.workspace_slug}
          </button>
        );
      })}
    </div>
  );
}

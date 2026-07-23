import { CreatorHubDualPane } from '@/components/layout/creator-hub-dual-pane';

/**
 * Creator hub content — stable dual-pane shell (V1.134 P3).
 *
 * Shared World/Work tab bar spans both panes; left workspace shows inline
 * create affordance; right pane shows single-kind cards or empty state.
 * Card selection navigates to canvas routes — no controller-stub replace.
 * Canvas routes under `/works/:workId/*` and `/worlds/:worldId/*` stay orthogonal.
 */
export function CreatorHubPage() {
  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="creator-hub-page">
      <CreatorHubDualPane />
    </div>
  );
}

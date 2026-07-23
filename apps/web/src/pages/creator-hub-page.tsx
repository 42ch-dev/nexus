import { CreatorHubDualPane } from '@/components/layout/creator-hub-dual-pane';

/**
 * Creator hub content — browse-only shell (V1.135 P0).
 *
 * World/Work tab bar + card list or empty state. Create lives in the sidebar
 * menu slot ({@link Sidebar} `panelContent`), not in this content column.
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

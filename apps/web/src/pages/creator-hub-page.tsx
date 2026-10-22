import { CreatorHubDualPane } from '@/components/layout/creator-hub-dual-pane';
import { WorkspaceChips } from '@/components/shell/workspace-chips';
import { WorkspaceProvider } from '@/lib/workspace-context';

/**
 * Creator hub content — browse-only shell (V1.135 P0).
 *
 * World/Work tab bar + card list or empty state. Create lives in the sidebar
 * menu slot ({@link Sidebar} `panelContent`), not in this content column.
 * Card selection navigates to canvas routes — no controller-stub replace.
 * Canvas routes under `/works/:workId/*` and `/worlds/:worldId/*` stay orthogonal.
 *
 * The workspace chip row (plan v1219 T3) mounts here — the world and work
 * content apps both render this page; the developer shell never does, so the
 * chips are creator-entrance-only (IA spec §3.3).
 */
export function CreatorHubPage() {
  return (
    <WorkspaceProvider>
      <div className="flex h-full min-h-0 flex-col" data-testid="creator-hub-page">
        <div className="px-4 pt-4 md:px-6 md:pt-6">
          <WorkspaceChips />
        </div>
        <div className="min-h-0 flex-1">
          <CreatorHubDualPane />
        </div>
      </div>
    </WorkspaceProvider>
  );
}

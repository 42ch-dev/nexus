import { useTranslation } from 'react-i18next';
import { NavLink } from 'react-router';
import { Lightbulb } from 'lucide-react';

import { CreatorHubDualPane } from '@/components/layout/creator-hub-dual-pane';
import { CreatorCreatePanel } from '@/components/layout/sidebar';
import { WorkspaceChips } from '@/components/shell/workspace-chips';
import { cn } from '@/lib/utils';
import { WorkspaceProvider } from '@/lib/workspace-context';

/**
 * Creator hub content — browse-only shell (V1.135 P0).
 *
 * World/Work tab bar + card list or empty state. The v1219 shell dropped the
 * sidebar, so the World/Work inline create panel mounts here in the content
 * column ({@link CreatorCreatePanel}, unchanged from the sidebar era).
 * Card selection navigates to canvas routes — no controller-stub replace.
 * Canvas routes under `/works/:workId/*` and `/worlds/:worldId/*` stay orthogonal.
 *
 * The workspace chip row (plan v1219 T3) mounts here — the world and work
 * content apps both render this page; the developer shell never does, so the
 * chips are creator-entrance-only (IA spec §3.3).
 */
export function CreatorHubPage() {
  const { t } = useTranslation('works');
  return (
    <WorkspaceProvider>
      <div className="flex h-full min-h-0 flex-col" data-testid="creator-hub-page">
        <div className="flex flex-col gap-4 px-4 pt-4 md:px-6 md:pt-6">
          <WorkspaceChips />
          <CreatorCreatePanel />
          {/* v1.222 review (6v6gGH) — the pool is creator-scoped, so the rail
              link (mounted only under `/works/:workId`) is unreachable for a
              creator with zero Works. This hub is the landing surface. */}
          <nav className="self-start" aria-label={t('inspiration.title')}>
            <NavLink
              to="/works/inspiration"
              data-testid="creator-hub-inspiration-link"
              className={({ isActive }) =>
                cn(
                  'inline-flex items-center gap-2 rounded-control px-3 py-2 text-label-14 transition-colors duration-state ease-standard motion-reduce:transition-none',
                  isActive
                    ? 'bg-gray-alpha-100 text-gray-1000'
                    : 'text-gray-800 hover:bg-gray-alpha-100 hover:text-gray-1000',
                )
              }
            >
              <Lightbulb className="h-4 w-4 shrink-0 text-gray-700" aria-hidden />
              {t('inspiration.railLink')}
            </NavLink>
          </nav>
        </div>
        <div className="min-h-0 flex-1">
          <CreatorHubDualPane />
        </div>
      </div>
    </WorkspaceProvider>
  );
}

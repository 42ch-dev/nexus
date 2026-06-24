import { ChevronDown, Loader2 } from 'lucide-react';

import { Button } from '@/components/ui/button';

/**
 * Load more control for cursor-paginated infinite queries.
 *
 * Shows nothing when there is no next page or a fetch is not yet possible.
 * Used by the Works dashboard and the Findings view (the two cursor-paginated
 * endpoints per F-P1/F-P2). DESIGN.md §Voice & Content: present participle
 * loading state.
 */
export function LoadMore({
  isFetchingNextPage,
  hasNextPage,
  fetchNextPage,
  label = 'Load more',
}: {
  isFetchingNextPage: boolean;
  hasNextPage: boolean;
  fetchNextPage: () => void;
  label?: string;
}) {
  if (!hasNextPage) return null;
  return (
    <div className="flex justify-center py-4">
      <Button
        type="button"
        variant="secondary"
        size="small"
        onClick={() => fetchNextPage()}
        disabled={isFetchingNextPage}
      >
        {isFetchingNextPage ? (
          <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
        ) : (
          <ChevronDown className="h-4 w-4" aria-hidden />
        )}
        {isFetchingNextPage ? `Loading more…` : label}
      </Button>
    </div>
  );
}

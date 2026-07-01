import { useState } from 'react';

import { FragmentsSection } from '@/components/memory/fragments-section';
import { PendingReviewsSection } from '@/components/memory/pending-reviews-section';
import { SoulSection } from '@/components/memory/soul-section';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState } from '@/components/ui/states';
import { useActiveCreatorId } from '@/api/queries';

/**
 * Memory view (Control Room) — V1.78 Creator Memory review-loop surface
 * (web-ui.md §24).
 *
 * Creator-scoped (`creator_id` on every memory endpoint). The active creator id
 * is derived from existing sessions (mirrors the canvas `useDerivedCreatorId`
 * pattern); the daemon rejects a mismatched creator with 403. Three affordances
 * consuming the shipped memory Local API:
 *   1. Pending-review list (cursor-paginated) with a live count badge + delete.
 *   2. "Review & Summarize" CTA → POST /memory/review with a processing state,
 *      then a result-counters toast; invalidates pending + count + fragments.
 *   3. Fragments browser (read-only, optional keyword filter).
 *
 * Layout (D-UX LOCKED): detail-panel + row-action hybrid — a Control-Room table
 * with a side inspector (matching the V1.77 findings-page pattern), not a
 * canvas graph. `createPendingReview` stays CLI/producer-only.
 *
 * Module shape (R-V179P1-QC1-001): this file is the route/page shell only —
 * active creator lookup, the page-level fragment-keyword state that coordinates
 * the SOUL viz click-to-filter with the fragments browser, the Card layout,
 * and section composition. The three sections live as siblings in
 * `components/memory/`. API mutation/drain semantics stay in `@/api/queries`
 * (P0-owned, untouched here).
 */
export function MemoryPage() {
  const creatorId = useActiveCreatorId();
  // Lifted so the SOUL viz click-to-filter can drive the fragments browser
  // (V1.79 P1 §D integration). FragmentsSection is now controlled.
  const [fragmentKeyword, setFragmentKeyword] = useState('');

  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>Memory</CardTitle>
        <CardDescription>
          Review pending captures, summarize them into long-term memory, and browse fragments.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {!creatorId ? (
          <EmptyState
            title="No active creator"
            description="Start a session or schedule so the memory surface can resolve your creator identity."
          />
        ) : (
          <div className="flex flex-col gap-8">
            <PendingReviewsSection creatorId={creatorId} />
            <FragmentsSection
              creatorId={creatorId}
              keyword={fragmentKeyword}
              onKeywordChange={setFragmentKeyword}
            />
            <SoulSection
              creatorId={creatorId}
              onFilterFragments={(kw) => setFragmentKeyword(kw ?? '')}
            />
          </div>
        )}
      </CardContent>
    </Card>
  );
}

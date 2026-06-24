import { Link } from 'react-router-dom';

import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/states';

/** 404 — part of the Control Room + Setup shell. */
export function NotFoundPage() {
  return (
    <EmptyState
      title="Page not found"
      description="That route is not part of the Control Room + Setup surface."
      action={
        <Button asChild variant="secondary" size="small">
          <Link to="/works">Go to Works</Link>
        </Button>
      }
    />
  );
}

import { useState } from 'react';
import { Plus, BookOpen } from 'lucide-react';

import { EmptyCreateCard } from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<EmptyCreateCard>` (V1.148 P1 promotion).
 *
 * Card-sized empty-state create affordance matching the list-item card
 * footprint. Shows the enabled hover/focus target and the disabled gate
 * (desktop-only features ship disabled with a native tooltip). Copy is
 * caller-owned literal English.
 */
export function EmptyCreateCardFixtures() {
  const [clicks, setClicks] = useState(0);

  return (
    <div data-testid="empty-create-card-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <div className="grid gap-4 sm:grid-cols-2">
          <EmptyCreateCard
            icon={Plus}
            title="Create a Work"
            description="Start a new work to run the local loop."
            onClick={() => setClicks((n) => n + 1)}
            data-testid="empty-create-card-enabled"
          />
          <EmptyCreateCard
            icon={BookOpen}
            title="Import a manuscript"
            description="Available in the desktop app."
            onClick={() => setClicks((n) => n + 1)}
            disabled
            titleAttr="Desktop app only"
            data-testid="empty-create-card-disabled"
          />
        </div>
        <p className="text-copy-13 text-gray-700 mt-4" data-testid="empty-create-card-click-count">
          Enabled card clicked {clicks} {clicks === 1 ? 'time' : 'times'}; the
          disabled card is inert with a native tooltip.
        </p>
      </div>
    </div>
  );
}

import type { ReactNode } from 'react';

import { Card, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';

/**
 * Shared placeholder for Control Room + Setup screens.
 *
 * The app shell, routing, design tokens, and the NexusClient adapter ship in
 * P1; screen content is filled by plan P2 (web-ui.md §6). Each placeholder
 * names the screen + its data source so the route is self-documenting while
 * empty. Copy follows DESIGN.md §Voice & Content (sentence case empty states).
 */
export function ScreenPlaceholder({
  title,
  description,
  dataSource,
  actions,
}: {
  title: string;
  description: string;
  /** Local API route this screen will consume in P2. */
  dataSource: string;
  actions?: ReactNode;
}) {
  return (
    <div className="flex flex-1 items-start">
      <Card className="w-full shadow-card">
        <CardHeader>
          <CardTitle>{title}</CardTitle>
          <CardDescription>{description}</CardDescription>
        </CardHeader>
        <div className="flex flex-col gap-3 text-copy-14 text-gray-900">
          <p className="text-copy-13 text-gray-700">
            This screen is part of the Control Room + Setup MVP and is implemented in plan P2.
          </p>
          <p className="text-copy-13-mono text-gray-800">
            data source: <code className="text-blue-900">{dataSource}</code>
          </p>
          {actions}
        </div>
      </Card>
    </div>
  );
}

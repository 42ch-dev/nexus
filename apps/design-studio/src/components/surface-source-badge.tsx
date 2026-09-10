import { Badge } from '@42ch/nexus-ui';

/** Four-tier import model — V1.128 P3 gallery labeling + V1.187 P2 studio-local. */
export type SurfaceSourceTier =
  | 'extract'
  | 'promoted'
  | 'transitional'
  | 'studio-local';

const TIER_COPY: Record<
  SurfaceSourceTier,
  { shortLabel: string; badgeVariant: 'preset' | 'running' | 'queued' | 'neutral' }
> = {
  extract: {
    shortLabel: 'App presentational extract',
    badgeVariant: 'preset',
  },
  promoted: {
    shortLabel: 'Promoted primitive',
    badgeVariant: 'running',
  },
  transitional: {
    shortLabel: 'Transitional primitive',
    badgeVariant: 'queued',
  },
  'studio-local': {
    shortLabel: 'Studio-local fixture',
    badgeVariant: 'neutral',
  },
};

function isStudioLocalPath(importPath: string): boolean {
  if (importPath.startsWith('@/fixtures/')) return true;
  if (importPath.startsWith('@/components/')) return true;
  if (importPath.startsWith('@/pages/')) return true;
  if (importPath.startsWith('@/lib/')) return true;
  if (importPath === 'DESIGN.md' || importPath === 'DESIGN.dark.md') return true;
  if (importPath.startsWith('./') || importPath.startsWith('../')) return true;
  return false;
}

/** Classify a Studio import path for Surfaces source badges. */
export function classifySurfaceImport(importPath: string): SurfaceSourceTier {
  if (importPath.startsWith('@42ch/nexus-ui')) {
    return 'promoted';
  }
  if (importPath.startsWith('@web-ui')) {
    return 'transitional';
  }
  if (importPath.startsWith('@web-')) {
    return 'extract';
  }
  if (isStudioLocalPath(importPath)) {
    return 'studio-local';
  }
  return 'studio-local';
}

/** Human-readable label for a tier (optionally including the import path). */
export function getSurfaceSourceLabel(
  importPath: string,
  options?: { includePath?: boolean },
): string {
  const tier = classifySurfaceImport(importPath);
  const { shortLabel } = TIER_COPY[tier];
  if (options?.includePath === false) {
    return shortLabel;
  }
  return `${shortLabel} (${importPath})`;
}

export interface SurfaceSourceBadgeProps {
  importPath: string;
}

/** Single import-path badge — extract vs promoted vs transitional vs studio-local. */
export function SurfaceSourceBadge({ importPath }: SurfaceSourceBadgeProps) {
  const tier = classifySurfaceImport(importPath);
  const { shortLabel, badgeVariant } = TIER_COPY[tier];

  // Narrow-viewport safety: the pill must never force document-wide overflow.
  // The full label + path stay visible — the mono path wraps inside the pill
  // (flex-wrap + break-all) instead of the nowrap pill escaping its column.
  return (
    <Badge
      variant={badgeVariant}
      tone="soft"
      className="h-auto min-h-6 max-w-full flex-wrap whitespace-normal py-0.5"
      data-testid={`surface-source-badge-${tier}`}
      data-import-path={importPath}
      title={getSurfaceSourceLabel(importPath)}
    >
      <span className="sr-only">{shortLabel}: </span>
      <span aria-hidden className="whitespace-nowrap">
        {shortLabel}
      </span>
      <code className="min-w-0 break-all text-label-12 font-mono font-normal opacity-90">
        {importPath}
      </code>
    </Badge>
  );
}

export interface SurfaceSourceBadgesProps {
  /** Distinct import paths cited by the section. */
  importPaths: string[];
}

/** Row of source badges for a Surfaces section. */
export function SurfaceSourceBadges({ importPaths }: SurfaceSourceBadgesProps) {
  const uniquePaths = [...new Set(importPaths)];

  if (uniquePaths.length === 0) {
    return null;
  }

  return (
    <div
      className="flex flex-wrap gap-2 mb-4"
      data-testid="surface-source-badges"
      aria-label="Import source tiers"
    >
      {uniquePaths.map((path) => (
        <SurfaceSourceBadge key={path} importPath={path} />
      ))}
    </div>
  );
}

/** Compact legend for the Surfaces layout intro (V1.128 P3). */
export function SurfaceSourceLegend() {
  return (
    <div
      className="mb-6 rounded-card border border-gray-alpha-200 bg-background-100 p-4"
      data-testid="surface-source-legend"
    >
      <p className="text-label-14 font-medium text-gray-1000 mb-2">
        Import tiers
      </p>
      <ul className="flex flex-col gap-2 text-copy-13 text-gray-700">
        <li className="flex flex-wrap items-center gap-2">
          <SurfaceSourceBadge importPath="@web-layout/example" />
          <span>
            Vite/tsconfig alias to an{' '}
            <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
              apps/web
            </code>{' '}
            presentational extract — not an npm package.
          </span>
        </li>
        <li className="flex flex-wrap items-center gap-2">
          <SurfaceSourceBadge importPath="@42ch/nexus-ui" />
          <span>
            Published workspace package — promoted after Studio visual
            acceptance.
          </span>
        </li>
        <li className="flex flex-wrap items-center gap-2">
          <SurfaceSourceBadge importPath="@web-ui/dialog" /> {/* transitional — legend example (not an import) */}
          <span>
            Unpromoted shadcn primitive still mirrored from{' '}
            <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
              apps/web/src/components/ui
            </code>
            .
          </span>
        </li>
        <li className="flex flex-wrap items-center gap-2">
          <SurfaceSourceBadge importPath="@/fixtures/example" />
          <span>
            Studio-local composition under{' '}
            <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
              apps/design-studio/src
            </code>{' '}
            — props-driven fixtures, not App extracts.
          </span>
        </li>
      </ul>
    </div>
  );
}

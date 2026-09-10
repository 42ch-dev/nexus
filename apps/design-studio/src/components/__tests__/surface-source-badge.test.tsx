import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import {
  SurfaceSourceBadge,
  SurfaceSourceBadges,
  classifySurfaceImport,
  getSurfaceSourceLabel,
} from '../surface-source-badge';

describe('classifySurfaceImport', () => {
  it('classifies @42ch/nexus-ui as promoted', () => {
    expect(classifySurfaceImport('@42ch/nexus-ui')).toBe('promoted');
    expect(classifySurfaceImport('@42ch/nexus-ui/button')).toBe('promoted');
  });

  it('classifies @web-ui as transitional', () => {
    expect(classifySurfaceImport('@web-ui/dialog')).toBe('transitional');
  });

  it('classifies recognized @web-* presentational roots as extract', () => {
    expect(classifySurfaceImport('@web-layout/shell-sidebar-chrome')).toBe(
      'extract',
    );
    expect(classifySurfaceImport('@web-canvas/node-chrome-shell')).toBe(
      'extract',
    );
    expect(classifySurfaceImport('@web-setup/agent-picker')).toBe('extract');
    expect(classifySurfaceImport('@web-settings/settings-host-chrome')).toBe(
      'extract',
    );
    expect(
      classifySurfaceImport('@web-global-timeline/global-timeline-list-chrome'),
    ).toBe('extract');
    expect(classifySurfaceImport('@web-shell/selection-submenu')).toBe(
      'extract',
    );
  });

  it('rejects lookalike package prefixes at root/subpath boundaries', () => {
    expect(classifySurfaceImport('@42ch/nexus-ui-legacy')).toBe('studio-local');
    expect(classifySurfaceImport('@web-ui-legacy/dialog')).toBe('studio-local');
    expect(classifySurfaceImport('@web-layouts/not-a-root')).toBe(
      'studio-local',
    );
  });

  it('falls back unrecognized @web-* aliases to studio-local', () => {
    expect(classifySurfaceImport('@web-foo/bar')).toBe('studio-local');
    expect(classifySurfaceImport('@web-lib/utils')).toBe('studio-local');
  });

  it('classifies Studio-local paths as studio-local', () => {
    expect(classifySurfaceImport('@/fixtures/mental-surfacing-fixtures')).toBe(
      'studio-local',
    );
    expect(classifySurfaceImport('@/components/studio-shell-logo')).toBe(
      'studio-local',
    );
    expect(classifySurfaceImport('@/pages/surfaces')).toBe('studio-local');
    expect(classifySurfaceImport('DESIGN.md')).toBe('studio-local');
  });

  it('falls back unknown paths to studio-local', () => {
    expect(classifySurfaceImport('@nexus/design-tokens')).toBe('studio-local');
  });
});

describe('getSurfaceSourceLabel', () => {
  it('includes import path by default', () => {
    expect(getSurfaceSourceLabel('@web-setup/agent-picker')).toBe(
      'App presentational extract (@web-setup/agent-picker)',
    );
    expect(getSurfaceSourceLabel('@/fixtures/canvas-surfaces-fixtures')).toBe(
      'Studio-local fixture (@/fixtures/canvas-surfaces-fixtures)',
    );
  });

  it('can omit the import path', () => {
    expect(
      getSurfaceSourceLabel('@42ch/nexus-ui', { includePath: false }),
    ).toBe('Promoted primitive');
  });
});

describe('SurfaceSourceBadge', () => {
  it('renders extract tier with test id and import path', () => {
    render(<SurfaceSourceBadge importPath="@web-layout/example" />);
    const badge = screen.getByTestId('surface-source-badge-extract');
    expect(badge).toHaveAttribute(
      'data-import-path',
      '@web-layout/example',
    );
    expect(within(badge).getByText('@web-layout/example')).toBeInTheDocument();
  });

  it('renders promoted tier', () => {
    render(<SurfaceSourceBadge importPath="@42ch/nexus-ui" />);
    expect(screen.getByTestId('surface-source-badge-promoted')).toBeInTheDocument();
  });

  it('renders transitional tier', () => {
    render(<SurfaceSourceBadge importPath="@web-ui/dialog" />); // transitional — badge path label (not an import)
    expect(screen.getByTestId('surface-source-badge-transitional')).toBeInTheDocument();
  });

  it('renders studio-local tier', () => {
    render(<SurfaceSourceBadge importPath="@/fixtures/example" />);
    expect(screen.getByTestId('surface-source-badge-studio-local')).toBeInTheDocument();
  });
});

describe('SurfaceSourceBadges', () => {
  it('deduplicates import paths', () => {
    render(
      <SurfaceSourceBadges
        importPaths={[
          '@42ch/nexus-ui',
          '@42ch/nexus-ui',
          '@web-setup/agent-picker',
        ]}
      />,
    );
    const badges = screen.getAllByTestId(/^surface-source-badge-/);
    expect(badges).toHaveLength(2);
  });

  it('returns null when no paths are provided', () => {
    const { container } = render(<SurfaceSourceBadges importPaths={[]} />);
    expect(container).toBeEmptyDOMElement();
  });
});

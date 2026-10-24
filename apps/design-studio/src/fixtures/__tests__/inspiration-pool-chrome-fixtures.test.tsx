import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { InspirationPoolChromeFixtures } from '@/fixtures/inspiration-pool-chrome-fixtures';
import { ThemeProvider } from '@/components/theme-provider';

/**
 * The Inspiration Pool chrome has five states worth eyeballing: the empty pool
 * with its add affordance, the populated list whose row actions follow the
 * stored status (`idea` → Promote + Archive, `promoted` → Work link + Archive,
 * `archived` → read-only), loading, the read error with retry, and a blank add
 * title that keeps submit disabled.
 *
 * The form field ids come from `useId()` (I-000086), so every frame must own
 * distinct, document-unique ids — asserted pairwise across frames plus a
 * negative check proving the uniqueness assertion is not vacuous.
 */
const VARIANTS = [
  'works-inspiration-fixture-empty',
  'works-inspiration-fixture-list',
  'works-inspiration-fixture-loading',
  'works-inspiration-fixture-error',
  'works-inspiration-fixture-add-invalid',
] as const;

/** Every id that appears more than once under `root`. */
function duplicateIds(root: Element): string[] {
  const seen = new Set<string>();
  const duplicates: string[] = [];
  for (const element of Array.from(root.querySelectorAll('[id]'))) {
    if (seen.has(element.id)) duplicates.push(element.id);
    else seen.add(element.id);
  }
  return duplicates;
}

afterEach(() => {
  cleanup();
  document.documentElement.classList.remove('dark');
});

describe('InspirationPoolChromeFixtures', () => {
  it('mounts every variant frame', () => {
    render(<InspirationPoolChromeFixtures />);

    expect(screen.getByTestId('works-inspiration-fixtures')).toBeInTheDocument();
    for (const testId of VARIANTS) {
      expect(screen.getByTestId(testId)).toBeInTheDocument();
    }
  });

  it('names the surface "Inspiration Pool" (compass D8)', () => {
    render(<InspirationPoolChromeFixtures />);

    for (const testId of VARIANTS) {
      expect(
        within(screen.getByTestId(testId)).getByText('Inspiration Pool', {
          selector: 'h3',
        }),
      ).toBeInTheDocument();
    }
  });

  it('renders one row per status with the contract actions', () => {
    render(<InspirationPoolChromeFixtures />);
    const frame = screen.getByTestId('works-inspiration-fixture-list');
    const rows = within(frame).getAllByTestId('inspiration-pool-row');

    expect(rows.map((row) => row.getAttribute('data-status'))).toEqual([
      'idea',
      'promoted',
      'archived',
    ]);

    const [idea, promoted, archived] = rows;

    // idea → Promote + Archive, no Work link yet.
    expect(within(idea).getByTestId('inspiration-pool-row-promote')).toBeEnabled();
    expect(within(idea).getByTestId('inspiration-pool-row-archive')).toBeEnabled();
    expect(within(idea).getByText('Idea')).toBeInTheDocument();
    expect(within(idea).queryAllByRole('link')).toHaveLength(0);

    // promoted → Work link, Archive, no Promote.
    const workLink = within(promoted).getByRole('link', { name: 'Open Work' });
    expect(workLink).toHaveAttribute('href', '/works/work-9d1e0f42/outline');
    expect(within(promoted).queryByTestId('inspiration-pool-row-promote')).toBeNull();
    expect(within(promoted).getByTestId('inspiration-pool-row-archive')).toBeEnabled();
    expect(within(promoted).getByText('Promoted')).toBeInTheDocument();

    // archived → read-only: neither Promote nor Archive nor a Work link.
    expect(within(archived).queryAllByRole('button')).toHaveLength(0);
    expect(within(archived).queryAllByRole('link')).toHaveLength(0);
    expect(within(archived).getByText('Archived')).toBeInTheDocument();

    // Title + added date on every row.
    expect(
      within(idea).getByText('The lighthouse keeper catalogues the ships it failed to save'),
    ).toBeInTheDocument();
    expect(within(idea).getByTestId('inspiration-pool-row-added')).toHaveTextContent(
      'Added: Oct 22, 2026',
    );
    // Row count rendered from the response `total`, labeled for assistive tech.
    expect(within(frame).getByTestId('inspiration-pool-count')).toHaveTextContent(
      /^Inspiration Pool: 3 \/ 3$/,
    );
  });

  it('renders the empty state with the add affordance', () => {
    render(<InspirationPoolChromeFixtures />);
    const frame = screen.getByTestId('works-inspiration-fixture-empty');

    expect(within(frame).getByText('No inspiration yet')).toBeInTheDocument();
    expect(within(frame).getByText('Add a first idea to start the pool.')).toBeInTheDocument();
    expect(within(frame).queryAllByTestId('inspiration-pool-row')).toHaveLength(0);
    expect(within(frame).getByLabelText('Title')).toBeInTheDocument();
  });

  it('renders the loading frame', () => {
    render(<InspirationPoolChromeFixtures />);
    const frame = screen.getByTestId('works-inspiration-fixture-loading');

    expect(within(frame).getByText('Loading inspiration…')).toBeInTheDocument();
    expect(within(frame).queryAllByTestId('inspiration-pool-row')).toHaveLength(0);
  });

  it('renders the read error with the shared retry affordance', () => {
    render(<InspirationPoolChromeFixtures />);
    const frame = screen.getByTestId('works-inspiration-fixture-error');

    expect(
      within(frame).getByText(
        'The daemon did not return the inspiration pool. Check that the local daemon is running.',
      ),
    ).toBeInTheDocument();
    expect(within(frame).getByRole('alert')).toBeInTheDocument();
    expect(within(frame).getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('keeps submit disabled while the title is blank, then submits the trimmed value', () => {
    render(<InspirationPoolChromeFixtures />);
    const frame = screen.getByTestId('works-inspiration-fixture-add-invalid');
    const submit = within(frame).getByTestId('inspiration-pool-add-submit');
    const input = within(frame).getByLabelText('Title');

    expect(submit).toBeDisabled();

    fireEvent.change(input, { target: { value: '   ' } });
    expect(submit).toBeDisabled();

    fireEvent.change(input, { target: { value: '  A drowned orchestra  ' } });
    expect(submit).toBeEnabled();

    fireEvent.click(submit);
    expect(within(frame).getByTestId('works-inspiration-fixture-submitted')).toHaveTextContent(
      'A drowned orchestra',
    );
  });

  it('gives every frame distinct, document-unique form ids', () => {
    render(<InspirationPoolChromeFixtures />);

    // Positive: pairwise-distinct ids across the five mounted frames.
    const ids = VARIANTS.map((testId) => {
      const input = screen.getByTestId(testId).querySelector('input');
      expect(input?.id).toBeTruthy();
      return input!.id;
    });
    expect(new Set(ids).size).toBe(VARIANTS.length);

    // Positive: no id repeats anywhere in the document.
    expect(duplicateIds(document.body)).toEqual([]);
  });

  it('flags literal duplicate ids — negative check for the uniqueness assertion', () => {
    const { container } = render(
      <div>
        <span id="inspiration-title" />
        <span id="inspiration-title" />
      </div>,
    );

    // If the chrome hardcoded literal ids, the Set-size assertion above would
    // fail exactly like this.
    expect(duplicateIds(container)).toEqual(['inspiration-title']);
  });

  it('renders every variant under both themes', () => {
    for (const forced of ['light', 'dark'] as const) {
      render(
        <ThemeProvider forcedTheme={forced}>
          <InspirationPoolChromeFixtures />
        </ThemeProvider>,
      );

      // Proves ThemeProvider applied the resolved `<html>.dark` marker, not a
      // fixture-owned class (vitest css:false leaves class strings uncomputed).
      expect(document.documentElement.classList.contains('dark')).toBe(forced === 'dark');
      for (const testId of VARIANTS) {
        expect(screen.getByTestId(testId)).toBeInTheDocument();
      }

      cleanup();
    }
  });
});

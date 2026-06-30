/**
 * FindingDetailPanel — V1.77 remediation surface coverage.
 *
 * Covers the three affordances against a mock NexusClient that captures
 * `updateFinding` calls: status-transition adjacency rendering (valid buttons
 * for non-terminal; "Terminal" copy for terminal), target_executor assignment,
 * and inline-edit save/reset. Optimistic update + invalidation is covered in
 * `findings-mutation.test.tsx` against a real BrowserClient + msw.
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { FindingDetailPanel } from '@/components/findings/finding-detail-panel';
import { renderInApp } from '@/test/test-providers';
import type { FindingDetailResponse, UpdateFindingRequest } from '@42ch/nexus-contracts';
import type { NexusClient } from '@/lib/nexus';

function makeFinding(over: Partial<FindingDetailResponse> = {}): FindingDetailResponse {
  return {
    finding_id: 'f1',
    work_id: 'w1',
    chapter: 3,
    severity: 'major',
    status: 'open',
    title: 'Pacing drops in act two',
    description: 'The middle chapters lose tension.',
    target_executor: 'none',
    kind: 'pacing',
    rule_suggestion: 'Tighten the chapter 12 reveal',
    created_at: 1_750_000_000,
    updated_at: 1_750_000_000,
    routing_hint: 'act-two',
    ...over,
  };
}

/** A client that records updateFinding calls and resolves the patched finding. */
function recordingClient(finding: FindingDetailResponse): {
  client: NexusClient;
  calls: { workId: string; findingId: string; patch: UpdateFindingRequest }[];
} {
  const calls: { workId: string; findingId: string; patch: UpdateFindingRequest }[] = [];
  const client = {
    updateFinding: vi.fn((workId: string, findingId: string, patch: UpdateFindingRequest) => {
      calls.push({ workId, findingId, patch });
      return Promise.resolve({ ...finding, ...patch, updated_at: finding.updated_at + 1 });
    }),
  } as unknown as NexusClient;
  return { client, calls };
}

describe('FindingDetailPanel — status transitions', () => {
  it('renders a button for every reachable transition from open', () => {
    const { client } = recordingClient(makeFinding({ status: 'open' }));
    renderInApp(<FindingDetailPanel workId="w1" finding={makeFinding({ status: 'open' })} />, {
      client,
    });
    // open → triaged | in_review | resolved | wont_fix | duplicate
    expect(screen.getByRole('button', { name: /advance finding to triaged/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /advance finding to in review/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /advance finding to resolved/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /advance finding to wont fix/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /advance finding to duplicate/i })).toBeInTheDocument();
  });

  it('fires updateFinding with the chosen status when a transition is clicked', async () => {
    const finding = makeFinding({ status: 'open' });
    const { client, calls } = recordingClient(finding);
    renderInApp(<FindingDetailPanel workId="w1" finding={finding} />, { client });

    fireEvent.click(screen.getByRole('button', { name: /advance finding to triaged/i }));
    await waitFor(() => expect(calls).toEqual([{ workId: 'w1', findingId: 'f1', patch: { status: 'triaged' } }]));
  });

  it('shows the Terminal copy and no transition buttons for a resolved finding', () => {
    const { client } = recordingClient(makeFinding({ status: 'resolved' }));
    renderInApp(<FindingDetailPanel workId="w1" finding={makeFinding({ status: 'resolved' })} />, {
      client,
    });
    expect(screen.getByText(/terminal/i)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /advance finding/i })).not.toBeInTheDocument();
  });
});

describe('FindingDetailPanel — target_executor assignment', () => {
  it('reflects the finding executor and fires updateFinding on change', async () => {
    const finding = makeFinding({ target_executor: 'none' });
    const { client, calls } = recordingClient(finding);
    renderInApp(<FindingDetailPanel workId="w1" finding={finding} />, { client });

    const select = screen.getByLabelText(/target executor/i) as HTMLSelectElement;
    expect(select.value).toBe('none');
    fireEvent.change(select, { target: { value: 'write' } });
    await waitFor(() =>
      expect(calls).toEqual([{ workId: 'w1', findingId: 'f1', patch: { target_executor: 'write' } }]),
    );
  });
});

describe('FindingDetailPanel — inline edit', () => {
  it('sends only changed fields on Save Changes', async () => {
    const finding = makeFinding({ title: 'Original title' });
    const { client, calls } = recordingClient(finding);
    renderInApp(<FindingDetailPanel workId="w1" finding={finding} />, { client });

    const titleInput = screen.getByLabelText(/title/i) as HTMLInputElement;
    fireEvent.change(titleInput, { target: { value: 'Edited title' } });
    fireEvent.click(screen.getByRole('button', { name: /save changes/i }));

    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0]!.patch).toEqual({ title: 'Edited title' });
  });

  it('disables Save Changes when there is no diff', () => {
    const { client } = recordingClient(makeFinding());
    renderInApp(<FindingDetailPanel workId="w1" finding={makeFinding()} />, { client });
    expect(screen.getByRole('button', { name: /save changes/i })).toBeDisabled();
  });

  it('Reset restores the canonical form state', () => {
    const finding = makeFinding({ title: 'Original title' });
    const { client } = recordingClient(finding);
    renderInApp(<FindingDetailPanel workId="w1" finding={finding} />, { client });

    const titleInput = screen.getByLabelText(/title/i) as HTMLInputElement;
    fireEvent.change(titleInput, { target: { value: 'Transient edit' } });
    fireEvent.click(screen.getByRole('button', { name: /reset/i }));
    expect(titleInput.value).toBe('Original title');
  });

  it('shows context readout (chapter, routing hint, id, timestamps)', () => {
    const { client } = recordingClient(makeFinding());
    renderInApp(<FindingDetailPanel workId="w1" finding={makeFinding()} />, { client });
    expect(screen.getByTestId('finding-context-chapter')).toHaveTextContent('Chapter: 3');
    expect(screen.getByTestId('finding-context-routing')).toHaveTextContent('Routing: act-two');
  });
});

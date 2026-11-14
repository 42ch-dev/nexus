/**
 * Live apply state on the compose surface (v241 P3.5 T3/T4) — one test per
 * row of the plan's state table:
 *
 * - no apply yet           → Apply is offered on approved snapshots; no live block
 * - an apply is live       → the live approval id and applied-at render
 * - live older than newest → a non-blocking "Newer approval available" badge
 * - rollback available     → the same Apply control, with append-only copy
 *
 * The badge compares `approval_id`s only (compass D11); it never blocks
 * apply or rollback and disappears when the live approval is the newest.
 */
import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type {
  ApprovedStrategySnapshot,
  StrategyAppliedStatus,
} from '@42ch/nexus-contracts';

import { renderInApp } from '@/test/test-providers';
import { candidateDocument, simulationTrajectory } from '@/test/simulation-fixture';

import { ApproveDisclosure } from './approve-disclosure';

function snapshot(approvalId = 'apr_123', revision = 3): ApprovedStrategySnapshot {
  return {
    approval_id: approvalId,
    strategy_id: 'user/demo',
    candidate_revision: revision,
    document: candidateDocument({ revision }),
    content_hash: `hash-${revision}`,
    trajectory: simulationTrajectory({ ir_content_hash: `hash-${revision}` }),
    pins: [{ id: 'prompt-step', version: '1.0.0', content_hash: 'abc123' }],
    approver: 'creator-1',
    approved_at: '2026-11-13T00:00:00Z',
  };
}

function appliedStatus(overrides: Partial<StrategyAppliedStatus> = {}): StrategyAppliedStatus {
  return {
    current_apply: {
      apply_id: 'app_1',
      approval_id: 'apr_123',
      content_hash: 'hash-3',
      applied_at: '2026-11-13T01:00:00Z',
      supersedes_apply_id: null,
    },
    newest_approval: {
      approval_id: 'apr_123',
      content_hash: 'hash-3',
      candidate_revision: 3,
      approved_at: '2026-11-13T00:00:00Z',
    },
    ...overrides,
  };
}

function renderDisclosure(
  props: Partial<Parameters<typeof ApproveDisclosure>[0]> = {},
) {
  return renderInApp(
    <ApproveDisclosure
      open
      onOpenChange={() => {}}
      status="ready"
      trajectory={simulationTrajectory()}
      error={null}
      onApprove={() => {}}
      {...props}
    />,
  );
}

const applyButton = () => screen.getByRole('button', { name: 'Apply snapshot' });

describe('ApproveDisclosure — live apply state (compose path)', () => {
  it('no apply yet: offers Apply on an approved snapshot, shows no live block and no badge', () => {
    renderDisclosure({
      approval: { kind: 'approved', snapshot: snapshot() },
      onApply: vi.fn(),
      appliedStatus: { current_apply: null, newest_approval: null },
    });

    expect(screen.queryByTestId('apply-live')).toBeNull();
    expect(screen.queryByTestId('apply-newer-available')).toBeNull();
    expect(applyButton()).toBeEnabled();
  });

  it('an apply is live: shows the live approval id and applied-at', () => {
    renderDisclosure({
      approval: { kind: 'approved', snapshot: snapshot() },
      onApply: vi.fn(),
      appliedStatus: appliedStatus(),
    });

    const live = screen.getByTestId('apply-live');
    expect(live).toHaveTextContent('apr_123');
    expect(live).toHaveTextContent('2026-11-13T01:00:00Z');
    // Ids match → the badge is absent.
    expect(screen.queryByTestId('apply-newer-available')).toBeNull();
  });

  it('live apply is not the newest approval: shows the non-blocking badge and keeps Apply enabled', () => {
    renderDisclosure({
      approval: { kind: 'approved', snapshot: snapshot('apr_456', 4) },
      onApply: vi.fn(),
      appliedStatus: appliedStatus({
        newest_approval: {
          approval_id: 'apr_456',
          content_hash: 'hash-4',
          candidate_revision: 4,
          approved_at: '2026-11-14T00:00:00Z',
        },
      }),
    });

    expect(screen.getByTestId('apply-newer-available')).toHaveTextContent(
      'Newer approval available',
    );
    // Non-blocking: the badge neither hides nor demotes the live identity,
    // and Apply stays enabled.
    expect(screen.getByTestId('apply-live')).toHaveTextContent('apr_123');
    expect(applyButton()).toBeEnabled();
  });

  it('a previous apply exists: rollback is the same Apply control, with append-only copy and no delete affordance', () => {
    const onApply = vi.fn();
    renderDisclosure({
      // The dialog holds the superseded approval; aiming Apply at it is the rollback.
      approval: { kind: 'approved', snapshot: snapshot('apr_123', 3) },
      onApply,
      appliedStatus: appliedStatus({
        current_apply: {
          apply_id: 'app_2',
          approval_id: 'apr_456',
          content_hash: 'hash-4',
          applied_at: '2026-11-14T01:00:00Z',
          supersedes_apply_id: 'app_1',
        },
        newest_approval: {
          approval_id: 'apr_456',
          content_hash: 'hash-4',
          candidate_revision: 4,
          approved_at: '2026-11-14T00:00:00Z',
        },
      }),
    });

    expect(screen.getByTestId('apply-rollback-note')).toHaveTextContent(
      'append-only',
    );
    // One control, no second button and nothing that reads as deletion.
    expect(screen.getAllByRole('button', { name: 'Apply snapshot' })).toHaveLength(1);
    expect(screen.queryByRole('button', { name: /delete|remove|rollback/i })).toBeNull();
    fireEvent.click(applyButton());
    expect(onApply).toHaveBeenCalledTimes(1);
  });

  it('shows no badge without a newest approval, even with a live apply', () => {
    renderDisclosure({
      approval: { kind: 'approved', snapshot: snapshot() },
      onApply: vi.fn(),
      appliedStatus: appliedStatus({ newest_approval: null }),
    });

    expect(screen.getByTestId('apply-live')).toBeInTheDocument();
    expect(screen.queryByTestId('apply-newer-available')).toBeNull();
  });
});

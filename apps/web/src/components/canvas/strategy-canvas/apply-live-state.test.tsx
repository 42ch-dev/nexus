/**
 * Live apply state and the Apply target list on the compose surface
 * (v241 P3.5 T3/T4) — one test per row of the plan's state table:
 *
 * - no apply yet           → Apply is offered on a listed approval; no live block
 * - an apply is live       → the live approval id and applied-at render
 * - live older than newest → a non-blocking "Newer approval available" badge
 * - rollback available     → the same Apply control aimed at a superseded approval
 *
 * Every comparison uses `approval_id`s only (compass D11). The rollback note has
 * exactly three cases: aimed at the live approval → an `already_applied` refusal,
 * no note; aimed at the newest approval → applying moves forward, no note; any
 * other listed approval → applying it moves production back, so the note shows.
 */
import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type {
  ApprovedStrategySnapshot,
  StrategyAppliedStatus,
  StrategyApprovals,
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

function approvalRow(
  approvalId = 'apr_123',
  revision = 3,
): StrategyApprovals['items'][number] {
  return {
    approval_id: approvalId,
    content_hash: `hash-${revision}`,
    candidate_revision: revision,
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
const targetSelect = () => screen.getByTestId('apply-target-select');

describe('ApproveDisclosure — live apply state (compose path)', () => {
  it('no apply yet: offers Apply on the listed approval, no live block and no badge', () => {
    renderDisclosure({
      onApply: vi.fn(),
      approvals: [approvalRow()],
      appliedStatus: { current_apply: null, newest_approval: null },
    });

    expect(screen.queryByTestId('apply-live')).toBeNull();
    expect(screen.queryByTestId('apply-newer-available')).toBeNull();
    expect(targetSelect()).toHaveValue('apr_123');
    expect(applyButton()).toBeEnabled();
  });

  it('an apply is live: shows the live approval id and applied-at', () => {
    renderDisclosure({
      onApply: vi.fn(),
      approvals: [approvalRow()],
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
      onApply: vi.fn(),
      // Newest first, as the read orders it; the default target is that newest row.
      approvals: [approvalRow('apr_456', 4), approvalRow()],
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

  it('aimed at the newest approval: no rollback note, the D9 badge stands alone', () => {
    renderDisclosure({
      onApply: vi.fn(),
      approvals: [approvalRow('apr_456', 4), approvalRow()],
      // The post-approve happy path: the dialog holds the newest approval while
      // the live apply is older. Applying it moves forward, not back.
      appliedStatus: appliedStatus({
        newest_approval: {
          approval_id: 'apr_456',
          content_hash: 'hash-4',
          candidate_revision: 4,
          approved_at: '2026-11-14T00:00:00Z',
        },
      }),
    });

    expect(targetSelect()).toHaveValue('apr_456');
    expect(screen.getByTestId('apply-newer-available')).toBeInTheDocument();
    expect(screen.queryByTestId('apply-rollback-note')).toBeNull();
  });

  it('aimed at the live approval: applying is an already_applied refusal, so no rollback note', () => {
    renderDisclosure({
      onApply: vi.fn(),
      approvals: [approvalRow('apr_456', 4), approvalRow()],
      applyTargetId: 'apr_123',
      appliedStatus: appliedStatus({
        newest_approval: {
          approval_id: 'apr_456',
          content_hash: 'hash-4',
          candidate_revision: 4,
          approved_at: '2026-11-14T00:00:00Z',
        },
      }),
    });

    expect(targetSelect()).toHaveValue('apr_123');
    expect(screen.queryByTestId('apply-rollback-note')).toBeNull();
  });

  it('a superseded target is the rollback: note, the same one Apply control, and that target applied', () => {
    const onApply = vi.fn();
    const superseded = approvalRow('apr_000', 2);
    renderDisclosure({
      onApply,
      approvals: [approvalRow('apr_456', 4), approvalRow(), superseded],
      applyTargetId: superseded.approval_id,
      appliedStatus: appliedStatus({
        newest_approval: {
          approval_id: 'apr_456',
          content_hash: 'hash-4',
          candidate_revision: 4,
          approved_at: '2026-11-14T00:00:00Z',
        },
      }),
    });

    expect(targetSelect()).toHaveValue('apr_000');
    expect(screen.getByTestId('apply-rollback-note')).toHaveTextContent('append-only');
    // One control, no second button and nothing that reads as deletion.
    expect(screen.getAllByRole('button', { name: 'Apply snapshot' })).toHaveLength(1);
    expect(screen.queryByRole('button', { name: /delete|remove|rollback/i })).toBeNull();
    fireEvent.click(applyButton());
    expect(onApply).toHaveBeenCalledWith(superseded);
  });

  it('picking another listed approval aims Apply at it', () => {
    const onSelectApplyTarget = vi.fn();
    renderDisclosure({
      onApply: vi.fn(),
      approvals: [approvalRow('apr_456', 4), approvalRow('apr_123', 3)],
      onSelectApplyTarget,
      appliedStatus: appliedStatus(),
    });

    fireEvent.change(targetSelect(), { target: { value: 'apr_123' } });
    expect(onSelectApplyTarget).toHaveBeenCalledWith('apr_123');
  });

  it('the frozen approval summary carries no Apply control of its own', () => {
    renderDisclosure({
      approval: { kind: 'approved', snapshot: snapshot() },
      onApply: vi.fn(),
      approvals: [approvalRow()],
      appliedStatus: appliedStatus(),
    });

    expect(screen.getByTestId('approve-approved')).toHaveTextContent('apr_123');
    expect(screen.getAllByRole('button', { name: 'Apply snapshot' })).toHaveLength(1);
    expect(screen.getByTestId('apply-targets')).toContainElement(applyButton());
  });

  it('a pending live read says so instead of silently rendering "no apply yet"', () => {
    renderDisclosure({
      onApply: vi.fn(),
      approvals: [approvalRow()],
      appliedStatus: null,
      appliedStatusPending: true,
    });

    expect(screen.getByTestId('apply-live-pending')).toBeInTheDocument();
    expect(screen.queryByTestId('apply-live')).toBeNull();
    expect(screen.queryByTestId('apply-newer-available')).toBeNull();
  });

  it('a failed live read names the failure instead of silently rendering "no apply yet"', () => {
    renderDisclosure({
      onApply: vi.fn(),
      approvals: [approvalRow()],
      appliedStatus: null,
      appliedStatusError: 'daemon unreachable',
    });

    expect(screen.getByTestId('apply-live-error')).toHaveTextContent('daemon unreachable');
    expect(screen.queryByTestId('apply-live')).toBeNull();
  });

  it('a failed approvals read names the failure instead of dropping the Apply control', () => {
    renderDisclosure({
      onApply: vi.fn(),
      approvals: null,
      approvalsError: 'daemon unreachable',
      appliedStatus: appliedStatus(),
    });

    expect(screen.getByTestId('apply-targets-error')).toHaveTextContent('daemon unreachable');
    expect(screen.queryByRole('button', { name: 'Apply snapshot' })).toBeNull();
  });
});

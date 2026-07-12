/**
 * FB-SE-004 — Keyboard edge-creation dialog (§4.4 pointer alternative).
 *
 * A Radix-based dialog that lets keyboard-only authors create a transition
 * without dragging: choose source → choose target → choose edge kind, then
 * commit with **Create Transition**. The commit routes through the same
 * `strategy.patch_transition` with `op: "create"` as the spatial path
 * (FB-SE-002). Radix Dialog handles focus trap, Escape, and return-focus
 * automatically (§4.4 a11y). Voice & Content labels are locked by the
 * primary spec.
 */
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input, Label, Select } from '@/components/ui';
import type { PresetState } from '@/lib/canvas/preset-yaml';

export interface KeyboardCreateArgs {
  sourceStateId: string;
  targetStateId: string;
  transitionKind: 'next' | 'branch' | 'default';
  condition?: string;
}

export interface EdgeCreateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  states: PresetState[];
  onCommit: (args: KeyboardCreateArgs) => void;
  isCommitting: boolean;
}

export function EdgeCreateDialog({
  open,
  onOpenChange,
  states,
  onCommit,
  isCommitting,
}: EdgeCreateDialogProps) {
  const { t } = useTranslation('canvas');
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [kind, setKind] = useState<'next' | 'branch' | 'default'>('next');
  const [condition, setCondition] = useState('');

  const edgeKindOptions = [
    { value: 'next', label: t('strategy.edgeCreate.kind.next') },
    { value: 'branch', label: t('strategy.edgeCreate.kind.branch') },
    { value: 'default', label: t('strategy.edgeCreate.kind.default') },
  ] as const;

  // Reset the form each time the dialog opens.
  useEffect(() => {
    if (open) {
      setSourceId('');
      setTargetId('');
      setKind('next');
      setCondition('');
    }
  }, [open]);

  // If the source changes and the target is no longer valid, clear target.
  // Moved to useEffect to avoid setState during render (QC3 W-001).
  const targetOptions = states.filter((s) => s.id !== sourceId);
  useEffect(() => {
    if (targetId && targetId === sourceId) {
      setTargetId('');
    }
  }, [sourceId]); // eslint-disable-line react-hooks/exhaustive-deps -- only react to source changes

  const canCommit = sourceId !== '' && targetId !== '' && !isCommitting;

  function handleCommit() {
    if (!canCommit) return;
    const args: KeyboardCreateArgs = {
      sourceStateId: sourceId,
      targetStateId: targetId,
      transitionKind: kind,
    };
    if (condition.trim()) args.condition = condition.trim();
    onCommit(args);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={t('strategy.edgeCreate.title')} description={t('strategy.edgeCreate.description')}>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            handleCommit();
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-source">{t('strategy.edgeCreate.sourceLabel')}</Label>
            <Select
              id="edge-create-source"
              value={sourceId}
              onChange={(e) => setSourceId(e.target.value)}
              autoFocus
            >
              <option value="" disabled>
                {t('strategy.edgeCreate.sourcePlaceholder')}
              </option>
              {states.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.id}
                </option>
              ))}
            </Select>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-target">{t('strategy.edgeCreate.targetLabel')}</Label>
            <Select
              id="edge-create-target"
              value={targetId}
              onChange={(e) => setTargetId(e.target.value)}
              disabled={sourceId === ''}
            >
              <option value="" disabled>
                {sourceId === ''
                  ? t('strategy.edgeCreate.targetPlaceholderLocked')
                  : t('strategy.edgeCreate.targetPlaceholder')}
              </option>
              {targetOptions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.id}
                </option>
              ))}
            </Select>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-kind">{t('strategy.edgeCreate.kindLabel')}</Label>
            <Select id="edge-create-kind" value={kind} onChange={(e) => setKind(e.target.value as typeof kind)}>
              {edgeKindOptions.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </Select>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="edge-create-condition">{t('strategy.edgeCreate.conditionLabel')}</Label>
            <Input
              id="edge-create-condition"
              value={condition}
              onChange={(e) => setCondition(e.target.value)}
              placeholder={t('strategy.edgeCreate.conditionPlaceholder')}
            />
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => onOpenChange(false)}
              disabled={isCommitting}
            >
              {t('strategy.edgeCreate.cancel')}
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={!canCommit}>
              {isCommitting ? t('strategy.edgeCreate.creating') : t('strategy.edgeCreate.submit')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

import { useEffect, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useCreateWorld } from '@/api/queries';

export function CreateWorldDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const create = useCreateWorld();
  const navigate = useNavigate();
  const { t } = useTranslation('shell');
  const [title, setTitle] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setTitle('');
      setError(null);
    }
  }, [open]);

  const valid = title.trim().length > 0;

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!valid) {
      setError(t('worldCreate.validationError'));
      return;
    }
    try {
      const res = await create.mutateAsync({ title: title.trim() });
      onOpenChange(false);
      navigate(`/worlds/${res.world_id}/timeline`);
    } catch {
      // Error toast already fired by the mutation's onError callback.
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('worldCreate.title')}
        description={t('worldCreate.description')}
      >
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="world-title">{t('worldCreate.titleLabel')}</Label>
            <Input
              id="world-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder={t('worldCreate.titlePlaceholder')}
              invalid={Boolean(error) && title.trim().length === 0}
              autoFocus
            />
          </div>
          {error && <p className="text-copy-13 text-red-700">{error}</p>}
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => onOpenChange(false)}>
              {t('common:action.cancel')}
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={!valid || create.isPending}>
              {create.isPending ? t('worldCreate.creating') : t('worldCreate.create')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
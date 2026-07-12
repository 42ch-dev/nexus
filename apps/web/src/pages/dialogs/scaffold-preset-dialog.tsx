import { useEffect, useState, type FormEvent } from 'react';

import { useTranslation } from 'react-i18next';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useScaffoldPreset } from '@/api/queries';
import { useToast } from '@/lib/use-toast';

/**
 * Scaffold Preset dialog — POST /v1/daemon/presets.
 *
 * Creates a new user preset scaffold from a name. The daemon writes the file
 * under the home layout and returns the path. The author then edits that file
 * and validates it before running.
 */
export function ScaffoldPresetDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const scaffold = useScaffoldPreset();
  const { toast } = useToast();
  const { t } = useTranslation('shell');
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setName('');
      setError(null);
    }
  }, [open]);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!name.trim()) {
      setError(t('scaffoldPreset.validationError'));
      return;
    }
    try {
      const res = await scaffold.mutateAsync({ name: name.trim() });
      toast({
        variant: 'success',
        title: t('common:toast.presetScaffolded'),
        description: res.path,
      });
      onOpenChange(false);
    } catch {
      // Error toast already fired by the mutation's onError callback.
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('scaffoldPreset.title')}
        description={t('scaffoldPreset.description')}
      >
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="preset-name">{t('scaffoldPreset.nameLabel')}</Label>
            <Input
              id="preset-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t('scaffoldPreset.namePlaceholder')}
              invalid={Boolean(error) && name.trim().length === 0}
              autoFocus
            />
            {error && <p className="text-copy-13 text-red-700">{error}</p>}
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => onOpenChange(false)}>
              {t('common:action.cancel')}
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={!name.trim() || scaffold.isPending}>
              {scaffold.isPending ? t('scaffoldPreset.creating') : t('scaffoldPreset.submit')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

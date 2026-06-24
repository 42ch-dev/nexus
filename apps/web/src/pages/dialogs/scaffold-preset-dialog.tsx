import { useEffect, useState, type FormEvent } from 'react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useScaffoldPreset } from '@/api/queries';
import { useToast } from '@/lib/use-toast';

/**
 * Scaffold Preset dialog — POST /v1/local/presets.
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
      setError('A name is required.');
      return;
    }
    try {
      const res = await scaffold.mutateAsync({ name: name.trim() });
      toast({
        variant: 'success',
        title: 'Preset scaffolded',
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
        title="Scaffold Preset"
        description="Create a new user preset from a name."
      >
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="preset-name">Name</Label>
            <Input
              id="preset-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. my-chapter-preset"
              invalid={Boolean(error) && name.trim().length === 0}
              autoFocus
            />
            {error && <p className="text-copy-13 text-red-700">{error}</p>}
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={!name.trim() || scaffold.isPending}>
              {scaffold.isPending ? 'Scaffolding preset…' : 'Scaffold Preset'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

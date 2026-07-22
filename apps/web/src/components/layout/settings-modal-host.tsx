import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Settings } from 'lucide-react';

import { Dialog, DialogContent, DialogTrigger } from '@/components/ui/dialog';

export function SettingsModalHost({ children }: { children?: ReactNode }) {
  const { t } = useTranslation('shell');
  const [open, setOpen] = useState(false);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <button
          type="button"
          aria-label={t('settings.title')}
          className="rounded-control p-1 text-gray-700 transition-colors duration-state ease-standard hover:bg-gray-alpha-100 hover:text-gray-1000"
        >
          <Settings className="h-5 w-5" aria-hidden />
        </button>
      </DialogTrigger>
      <DialogContent
        title={t('settings.title')}
        className="!w-[80vw] !max-w-[80vw] !h-[80vh] !max-h-[80vh]"
      >
        <div className="flex-1 overflow-auto">
          {children}
        </div>
      </DialogContent>
    </Dialog>
  );
}
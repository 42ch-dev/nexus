import { Link } from 'react-router-dom';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/states';

/** 404 — part of the Control Room + Setup shell. */
export function NotFoundPage() {
  const { t } = useTranslation('shell');
  return (
    <EmptyState
      title={t('notFound.title')}
      description={t('notFound.description')}
      action={
        <Button asChild variant="secondary" size="small">
          <Link to="/works">{t('notFound.action')}</Link>
        </Button>
      }
    />
  );
}

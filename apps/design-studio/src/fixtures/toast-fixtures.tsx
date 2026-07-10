import { useEffect } from 'react';

import { ToastProvider, Toaster, useToast } from '@42ch/nexus-ui';

/**
 * Studio Components fixture for the live Toast variant matrix.
 *
 * Uses promoted `@42ch/nexus-ui` Toast primitives (`ToastProvider`, `useToast`,
 * `Toaster`). Queues all four variants on mount so the fixture renders the
 * actual notification chrome.
 */
function ToastQueue() {
  const { toast } = useToast();

  useEffect(() => {
    toast({
      variant: 'success',
      title: 'Profile saved',
      description: 'Your workspace profile is ready.',
      testId: 'toast-variant-success',
      duration: 0,
    });
    toast({
      variant: 'error',
      title: 'Could not save profile',
      description: 'Check your connection and try again.',
      testId: 'toast-variant-error',
      duration: 0,
    });
    toast({
      variant: 'warning',
      title: 'Workspace path changed',
      description: 'Reload the app so the daemon uses the new path.',
      testId: 'toast-variant-warning',
      duration: 0,
    });
    toast({
      variant: 'info',
      title: 'Update available',
      testId: 'toast-variant-info',
      duration: 0,
    });
  }, [toast]);

  return null;
}

export function ToastFixtures() {
  return (
    <div data-testid="toast-matrix">
      <ToastProvider>
        <ToastQueue />
        <Toaster />
      </ToastProvider>
    </div>
  );
}

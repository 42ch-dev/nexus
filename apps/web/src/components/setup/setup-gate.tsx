import { Navigate } from 'react-router-dom';

import { useSetupCompleted } from '@/lib/setup-completed-context';
import type { ReactNode } from 'react';

interface SetupGateProps {
  children: ReactNode;
}

/**
 * Setup-marker routing gate (inner).
 *
 * Runs only after the outer {@link DaemonLaunchGate} has reached Ready.
 * - Incomplete setup → `/setup`
 * - Completed setup → main shell children
 *
 * Does not own daemon wait/splash (moved to DaemonLaunchGate in V1.105).
 */
export function SetupGate({ children }: SetupGateProps) {
  const { completed, isLoading } = useSetupCompleted();

  if (isLoading) {
    return null;
  }

  if (!completed) {
    return <Navigate to="/setup" replace />;
  }

  return <>{children}</>;
}

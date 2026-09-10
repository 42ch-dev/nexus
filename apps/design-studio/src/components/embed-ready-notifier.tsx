import { useEffect, type ReactNode } from 'react';
import { useLocation } from 'react-router';

import { useStudioEmbed } from '@/components/studio-embed-context';

/** Posts the embed-ready handshake after the matched gallery route commits. */
export function EmbeddedRouteReady({ children }: { children: ReactNode }) {
  const location = useLocation();
  const { isEmbedded, forcedTheme } = useStudioEmbed();

  useEffect(() => {
    if (!isEmbedded || forcedTheme === null) return;
    if (window.parent === window) return;

    window.parent.postMessage(
      {
        type: 'nexus-studio-embed-ready',
        theme: forcedTheme,
        path: location.pathname,
      },
      window.location.origin,
    );
  }, [isEmbedded, forcedTheme, location.pathname]);

  return <>{children}</>;
}

/** @deprecated Use EmbeddedRouteReady inside embedded lazy routes. */
export function EmbedReadyNotifier() {
  return <EmbeddedRouteReady>{null}</EmbeddedRouteReady>;
}

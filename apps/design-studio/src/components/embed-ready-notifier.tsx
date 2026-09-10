import { useEffect } from 'react';
import { useLocation } from 'react-router';

import { useStudioEmbed } from '@/components/studio-embed-context';

/** Posts the embed-ready handshake after route commit in iframe documents. */
export function EmbedReadyNotifier() {
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

  return null;
}

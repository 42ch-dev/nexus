import { createContext, useContext, type ReactNode } from 'react';

import type { EmbeddedTheme } from '@/lib/studio-embed';

type StudioEmbedContextValue = {
  isEmbedded: boolean;
  forcedTheme: EmbeddedTheme | null;
};

const StudioEmbedContext = createContext<StudioEmbedContextValue>({
  isEmbedded: false,
  forcedTheme: null,
});

export function StudioEmbedProvider({
  forcedTheme,
  children,
}: {
  forcedTheme: EmbeddedTheme | null;
  children: ReactNode;
}) {
  return (
    <StudioEmbedContext.Provider value={{ isEmbedded: forcedTheme !== null, forcedTheme }}>
      {children}
    </StudioEmbedContext.Provider>
  );
}

export function useStudioEmbed(): StudioEmbedContextValue {
  return useContext(StudioEmbedContext);
}

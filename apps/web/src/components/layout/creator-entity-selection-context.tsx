import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

export type CreatorEntityRef =
  | { kind: 'work'; id: string; label: string }
  | { kind: 'world'; id: string; label: string };

type CreatorEntitySelectionContextValue = {
  selectedEntity: CreatorEntityRef | null;
  setSelectedEntity: (entity: CreatorEntityRef | null) => void;
  clearSelectedEntity: () => void;
};

const CreatorEntitySelectionContext = createContext<CreatorEntitySelectionContextValue | null>(
  null,
);

/**
 * SSOT for Creator shell content region mode (V1.128 P2).
 *
 * `selectedEntity === null` → Create page; non-null → Controller stub.
 * Orthogonal to route params and shell-sidebar `submenuItem` anchor state.
 */
export function CreatorEntitySelectionProvider({ children }: { children: ReactNode }) {
  const [selectedEntity, setSelectedEntity] = useState<CreatorEntityRef | null>(null);

  const clearSelectedEntity = useCallback(() => {
    setSelectedEntity(null);
  }, []);

  const value = useMemo(
    () => ({
      selectedEntity,
      setSelectedEntity,
      clearSelectedEntity,
    }),
    [selectedEntity, clearSelectedEntity],
  );

  return (
    <CreatorEntitySelectionContext.Provider value={value}>
      {children}
    </CreatorEntitySelectionContext.Provider>
  );
}

export function useCreatorEntitySelection(): CreatorEntitySelectionContextValue {
  const ctx = useContext(CreatorEntitySelectionContext);
  if (!ctx) {
    throw new Error('useCreatorEntitySelection must be used within CreatorEntitySelectionProvider');
  }
  return ctx;
}

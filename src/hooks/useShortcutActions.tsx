import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  type ReactNode,
} from "react";

export interface ShortcutHandlers {
  refresh?: () => void;
  promote?: () => void;
  selectAll?: () => void;
  closeOverlay?: () => void;
}

interface ShortcutActionsContextValue {
  register: (handlers: ShortcutHandlers) => void;
  unregister: () => void;
  invoke: <K extends keyof ShortcutHandlers>(key: K) => void;
}

const ShortcutActionsContext = createContext<ShortcutActionsContextValue | null>(
  null,
);

export function ShortcutActionsProvider({ children }: { children: ReactNode }) {
  const handlersRef = useRef<ShortcutHandlers>({});

  const register = useCallback((handlers: ShortcutHandlers) => {
    handlersRef.current = handlers;
  }, []);

  const unregister = useCallback(() => {
    handlersRef.current = {};
  }, []);

  const invoke = useCallback(<K extends keyof ShortcutHandlers>(key: K) => {
    handlersRef.current[key]?.();
  }, []);

  const value = useMemo(
    () => ({ register, unregister, invoke }),
    [register, unregister, invoke],
  );

  return (
    <ShortcutActionsContext.Provider value={value}>
      {children}
    </ShortcutActionsContext.Provider>
  );
}

export function useShortcutActions() {
  const ctx = useContext(ShortcutActionsContext);
  if (!ctx) {
    throw new Error("useShortcutActions must be used within ShortcutActionsProvider");
  }
  return ctx;
}

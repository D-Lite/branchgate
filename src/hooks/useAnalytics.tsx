import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  initAnalytics,
  isAnalyticsAvailable,
  setAnalyticsOptIn,
} from "../lib/analytics";
import { getAppSettings, updateAppSettings } from "../lib/tauri";

interface AnalyticsContextValue {
  shareUsage: boolean;
  setShareUsage: (enabled: boolean) => Promise<void>;
}

const AnalyticsContext = createContext<AnalyticsContextValue | null>(null);

export function AnalyticsProvider({ children }: { children: ReactNode }) {
  const [shareUsage, setShareUsageState] = useState(true);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        let settings = await getAppSettings();

        if (!settings.analyticsConsentDecided) {
          settings = await updateAppSettings({
            analyticsConsentDecided: true,
            shareAnonymousUsage: isAnalyticsAvailable(),
          });
        }

        if (!cancelled) {
          setShareUsageState(settings.shareAnonymousUsage);
        }

        await initAnalytics();
      } catch (err) {
        console.error(err);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  const setShareUsage = useCallback(async (enabled: boolean) => {
    const next = await updateAppSettings({
      shareAnonymousUsage: enabled,
      analyticsConsentDecided: true,
    });
    setShareUsageState(next.shareAnonymousUsage);
    await setAnalyticsOptIn(enabled);
  }, []);

  const value = useMemo(
    () => ({ shareUsage, setShareUsage }),
    [shareUsage, setShareUsage],
  );

  return (
    <AnalyticsContext.Provider value={value}>{children}</AnalyticsContext.Provider>
  );
}

export function useAnalytics() {
  const ctx = useContext(AnalyticsContext);
  if (!ctx) {
    throw new Error("useAnalytics must be used within AnalyticsProvider");
  }
  return ctx;
}

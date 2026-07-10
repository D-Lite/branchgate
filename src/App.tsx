import { useCallback, useEffect, useState } from "react";
import { BrowserRouter, Navigate, Route, Routes, useLocation } from "react-router-dom";
import { AppShell } from "./components/layout/AppShell";
import { ThemeProvider } from "./hooks/useTheme";
import { AnalyticsProvider } from "./hooks/useAnalytics";
import {
  getAppInfo,
  listPipelines,
  platformClass,
  type AppInfo,
  type Pipeline,
} from "./lib/tauri";
import { ConnectLocal } from "./pages/ConnectLocal";
import { Dashboard } from "./pages/Dashboard";
import { HistoryPage } from "./pages/History";
import { Onboarding } from "./pages/Onboarding";
import { PipelinePage } from "./pages/Pipeline";
import { SettingsPage } from "./pages/Settings";
import "./pages/ConnectLocal.css";
import "./pages/Dashboard.css";
import "./pages/Onboarding.css";
import "./pages/Pipeline.css";
import "./pages/Settings.css";
import "./pages/History.css";

function markOnboarded() {
  localStorage.setItem("branchgate-onboarded", "1");
}

function isOnboarded() {
  return localStorage.getItem("branchgate-onboarded") === "1";
}

function AppRoutes() {
  const location = useLocation();
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [pipelines, setPipelines] = useState<Pipeline[]>([]);
  const [onboarded, setOnboarded] = useState(isOnboarded);

  const refreshPipelines = useCallback(async () => {
    try {
      const rows = await listPipelines();
      setPipelines(rows);
    } catch {
      setPipelines([]);
    }
  }, []);

  useEffect(() => {
    getAppInfo().then(setAppInfo).catch(console.error);
    refreshPipelines();
  }, [refreshPipelines]);

  useEffect(() => {
    if (!appInfo) return;
    document.documentElement.classList.add(platformClass(appInfo.platform));
  }, [appInfo]);

  const handleConnectSuccess = useCallback(() => {
    markOnboarded();
    setOnboarded(true);
    refreshPipelines();
  }, [refreshPipelines]);

  const handleSkipOnboarding = useCallback(() => {
    markOnboarded();
    setOnboarded(true);
  }, []);

  const content = (() => {
    if (location.pathname === "/connect") {
      return <ConnectLocal onSuccess={handleConnectSuccess} />;
    }

    if (!onboarded) {
      return <Onboarding onSkip={handleSkipOnboarding} />;
    }

    return (
      <Routes>
        <Route
          element={
            <AppShell
              platform={appInfo?.platform ?? "unknown"}
              pipelines={pipelines.map((p) => ({
                id: p.id,
                name: p.name,
                pendingCount: p.pendingCount,
              }))}
            />
          }
        >
          <Route index element={<Dashboard pipelines={pipelines} />} />
          <Route
            path="pipeline/:id"
            element={
              <PipelinePage
                pipelines={pipelines}
                onPipelinesChange={refreshPipelines}
              />
            }
          />
          <Route path="history" element={<HistoryPage />} />
          <Route
            path="settings"
            element={
              <SettingsPage
                appInfo={appInfo}
                onDataChange={refreshPipelines}
                onReset={() => {
                  localStorage.removeItem("branchgate-onboarded");
                  setOnboarded(false);
                  setPipelines([]);
                }}
              />
            }
          />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    );
  })();

  return <AnalyticsProvider>{content}</AnalyticsProvider>;
}

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <AppRoutes />
      </BrowserRouter>
    </ThemeProvider>
  );
}

import { useState } from "react";
import { NavLink, Outlet } from "react-router-dom";
import { BrandLockup } from "../brand/BrandLockup";
import { CommandPalette } from "../CommandPalette";
import { ShortcutsHelp } from "../ShortcutsHelp";
import { useGlobalShortcuts } from "../../hooks/useGlobalShortcuts";
import {
  ShortcutActionsProvider,
  useShortcutActions,
} from "../../hooks/useShortcutActions";
import { modLabel } from "../../lib/keyboard";
import "../CommandPalette.css";
import "../ShortcutsHelp.css";
import "./AppShell.css";

interface SidebarPipeline {
  id: number;
  name: string;
  pendingCount: number;
}

interface AppShellProps {
  platform: string;
  pipelines: SidebarPipeline[];
}

export function AppShell({ platform, pipelines }: AppShellProps) {
  return (
    <ShortcutActionsProvider>
      <AppShellInner platform={platform} pipelines={pipelines} />
    </ShortcutActionsProvider>
  );
}

function AppShellInner({ platform, pipelines }: AppShellProps) {
  const { invoke } = useShortcutActions();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);

  useGlobalShortcuts({
    pipelines,
    paletteOpen,
    helpOpen,
    setPaletteOpen,
    setHelpOpen,
    invoke,
  });

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="sidebar-brand">
          <BrandLockup size={18} showVersion />
        </div>

        <nav className="sidebar-nav">
          <NavLink
            to="/"
            end
            className={({ isActive }) =>
              `nav-item${isActive ? " nav-item-active" : ""}`
            }
          >
            <GridIcon />
            <span>Pipelines</span>
            <kbd className="nav-kbd mono">{modLabel()}1</kbd>
          </NavLink>
          <NavLink
            to="/history"
            className={({ isActive }) =>
              `nav-item${isActive ? " nav-item-active" : ""}`
            }
          >
            <ClockIcon />
            <span>History</span>
            <kbd className="nav-kbd mono">{modLabel()}2</kbd>
          </NavLink>
          <NavLink
            to="/settings"
            className={({ isActive }) =>
              `nav-item${isActive ? " nav-item-active" : ""}`
            }
          >
            <SettingsIcon />
            <span>Settings</span>
            <kbd className="nav-kbd mono">{modLabel()},</kbd>
          </NavLink>
        </nav>

        {pipelines.length > 0 && (
          <div className="sidebar-pipelines">
            <div className="sidebar-section-label">
              Pipelines
              <kbd className="nav-kbd mono">{modLabel()}P</kbd>
            </div>
            {pipelines.map((p) => (
              <NavLink
                key={p.id}
                to={`/pipeline/${p.id}`}
                className={({ isActive }) =>
                  `pipeline-item${isActive ? " pipeline-item-active" : ""}`
                }
              >
                <span className="pipeline-dot" />
                <span className="pipeline-name">{p.name}</span>
                <span className="pipeline-count mono">{p.pendingCount}</span>
              </NavLink>
            ))}
          </div>
        )}

        <div className="sidebar-footer">
          <div className="account-row">
            <span className="status-dot" />
            <span className="account-label mono">
              {platform === "macos" ? "macOS" : platform} · local
            </span>
          </div>
          <button
            type="button"
            className="shortcuts-hint-btn mono"
            onClick={() => setHelpOpen(true)}
          >
            ? shortcuts
          </button>
        </div>
      </aside>

      <main className="main-panel">
        <Outlet />
      </main>

      <CommandPalette
        open={paletteOpen}
        pipelines={pipelines}
        onClose={() => setPaletteOpen(false)}
      />
      <ShortcutsHelp open={helpOpen} onClose={() => setHelpOpen(false)} />
    </div>
  );
}

function GridIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <rect x="3" y="3" width="7" height="7" />
      <rect x="14" y="3" width="7" height="7" />
      <rect x="3" y="14" width="7" height="7" />
      <rect x="14" y="14" width="7" height="7" />
    </svg>
  );
}

function ClockIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M19.1 4.9L17 7M7 17l-2.1 2.1" />
    </svg>
  );
}

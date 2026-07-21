import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { useAnalytics } from "../hooks/useAnalytics";
import { isAnalyticsAvailable } from "../lib/analytics";
import { useTheme } from "../hooks/useTheme";
import { PALETTE_OPTIONS } from "../lib/themes";
import {
  clearSyncCache,
  detectEditors,
  disconnectRepo,
  getAppSettings,
  getDiagnostics,
  listConnectedRepos,
  listEditors,
  MERGE_STRATEGIES,
  resetAppData,
  setPreferredEditor,
  updateAppSettings,
  updateRepoSettings,
  type AppInfo,
  type AppSettings,
  type ConnectedRepo,
  type DiagnosticsInfo,
  type EditorInfo,
  type MergeStrategy,
} from "../lib/tauri";

interface SettingsPageProps {
  appInfo: AppInfo | null;
  onDataChange: () => void;
  onReset: () => void;
}

const TABS = [
  "Accounts",
  "Repos",
  "Merge behavior",
  "Appearance",
  "Editors",
  "Notifications",
  "CLI",
  "Data & privacy",
  "Diagnostics",
] as const;

type TabId = (typeof TABS)[number];

export function SettingsPage({ appInfo, onDataChange, onReset }: SettingsPageProps) {
  const [activeTab, setActiveTab] = useState<TabId>("Appearance");
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [repos, setRepos] = useState<ConnectedRepo[]>([]);
  const [editors, setEditors] = useState<EditorInfo[]>([]);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [appSettings, repoRows, editorRows, diag] = await Promise.all([
        getAppSettings(),
        listConnectedRepos(),
        listEditors(),
        getDiagnostics(),
      ]);
      setSettings(appSettings);
      setRepos(repoRows);
      setEditors(editorRows);
      setDiagnostics(diag);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const flash = (text: string) => {
    setMessage(text);
    window.setTimeout(() => setMessage(null), 3000);
  };

  const runAction = async (action: () => Promise<void>, success: string) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      flash(success);
      await load();
      onDataChange();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <header className="page-header">
        <h1>Settings</h1>
        <p>Accounts, working copies, editors, and app preferences.</p>
      </header>
      <div className="page-body settings-layout">
        <aside className="settings-tabs">
          {TABS.map((tab) => (
            <button
              key={tab}
              type="button"
              className={`settings-tab${tab === activeTab ? " settings-tab-active" : ""}`}
              onClick={() => setActiveTab(tab)}
            >
              {tab}
            </button>
          ))}
        </aside>

        <section className="settings-panel card">
          {loading ? (
            <p className="settings-desc">Loading settings…</p>
          ) : (
            <>
              {message && (
                <div className="settings-flash" role="status" aria-live="polite">
                  {message}
                </div>
              )}
              {error && (
                <div className="settings-error" role="alert">
                  {error}
                </div>
              )}

              {activeTab === "Accounts" && <AccountsTab />}
              {activeTab === "Repos" && (
                <ReposTab
                  repos={repos}
                  busy={busy}
                  onRepoUpdate={async (repoId, strategy) => {
                    await runAction(
                      () => updateRepoSettings({ repoId, defaultMergeStrategy: strategy }),
                      "Repository settings saved",
                    );
                  }}
                  onGitRuntimeUpdate={async (repoId, gitBackend, wslDistro) => {
                    await runAction(
                      () => updateRepoSettings({ repoId, gitBackend, wslDistro }),
                      "Git runtime updated",
                    );
                  }}
                  onDisconnect={async (repoId) => {
                    if (!window.confirm("Disconnect this repository and remove its pipelines?")) {
                      return;
                    }
                    await runAction(
                      () => disconnectRepo(repoId),
                      "Repository disconnected",
                    );
                  }}
                />
              )}
              {activeTab === "Merge behavior" && settings && (
                <MergeBehaviorTab
                  settings={settings}
                  busy={busy}
                  onChange={async (strategy) => {
                    const next = await updateAppSettings({ defaultMergeStrategy: strategy });
                    setSettings(next);
                    flash("Default merge strategy updated");
                  }}
                />
              )}
              {activeTab === "Appearance" && <AppearanceTab />}
              {activeTab === "Editors" && (
                <EditorsTab
                  editors={editors}
                  busy={busy}
                  onDetect={async () => {
                    setBusy(true);
                    setError(null);
                    try {
                      const rows = await detectEditors();
                      setEditors(rows);
                      flash("Editors re-detected");
                    } catch (err) {
                      setError(err instanceof Error ? err.message : String(err));
                    } finally {
                      setBusy(false);
                    }
                  }}
                  onPrefer={async (editorId) => {
                    setBusy(true);
                    setError(null);
                    try {
                      const rows = await setPreferredEditor(editorId);
                      setEditors(rows);
                      flash("Preferred editor updated");
                    } catch (err) {
                      setError(err instanceof Error ? err.message : String(err));
                    } finally {
                      setBusy(false);
                    }
                  }}
                />
              )}
              {activeTab === "Notifications" && settings && (
                <NotificationsTab
                  settings={settings}
                  busy={busy}
                  onChange={async (patch) => {
                    const next = await updateAppSettings(patch);
                    setSettings(next);
                    flash("Notification preferences saved");
                  }}
                />
              )}
              {activeTab === "CLI" && <CliTab />}
              {activeTab === "Data & privacy" && (
                <DataPrivacyTab
                  settings={settings}
                  busy={busy}
                  onClonesRootChange={async (path) => {
                    const next = await updateAppSettings({ managedClonesRoot: path });
                    setSettings(next);
                    flash("Managed clones folder updated");
                  }}
                  onClearCache={async () => {
                    if (!window.confirm("Clear sync cache? Pipelines will re-scan on next refresh.")) {
                      return;
                    }
                    await runAction(() => clearSyncCache(), "Sync cache cleared");
                  }}
                  onReset={async () => {
                    if (
                      !window.confirm(
                        "Reset all Branchgate data? Repositories, pipelines, and promotion history will be removed.",
                      )
                    ) {
                      return;
                    }
                    setBusy(true);
                    setError(null);
                    try {
                      await resetAppData();
                      onReset();
                    } catch (err) {
                      setError(err instanceof Error ? err.message : String(err));
                      setBusy(false);
                    }
                  }}
                />
              )}
              {activeTab === "Diagnostics" && (
                <DiagnosticsTab
                  appInfo={appInfo}
                  diagnostics={diagnostics}
                  busy={busy}
                  onRefresh={load}
                />
              )}
            </>
          )}
        </section>
      </div>
    </>
  );
}

function AccountsTab() {
  return (
    <>
      <h2>Accounts</h2>
      <p className="settings-desc">
        Branchgate is running in local mode — promotions run directly against repos on your machine.
      </p>

      <div className="settings-card">
        <div className="settings-card-head">
          <span className="settings-card-title">Local mode</span>
          <span className="status-chip promoted mono">active</span>
        </div>
        <p className="settings-card-body">
          Connect repositories from the dashboard or{" "}
          <Link to="/connect">connect wizard</Link>. No cloud account required.
        </p>
      </div>

      <div className="settings-card settings-card-muted">
        <div className="settings-card-head">
          <span className="settings-card-title">GitHub</span>
          <span className="status-chip mono">coming soon</span>
        </div>
        <p className="settings-card-body">
          GitHub OAuth, remote sync, and automatic PR creation will ship in a future release.
          Tokens will be stored in your OS keychain.
        </p>
        <button type="button" className="btn btn-subtle" disabled>
          Connect GitHub account
        </button>
      </div>
    </>
  );
}

function ReposTab({
  repos,
  busy,
  onRepoUpdate,
  onGitRuntimeUpdate,
  onDisconnect,
}: {
  repos: ConnectedRepo[];
  busy: boolean;
  onRepoUpdate: (repoId: number, strategy: MergeStrategy) => Promise<void>;
  onGitRuntimeUpdate: (
    repoId: number,
    gitBackend: "auto" | "native" | "wsl",
    wslDistro: string | null,
  ) => Promise<void>;
  onDisconnect: (repoId: number) => Promise<void>;
}) {
  return (
    <>
      <h2>Repositories</h2>
      <p className="settings-desc">
        Connected working copies and per-repo promotion defaults.
      </p>

      {repos.length === 0 ? (
        <div className="settings-empty">
          <p>No repositories connected yet.</p>
          <Link to="/connect" className="btn btn-primary">
            Connect a repo
          </Link>
        </div>
      ) : (
        <ul className="settings-repo-list">
          {repos.map((repo) => (
            <li key={repo.id} className="settings-repo-item">
              <div className="settings-repo-main">
                <span className="settings-repo-name">{repo.name ?? "Unnamed repo"}</span>
                <span className="settings-repo-meta mono">
                  {repo.kind} · {repo.pipelineCount} pipeline{repo.pipelineCount === 1 ? "" : "s"}
                </span>
                {repo.localPath && (
                  <span className="settings-repo-path mono">{repo.localPath}</span>
                )}
              </div>

              <div className="settings-repo-controls">
                <label className="settings-field">
                  <span>Working copy</span>
                  <select value={repo.workingCopyMode} disabled>
                    <option value="existing_local">Existing local clone</option>
                    <option value="managed">Managed clone (soon)</option>
                  </select>
                </label>

                <label className="settings-field">
                  <span>Merge assumption</span>
                  <select
                    value={repo.defaultMergeStrategy}
                    disabled={busy}
                    onChange={(e) =>
                      onRepoUpdate(repo.id, e.target.value as MergeStrategy)
                    }
                  >
                    {MERGE_STRATEGIES.map((s) => (
                      <option key={s.value} value={s.value}>
                        {s.label}
                      </option>
                    ))}
                  </select>
                </label>

                <label className="settings-field">
                  <span>Git runtime</span>
                  <select
                    value={repo.gitBackend}
                    disabled={busy}
                    onChange={(event) => {
                      const backend = event.target.value as "auto" | "native" | "wsl";
                      void onGitRuntimeUpdate(
                        repo.id,
                        backend,
                        backend === "wsl" ? (repo.wslDistro ?? "Ubuntu") : null,
                      );
                    }}
                  >
                    <option value="auto">Auto-detect</option>
                    <option value="native">Native Git</option>
                    <option value="wsl">Git inside WSL</option>
                  </select>
                </label>

                {repo.gitBackend === "wsl" && (
                  <label className="settings-field">
                    <span>WSL distribution</span>
                    <input
                      type="text"
                      defaultValue={repo.wslDistro ?? ""}
                      placeholder="Ubuntu"
                      disabled={busy}
                      onBlur={(event) => {
                        const distro = event.target.value.trim();
                        if (distro && distro !== repo.wslDistro) {
                          void onGitRuntimeUpdate(repo.id, "wsl", distro);
                        }
                      }}
                    />
                  </label>
                )}

                <button
                  type="button"
                  className="btn btn-subtle settings-danger-btn"
                  disabled={busy}
                  onClick={() => onDisconnect(repo.id)}
                >
                  Disconnect
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function MergeBehaviorTab({
  settings,
  busy,
  onChange,
}: {
  settings: AppSettings;
  busy: boolean;
  onChange: (strategy: MergeStrategy) => Promise<void>;
}) {
  return (
    <>
      <h2>Merge behavior</h2>
      <p className="settings-desc">
        Default assumption for how changes were merged on the source branch. Per-repo overrides
        are available under Repos.
      </p>

      <div className="settings-option-list">
        {MERGE_STRATEGIES.map((strategy) => (
          <label key={strategy.value} className="settings-radio-row">
            <input
              type="radio"
              name="default-merge"
              checked={settings.defaultMergeStrategy === strategy.value}
              disabled={busy}
              onChange={() => onChange(strategy.value)}
            />
            <span>
              <strong>{strategy.label}</strong>
              <span className="settings-radio-hint">{strategy.hint}</span>
            </span>
          </label>
        ))}
      </div>
    </>
  );
}

function AppearanceTab() {
  const { mode, palette, setMode, setPalette } = useTheme();

  return (
    <>
      <h2>Appearance</h2>
      <p className="settings-desc">Color scheme and light/dark preference.</p>

      <h3 className="settings-subhead">Color scheme</h3>
      <div className="palette-grid">
        {PALETTE_OPTIONS.map((option) => (
          <button
            key={option.id}
            type="button"
            className={`palette-tile${palette === option.id ? " palette-tile-active" : ""}`}
            onClick={() => setPalette(option.id)}
          >
            <span className="palette-preview" aria-hidden>
              <span
                className="palette-swatch"
                style={{ background: option.preview.bg }}
              />
              <span
                className="palette-swatch"
                style={{ background: option.preview.surface }}
              />
              <span
                className="palette-swatch palette-swatch-accent"
                style={{ background: option.preview.accent }}
              />
            </span>
            <span className="palette-tile-label">{option.label}</span>
            <span className="palette-tile-desc">{option.description}</span>
          </button>
        ))}
      </div>

      <h3 className="settings-subhead">Mode</h3>
      <div className="theme-tiles">
        {(["light", "dark", "system"] as const).map((m) => (
          <button
            key={m}
            type="button"
            className={`theme-tile${mode === m ? " theme-tile-active" : ""}`}
            onClick={() => setMode(m)}
          >
            <span className="theme-tile-label">
              {m === "system" ? "System" : m[0].toUpperCase() + m.slice(1)}
            </span>
          </button>
        ))}
      </div>
    </>
  );
}

function EditorsTab({
  editors,
  busy,
  onDetect,
  onPrefer,
}: {
  editors: EditorInfo[];
  busy: boolean;
  onDetect: () => Promise<void>;
  onPrefer: (editorId: number) => Promise<void>;
}) {
  const [detecting, setDetecting] = useState(false);
  const [pendingEditorId, setPendingEditorId] = useState<number | null>(null);
  const editorBusy = busy || detecting || pendingEditorId !== null;

  const detect = async () => {
    if (editorBusy) return;
    setDetecting(true);
    try {
      await onDetect();
    } finally {
      setDetecting(false);
    }
  };

  const prefer = async (editorId: number) => {
    if (editorBusy) return;
    setPendingEditorId(editorId);
    try {
      await onPrefer(editorId);
    } finally {
      setPendingEditorId(null);
    }
  };

  return (
    <div aria-busy={editorBusy}>
      <h2>Editors</h2>
      <p className="settings-desc">
        Detected editors are used to open conflicted files during promotion runs.
      </p>

      <div className="settings-actions-row">
        <button type="button" className="btn btn-ghost" disabled={editorBusy} onClick={detect}>
          {detecting ? "Detecting editors…" : "Re-detect editors"}
        </button>
      </div>

      <p className="settings-live-region" role="status" aria-live="polite" aria-atomic="true">
        {detecting
          ? "Detecting installed editors"
          : pendingEditorId !== null
            ? "Updating preferred editor"
            : ""}
      </p>

      {editors.length === 0 ? (
        <div className="settings-empty">
          <p>No editors detected yet.</p>
          <button type="button" className="btn btn-primary" disabled={editorBusy} onClick={detect}>
            {detecting ? "Detecting editors…" : "Detect editors"}
          </button>
        </div>
      ) : (
        <ul className="settings-editor-list" aria-busy={editorBusy}>
          {editors.map((editor) => (
            <li key={editor.id} className="settings-editor-item">
              <label className="settings-editor-pick">
                <input
                  type="radio"
                  name="preferred-editor"
                  checked={editor.isPreferred}
                  aria-disabled={editorBusy}
                  onChange={() => prefer(editor.id)}
                />
                <span>
                  <strong>{editor.name}</strong>
                  <span className="settings-editor-meta mono">
                    {editor.command}
                    {editor.detectedPath ? ` · ${editor.detectedPath}` : ""}
                  </span>
                </span>
              </label>
              {editor.isPreferred && <span className="status-chip promoted mono">preferred</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function NotificationsTab({
  settings,
  busy,
  onChange,
}: {
  settings: AppSettings;
  busy: boolean;
  onChange: (patch: Partial<AppSettings>) => Promise<void>;
}) {
  return (
    <>
      <h2>Notifications</h2>
      <p className="settings-desc">
        In-app alerts during promotion runs. System notifications will follow in a later release.
      </p>

      <div className="settings-toggle-list">
        <label className="settings-toggle-row">
          <span>
            <strong>Conflict during promotion</strong>
            <span className="settings-radio-hint">Alert when a cherry-pick or merge stops on a conflict</span>
          </span>
          <input
            type="checkbox"
            checked={settings.notifyOnConflict}
            disabled={busy}
            onChange={(e) => onChange({ notifyOnConflict: e.target.checked })}
          />
        </label>

        <label className="settings-toggle-row">
          <span>
            <strong>Promotion complete</strong>
            <span className="settings-radio-hint">Alert when changes land on the target branch</span>
          </span>
          <input
            type="checkbox"
            checked={settings.notifyOnComplete}
            disabled={busy}
            onChange={(e) => onChange({ notifyOnComplete: e.target.checked })}
          />
        </label>
      </div>
    </>
  );
}

function CliTab() {
  return (
    <>
      <h2>CLI</h2>
      <p className="settings-desc">
        Run promotions and sync pipelines from your terminal — useful for CI and scripting.
      </p>

      <div className="settings-card settings-card-muted">
        <div className="settings-card-head">
          <span className="settings-card-title">Branchgate CLI</span>
          <span className="status-chip mono">coming soon</span>
        </div>
        <p className="settings-card-body">
          A headless <span className="mono">branchgate</span> command for listing pending changes,
          running promotions, and checking run status without opening the desktop app.
        </p>
        <pre className="settings-cli-preview mono">
{`branchgate sync --pipeline <id>
branchgate promote --pipeline <id> --select all
branchgate history --pipeline <id>`}
        </pre>
        <button type="button" className="btn btn-subtle" disabled>
          Install CLI
        </button>
      </div>
    </>
  );
}

function DataPrivacyTab({
  settings,
  busy,
  onClonesRootChange,
  onClearCache,
  onReset,
}: {
  settings: AppSettings | null;
  busy: boolean;
  onClonesRootChange: (path: string | null) => Promise<void>;
  onClearCache: () => Promise<void>;
  onReset: () => Promise<void>;
}) {
  const pickClonesFolder = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose managed clones folder",
    });
    if (typeof selected === "string") {
      await onClonesRootChange(selected);
    }
  };

  return (
    <>
      <h2>Data &amp; privacy</h2>
      <p className="settings-desc">
        Branchgate stores pipeline state locally in SQLite on your machine. Git credentials are
        never written to the database.
      </p>

      <div className="settings-card">
        <div className="settings-card-head">
          <span className="settings-card-title">Managed clones folder</span>
        </div>
        <p className="settings-card-body">
          Reserved for a future managed-clone mode. Local repos you connect today are left untouched.
        </p>
        <div className="settings-path-row">
          <code className="settings-path mono">
            {settings?.managedClonesRoot ?? "Not set — defaults to app data folder"}
          </code>
          <button type="button" className="btn btn-ghost" disabled={busy} onClick={pickClonesFolder}>
            Choose folder
          </button>
        </div>
      </div>

      <div className="settings-card">
        <div className="settings-card-head">
          <span className="settings-card-title">Clear sync cache</span>
        </div>
        <p className="settings-card-body">
          Forces pipelines to re-scan branch heads on the next refresh. Promotion history is kept.
        </p>
        <button type="button" className="btn btn-subtle" disabled={busy} onClick={onClearCache}>
          Clear sync cache
        </button>
      </div>

      <div className="settings-card settings-card-danger">
        <div className="settings-card-head">
          <span className="settings-card-title">Reset app data</span>
        </div>
        <p className="settings-card-body">
          Removes all repositories, pipelines, and promotion history. Editor preferences and app
          settings are kept.
        </p>
        <button type="button" className="btn btn-subtle settings-danger-btn" disabled={busy} onClick={onReset}>
          Reset all data
        </button>
      </div>
    </>
  );
}

function DiagnosticsTab({
  appInfo,
  diagnostics,
  busy,
  onRefresh,
}: {
  appInfo: AppInfo | null;
  diagnostics: DiagnosticsInfo | null;
  busy: boolean;
  onRefresh: () => Promise<void>;
}) {
  const { shareUsage, setShareUsage } = useAnalytics();
  const analyticsReady = isAnalyticsAvailable();

  return (
    <>
      <h2>Diagnostics</h2>
      <p className="settings-desc">Runtime information and optional anonymous usage reporting.</p>

      <div className="settings-card">
        <div className="settings-card-head">
          <span className="settings-card-title">Anonymous usage data</span>
          <span className="status-chip mono">
            {analyticsReady ? (shareUsage ? "on" : "off") : "not configured"}
          </span>
        </div>
        <p className="settings-card-body">
          On by default. Branchgate sends anonymous events like app launches, promotion
          completions, and conflicts. Never includes repo names, PR titles, or file paths.
        </p>
        {analyticsReady ? (
          <label className="settings-toggle-row">
            <span>
              <strong>Share anonymous usage data</strong>
              <span className="settings-radio-hint">
                Turn off here if you prefer not to share usage data
              </span>
            </span>
            <input
              type="checkbox"
              checked={shareUsage}
              disabled={busy}
              onChange={(e) => setShareUsage(e.target.checked)}
            />
          </label>
        ) : (
          <p className="settings-card-body mono">
            Set <code>VITE_POSTHOG_KEY</code> in <code>.env</code> at build time to enable.
          </p>
        )}
      </div>

      <div className="settings-actions-row">
        <button type="button" className="btn btn-ghost" disabled={busy} onClick={onRefresh}>
          Refresh
        </button>
      </div>

      <dl className="diag-list mono">
        <div>
          <dt>Version</dt>
          <dd>{appInfo?.version ?? "—"}</dd>
        </div>
        <div>
          <dt>Platform</dt>
          <dd>{appInfo?.platform ?? "—"}</dd>
        </div>
        <div>
          <dt>Mode</dt>
          <dd>{diagnostics?.mode ?? "—"}</dd>
        </div>
        <div>
          <dt>Database</dt>
          <dd>{appInfo?.dbReady ? "ready" : "not ready"}</dd>
        </div>
        <div>
          <dt>Database path</dt>
          <dd className="diag-path">{diagnostics?.dbPath ?? "—"}</dd>
        </div>
        <div>
          <dt>Repositories</dt>
          <dd>{diagnostics?.repoCount ?? "—"}</dd>
        </div>
        <div>
          <dt>Pipelines</dt>
          <dd>{diagnostics?.pipelineCount ?? "—"}</dd>
        </div>
        <div>
          <dt>Editors</dt>
          <dd>{diagnostics?.editorCount ?? "—"}</dd>
        </div>
        <div>
          <dt>GitHub</dt>
          <dd>{diagnostics?.githubConnected ? "connected" : "not connected"}</dd>
        </div>
        <div>
          <dt>API rate limit</dt>
          <dd>n/a (local mode)</dd>
        </div>
      </dl>
    </>
  );
}

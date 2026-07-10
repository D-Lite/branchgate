import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import {
  connectLocalRepo,
  defaultPipelineName,
  inspectLocalRepo,
  listConnectedRepos,
  suggestBranches,
  type ConnectedRepo,
  type LocalRepoInfo,
} from "../lib/tauri";
import { track } from "../lib/analytics";
import { BrandLockup } from "../components/brand/BrandLockup";

interface ConnectLocalProps {
  onSuccess: () => void;
}

type Step = "pick" | "configure";

export function ConnectLocal({ onSuccess }: ConnectLocalProps) {
  const navigate = useNavigate();
  const [step, setStep] = useState<Step>("pick");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [existingRepos, setExistingRepos] = useState<ConnectedRepo[]>([]);
  const [repo, setRepo] = useState<LocalRepoInfo | null>(null);
  const [selectedRepoId, setSelectedRepoId] = useState<number | null>(null);
  const [sourceBranch, setSourceBranch] = useState("");
  const [targetBranch, setTargetBranch] = useState("");
  const [pipelineName, setPipelineName] = useState("");

  const stepLabel = step === "pick" ? "Step 1 of 2" : "Step 2 of 2";
  const branchOptions = useMemo(() => repo?.branches ?? [], [repo]);

  useEffect(() => {
    listConnectedRepos()
      .then(setExistingRepos)
      .catch(() => setExistingRepos([]));
  }, []);

  const loadRepo = useCallback(async (path: string, repoId?: number | null) => {
    const info = await inspectLocalRepo(path);
    const suggested = suggestBranches(info.branches);
    setRepo(info);
    setSelectedRepoId(repoId ?? info.existingRepoId);
    setSourceBranch(suggested.source);
    setTargetBranch(suggested.target);
    setPipelineName(
      defaultPipelineName(info.name, suggested.source, suggested.target),
    );
    setStep("configure");
  }, []);

  const pickFolder = useCallback(async () => {
    setError(null);
    setLoading(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Choose a local git repository",
      });
      if (!selected || Array.isArray(selected)) return;
      await loadRepo(selected);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [loadRepo]);

  const useExistingRepo = useCallback(
    async (connected: ConnectedRepo) => {
      if (!connected.localPath) return;
      setError(null);
      setLoading(true);
      try {
        await loadRepo(connected.localPath, connected.id);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
      }
    },
    [loadRepo],
  );

  const submit = useCallback(async () => {
    if (!repo) return;
    setError(null);
    setLoading(true);
    try {
      const pipeline = await connectLocalRepo({
        localPath: repo.path,
        pipelineName: pipelineName.trim(),
        sourceBranch,
        targetBranch,
        repoId: selectedRepoId,
      });
      track.pipelineCreated();
      onSuccess();
      navigate(`/pipeline/${pipeline.id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [
    repo,
    pipelineName,
    sourceBranch,
    targetBranch,
    selectedRepoId,
    onSuccess,
    navigate,
  ]);

  return (
    <div className="connect">
      <div className="connect-inner">
        <header className="connect-header">
          <button
            type="button"
            className="btn btn-subtle connect-back"
            onClick={() => navigate(-1)}
          >
            ← Back
          </button>
          <div className="connect-brand">
            <BrandLockup size={18} />
            <div>
              <h1>Connect local repo</h1>
              <p className="mono connect-step">{stepLabel}</p>
            </div>
          </div>
        </header>

        {error && <div className="connect-error">{error}</div>}

        {step === "pick" && (
          <>
            {existingRepos.length > 0 && (
              <section className="connect-panel card">
                <h2>Connected repositories</h2>
                <p>Add another pipeline to a repo you&apos;ve already connected.</p>
                <div className="existing-repos">
                  {existingRepos.map((r) => (
                    <button
                      key={r.id}
                      type="button"
                      className="existing-repo-row"
                      onClick={() => useExistingRepo(r)}
                      disabled={loading}
                    >
                      <span className="existing-repo-name">{r.name ?? "local repo"}</span>
                      <span className="existing-repo-meta mono">
                        {r.pipelineCount} pipeline{r.pipelineCount === 1 ? "" : "s"}
                      </span>
                    </button>
                  ))}
                </div>
              </section>
            )}

            <section className="connect-panel card">
              <h2>{existingRepos.length > 0 ? "Or choose a new folder" : "Choose repository folder"}</h2>
              <p>
                Point Branchgate at an existing git checkout. The folder must
                contain a <span className="mono">.git</span> directory.
              </p>
              <button
                type="button"
                className="btn btn-primary"
                onClick={pickFolder}
                disabled={loading}
              >
                {loading ? "Opening…" : "Choose folder…"}
              </button>
            </section>
          </>
        )}

        {step === "configure" && repo && (
          <section className="connect-panel card">
            <div className="repo-path-row">
              <span className="field-label">Repository</span>
              <code className="repo-path mono">{repo.path}</code>
              {selectedRepoId && (
                <span className="badge mono">existing connection</span>
              )}
              <button
                type="button"
                className="btn btn-subtle"
                onClick={() => {
                  setStep("pick");
                  setRepo(null);
                  setSelectedRepoId(null);
                  setError(null);
                }}
              >
                Change repo
              </button>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="pipeline-name">
                Pipeline name
              </label>
              <input
                id="pipeline-name"
                className="field-input"
                value={pipelineName}
                onChange={(e) => setPipelineName(e.target.value)}
              />
            </div>

            <div className="field-row">
              <div className="field">
                <label className="field-label" htmlFor="source-branch">
                  Source branch
                </label>
                <select
                  id="source-branch"
                  className="field-select mono"
                  value={sourceBranch}
                  onChange={(e) => setSourceBranch(e.target.value)}
                >
                  {branchOptions.map((branch) => (
                    <option key={branch} value={branch}>
                      {branch}
                    </option>
                  ))}
                </select>
                <span className="field-hint">Merged changes live here</span>
              </div>

              <div className="field">
                <label className="field-label" htmlFor="target-branch">
                  Target branch
                </label>
                <select
                  id="target-branch"
                  className="field-select mono"
                  value={targetBranch}
                  onChange={(e) => setTargetBranch(e.target.value)}
                >
                  {branchOptions.map((branch) => (
                    <option key={branch} value={branch}>
                      {branch}
                    </option>
                  ))}
                </select>
                <span className="field-hint">Promote selected changes here</span>
              </div>
            </div>

            <div className="connect-actions">
              <button
                type="button"
                className="btn btn-primary"
                onClick={submit}
                disabled={
                  loading ||
                  !pipelineName.trim() ||
                  !sourceBranch ||
                  !targetBranch ||
                  sourceBranch === targetBranch
                }
              >
                {loading ? "Creating…" : "Create pipeline"}
              </button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

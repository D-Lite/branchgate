import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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

const CREATE_TARGET = "__branchgate_create__";

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
  const [newBranchName, setNewBranchName] = useState("");
  const [newBranchBase, setNewBranchBase] = useState("");
  const [pipelineName, setPipelineName] = useState("");
  const autoNameRef = useRef("");

  const stepLabel = step === "pick" ? "Step 1 of 2" : "Step 2 of 2";
  const creatingTarget = targetBranch === CREATE_TARGET;
  const effectiveTarget = creatingTarget ? newBranchName.trim() : targetBranch;
  const branchOptions = useMemo(() => repo?.branches ?? [], [repo]);

  useEffect(() => {
    listConnectedRepos()
      .then(setExistingRepos)
      .catch(() => setExistingRepos([]));
  }, []);

  const applyGeneratedName = useCallback(
    (repoName: string, source: string, target: string) => {
      if (!target) return;
      const generated = defaultPipelineName(repoName, source, target);
      setPipelineName((current) => {
        if (!current || current === autoNameRef.current) {
          autoNameRef.current = generated;
          return generated;
        }
        return current;
      });
    },
    [],
  );

  const loadRepo = useCallback(
    async (path: string, repoId?: number | null) => {
      const info = await inspectLocalRepo(path);
      const suggested = suggestBranches(info.branches);
      setRepo(info);
      setSelectedRepoId(repoId ?? info.existingRepoId);
      setSourceBranch(suggested.source);
      setTargetBranch(suggested.target);
      setNewBranchName("");
      setNewBranchBase(suggested.source);
      const generated = defaultPipelineName(
        info.name,
        suggested.source,
        suggested.target,
      );
      autoNameRef.current = generated;
      setPipelineName(generated);
      setStep("configure");
    },
    [],
  );

  useEffect(() => {
    if (!repo || step !== "configure") return;
    let cancelled = false;

    const refreshBranches = async () => {
      try {
        const info = await inspectLocalRepo(repo.path);
        if (cancelled) return;
        setRepo((current) => {
          if (!current) return current;
          const same =
            current.branches.length === info.branches.length &&
            current.branches.every((branch, index) => branch === info.branches[index]);
          if (same) return current;
          return { ...current, branches: info.branches };
        });
      } catch {
        // Keep the last known list if Git is briefly unavailable.
      }
    };

    const onFocus = () => {
      void refreshBranches();
    };
    const onVisibility = () => {
      if (document.visibilityState === "visible") onFocus();
    };

    void refreshBranches();
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibility);
    const interval = window.setInterval(onFocus, 2500);

    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibility);
      window.clearInterval(interval);
    };
  }, [repo?.path, step]);

  useEffect(() => {
    if (!repo || !sourceBranch || !effectiveTarget) return;
    applyGeneratedName(repo.name, sourceBranch, effectiveTarget);
  }, [repo, sourceBranch, effectiveTarget, applyGeneratedName]);

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
    if (!repo || !effectiveTarget) return;
    setError(null);
    setLoading(true);
    try {
      const pipeline = await connectLocalRepo({
        localPath: repo.path,
        pipelineName: pipelineName.trim(),
        sourceBranch,
        targetBranch: effectiveTarget,
        repoId: selectedRepoId,
        createTargetBranch: creatingTarget,
        targetBaseBranch: creatingTarget
          ? newBranchBase || sourceBranch
          : undefined,
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
    effectiveTarget,
    selectedRepoId,
    creatingTarget,
    newBranchBase,
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
                  onChange={(e) => {
                    const next = e.target.value;
                    setSourceBranch(next);
                    if (creatingTarget && (!newBranchBase || newBranchBase === sourceBranch)) {
                      setNewBranchBase(next);
                    }
                  }}
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
                  onChange={(e) => {
                    const next = e.target.value;
                    setTargetBranch(next);
                    if (next === CREATE_TARGET) {
                      setNewBranchBase((current) => current || sourceBranch);
                    }
                  }}
                >
                  {branchOptions.map((branch) => (
                    <option key={branch} value={branch}>
                      {branch}
                    </option>
                  ))}
                  <option value={CREATE_TARGET}>Create new branch…</option>
                </select>
                <span className="field-hint">
                  {creatingTarget
                    ? "Creates a local branch without switching your checkout"
                    : "Promote selected changes here"}
                </span>
              </div>
            </div>

            {creatingTarget && (
              <div className="create-branch-panel">
                <div className="field">
                  <label className="field-label" htmlFor="new-branch-name">
                    New branch name
                  </label>
                  <input
                    id="new-branch-name"
                    className="field-input mono"
                    value={newBranchName}
                    onChange={(e) => setNewBranchName(e.target.value)}
                    placeholder="release/next"
                    autoComplete="off"
                    spellCheck={false}
                  />
                </div>
                <div className="field">
                  <label className="field-label" htmlFor="new-branch-base">
                    Create from
                  </label>
                  <select
                    id="new-branch-base"
                    className="field-select mono"
                    value={newBranchBase || sourceBranch}
                    onChange={(e) => setNewBranchBase(e.target.value)}
                  >
                    {branchOptions.map((branch) => (
                      <option key={branch} value={branch}>
                        {branch}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            )}

            <div className="connect-actions">
              <button
                type="button"
                className="btn btn-primary"
                onClick={submit}
                disabled={
                  loading ||
                  !pipelineName.trim() ||
                  !sourceBranch ||
                  !effectiveTarget ||
                  sourceBranch === effectiveTarget
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

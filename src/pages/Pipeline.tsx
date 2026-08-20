import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import {
  abortPromotionRun,
  continuePromotionRun,
  deletePipeline,
  formatRelativeTime,
  getActivePromotionRun,
  getPipelineChecklist,
  getPipelineSyncStatus,
  openConflictInEditor,
  openRepoInEditor,
  refreshPromotionRun,
  shortSha,
  startPromotionRun,
  syncPipeline,
  type ChecklistItem,
  type Pipeline,
  type PromotionRunResult,
  type SyncProgress,
  type SyncSummary,
} from "../lib/tauri";
import { useShortcutActions } from "../hooks/useShortcutActions";
import {
  applyCachedSelection,
  clearCachedSelection,
  saveCachedSelection,
} from "../lib/selectionCache";
import {
  track,
} from "../lib/analytics";

interface PipelinePageProps {
  pipelines: Pipeline[];
  onPipelinesChange: () => void | Promise<void>;
}

type ModalStep = "none" | "confirm" | "running" | "result";

function statusToSyncInfo(
  pipelineId: number,
  status: {
    syncedAt: number | null;
    sourceHead: string | null;
    targetHead: string | null;
    pendingCount: number;
    promotedCount: number;
  },
): SyncSummary | null {
  if (!status.syncedAt || !status.sourceHead || !status.targetHead) return null;
  return {
    pipelineId,
    sourceHead: status.sourceHead,
    targetHead: status.targetHead,
    unitsFound: status.pendingCount + status.promotedCount,
    pendingCount: status.pendingCount,
    promotedCount: status.promotedCount,
    syncedAt: status.syncedAt,
  };
}

export function PipelinePage({ pipelines, onPipelinesChange }: PipelinePageProps) {
  const { id } = useParams();
  const navigate = useNavigate();
  const pipelineId = Number(id);
  const pipeline = pipelines.find((p) => p.id === pipelineId);

  const [items, setItems] = useState<ChecklistItem[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const [syncInfo, setSyncInfo] = useState<SyncSummary | null>(null);
  const [syncProgress, setSyncProgress] = useState<SyncProgress | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [modalStep, setModalStep] = useState<ModalStep>("none");
  const [runResult, setRunResult] = useState<PromotionRunResult | null>(null);
  const [promoteError, setPromoteError] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<PromotionRunResult | null>(null);
  const [editorError, setEditorError] = useState<string | null>(null);
  const [runBusy, setRunBusy] = useState(false);
  const [deletingPipeline, setDeletingPipeline] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [deletePipelineError, setDeletePipelineError] = useState<string | null>(null);
  const modalRef = useRef<HTMLDivElement>(null);
  const syncGeneration = useRef(0);

  const applySelection = useCallback(
    (rows: ChecklistItem[]) => {
      const pendingIds = rows
        .filter((r) => r.status === "pending")
        .map((r) => r.prId);
      setSelected(applyCachedSelection(pipelineId, pendingIds));
    },
    [pipelineId],
  );

  const persistSelection = useCallback(
    (next: Set<number>, rows: ChecklistItem[]) => {
      const pendingIds = new Set(
        rows.filter((r) => r.status === "pending").map((r) => r.prId),
      );
      saveCachedSelection(
        pipelineId,
        [...next].filter((id) => pendingIds.has(id)),
      );
    },
    [pipelineId],
  );

  const loadChecklist = useCallback(async () => {
    if (!pipelineId) return;
    const rows = await getPipelineChecklist(pipelineId);
    setItems(rows);
    applySelection(rows);
  }, [pipelineId, applySelection]);

  const loadActiveRun = useCallback(async () => {
    if (!pipelineId) return;
    try {
      const run = await getActivePromotionRun(pipelineId);
      setActiveRun(run);
    } catch {
      setActiveRun(null);
    }
  }, [pipelineId]);

  const refreshConflict = useCallback(async (runId: number) => {
    const result = await refreshPromotionRun(runId);
    setActiveRun(result);
    setRunResult((current) => current?.runId === runId ? result : current);
    return result;
  }, []);

  useEffect(() => {
    if (!activeRun || activeRun.status !== "failed") return;
    const refresh = () => {
      void refreshConflict(activeRun.runId).catch(() => {});
    };
    const interval = window.setInterval(refresh, 2500);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", refresh);
    };
  }, [activeRun?.runId, activeRun?.status, refreshConflict]);

  const runSync = useCallback(async (cancelled?: () => boolean) => {
    if (!pipelineId) return;
    const generation = ++syncGeneration.current;
    const isCancelled = () =>
      (typeof cancelled === "function" ? cancelled() : false) ||
      syncGeneration.current !== generation;
    setError(null);
    setSyncing(true);
    setSyncProgress(null);
    const unlisten = await listen<SyncProgress>("pipeline-sync-progress", (event) => {
      if (event.payload.pipelineId !== pipelineId || isCancelled()) return;
      setSyncProgress(event.payload);
      void getPipelineChecklist(pipelineId)
        .then((rows) => {
          if (isCancelled()) return;
          setItems(rows);
          applySelection(rows);
        })
        .catch(() => {});
    });
    try {
      const summary = await syncPipeline(pipelineId);
      if (isCancelled()) return;
      setSyncInfo(summary);
      const rows = await getPipelineChecklist(pipelineId);
      if (isCancelled()) return;
      setItems(rows);
      applySelection(rows);
      onPipelinesChange();
    } catch (err) {
      if (!isCancelled()) {
        setError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      unlisten();
      if (syncGeneration.current === generation) {
        setSyncing(false);
        setSyncProgress(null);
      }
    }
  }, [pipelineId, applySelection, onPipelinesChange]);

  const closeModal = useCallback(() => {
    setModalStep("none");
    setRunResult(null);
    setPromoteError(null);
  }, []);

  const closeDeleteConfirm = useCallback(() => {
    if (deletingPipeline) return;
    setDeleteConfirmOpen(false);
    setDeletePipelineError(null);
  }, [deletingPipeline]);

  useEffect(() => {
    if (modalStep === "none" && !deleteConfirmOpen) return;
    const previousFocus = document.activeElement as HTMLElement | null;
    const modal = modalRef.current;
    const focusable = () =>
      [...(modal?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? [])];
    window.setTimeout(() => focusable()[0]?.focus(), 0);

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && deleteConfirmOpen && !deletingPipeline) {
        event.preventDefault();
        closeDeleteConfirm();
        return;
      }
      if (event.key === "Escape" && modalStep !== "running" && !runBusy) {
        event.preventDefault();
        closeModal();
        return;
      }
      if (event.key !== "Tab") return;
      const elements = focusable();
      if (elements.length === 0) return;
      const first = elements[0];
      const last = elements[elements.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      previousFocus?.focus();
    };
  }, [
    modalStep,
    runBusy,
    closeModal,
    deleteConfirmOpen,
    deletingPipeline,
    closeDeleteConfirm,
  ]);

  useEffect(() => {
    if (!pipelineId || !pipeline) return;

    let cancelled = false;

    setItems([]);
    setSelected(new Set());
    setExpanded(new Set());
    setSyncInfo(null);
    setSyncProgress(null);
    setSyncing(false);
    setError(null);

    (async () => {
      try {
        const [rows, status] = await Promise.all([
          getPipelineChecklist(pipelineId),
          getPipelineSyncStatus(pipelineId),
        ]);
        if (cancelled) return;

        setItems(rows);
        applySelection(rows);
        setSyncInfo(statusToSyncInfo(pipelineId, status));
        void loadActiveRun();

        if (status.needsSync) {
          await runSync(() => cancelled);
          if (!cancelled) void loadActiveRun();
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      }
    })();

    return () => {
      cancelled = true;
      syncGeneration.current += 1;
    };
  }, [pipelineId, pipeline, applySelection, loadActiveRun, runSync]);

  const { register, unregister } = useShortcutActions();
  const pendingItems = items.filter((i) => i.status === "pending");
  const promotedItems = items.filter((i) => i.status === "promoted");

  useEffect(() => {
    register({
      refresh: () => {
        if (!syncing) void runSync();
      },
      promote: () => {
        if (selected.size > 0 && modalStep === "none") {
          setModalStep("confirm");
        }
      },
      selectAll: () => {
        const pendingIds = items
          .filter((i) => i.status === "pending")
          .map((i) => i.prId);
        const next = new Set(pendingIds);
        setSelected(next);
        persistSelection(next, items);
      },
      closeOverlay: () => {
        if (deleteConfirmOpen && !deletingPipeline) {
          closeDeleteConfirm();
        } else if (modalStep !== "none" && modalStep !== "running") {
          closeModal();
        }
      },
    });
    return unregister;
  }, [
    register,
    unregister,
    runSync,
    syncing,
    selected.size,
    modalStep,
    items,
    closeModal,
    deleteConfirmOpen,
    deletingPipeline,
    closeDeleteConfirm,
    persistSelection,
  ]);

  const selectedItems = items.filter((i) => selected.has(i.prId));
  const remainingConflictCount =
    runResult?.items.reduce((count, item) => count + item.conflictFiles.length, 0) ?? 0;
  const syncLabel = syncing
    ? syncProgress && syncProgress.totalCount > 0
      ? `Syncing ${syncProgress.loadedCount} of ${syncProgress.totalCount}…`
      : "Syncing…"
    : "Refresh";

  const recordPromotionResult = useCallback(
    (result: { status: string; items: { prId: number }[] }) => {
      const prCount = result.items.length;
      if (result.status === "merged") {
        track.promotionRunCompleted(prCount, false);
      } else if (result.status === "failed") {
        track.promotionRunCompleted(prCount, true);
      }
    },
    [],
  );

  const startPromote = useCallback(async () => {
    if (!pipelineId || selected.size === 0) return;
    setPromoteError(null);
    setModalStep("running");
    track.promotionRunStarted(selected.size);
    try {
      const result = await startPromotionRun(pipelineId, [...selected]);
      setRunResult(result);
      setModalStep("result");
      if (result.status === "merged") {
        clearCachedSelection(pipelineId);
      }
      recordPromotionResult(result);
      await loadChecklist();
      await loadActiveRun();
      onPipelinesChange();
    } catch (err) {
      setPromoteError(err instanceof Error ? err.message : String(err));
      setModalStep("confirm");
    }
  }, [
    pipelineId,
    selected,
    loadChecklist,
    loadActiveRun,
    onPipelinesChange,
    recordPromotionResult,
  ]);

  const handleAbort = useCallback(async () => {
    if (!runResult) return;
    setRunBusy(true);
    try {
      await abortPromotionRun(runResult.runId);
      closeModal();
      setActiveRun(null);
      try {
        await runSync();
        await loadActiveRun();
      } catch (refreshError) {
        setError(
          `Run aborted, but the pipeline could not refresh: ${
            refreshError instanceof Error ? refreshError.message : String(refreshError)
          }`,
        );
      }
    } catch (err) {
      setPromoteError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunBusy(false);
    }
  }, [runResult, runSync, loadActiveRun, closeModal]);

  const continueResolving = useCallback(async () => {
    const currentRun = runResult ?? activeRun;
    if (!currentRun) return;
    setEditorError(null);
    setPromoteError(null);
    setRunBusy(true);
    try {
      const result = await continuePromotionRun(currentRun.runId);
      setRunResult(result);
      setActiveRun(result.status === "failed" ? result : null);
      setModalStep("result");
      recordPromotionResult(result);
      await loadChecklist();
      await loadActiveRun();
      onPipelinesChange();
    } catch (err) {
      setPromoteError(err instanceof Error ? err.message : String(err));
      setModalStep("result");
      await refreshConflict(currentRun.runId).catch(() => currentRun);
    } finally {
      setRunBusy(false);
    }
  }, [
    runResult,
    activeRun,
    recordPromotionResult,
    loadChecklist,
    loadActiveRun,
    onPipelinesChange,
    refreshConflict,
  ]);

  const handleOpenFile = useCallback(
    async (runId: number, filePath: string) => {
      setEditorError(null);
      try {
        const editor = await openConflictInEditor(runId, filePath);
        track.conflictEditorOpened(editor);
      } catch (err) {
        setEditorError(err instanceof Error ? err.message : String(err));
      }
    },
    [],
  );

  const handleOpenRepo = useCallback(async (runId: number) => {
    setEditorError(null);
    try {
      await openRepoInEditor(runId);
    } catch (err) {
      setEditorError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  const toggleSelected = (prId: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(prId)) next.delete(prId);
      else next.add(prId);
      persistSelection(next, items);
      return next;
    });
  };

  const deselectAll = useCallback(() => {
    const next = new Set<number>();
    setSelected(next);
    persistSelection(next, items);
  }, [items, persistSelection]);

  const handleDeletePipeline = useCallback(async () => {
    if (!pipeline || activeRun) return;
    setDeletingPipeline(true);
    setDeletePipelineError(null);
    try {
      await deletePipeline(pipelineId);
      clearCachedSelection(pipelineId);
      setDeleteConfirmOpen(false);
      await onPipelinesChange();
      navigate("/");
    } catch (deleteError) {
      setDeletePipelineError(
        deleteError instanceof Error ? deleteError.message : String(deleteError),
      );
    } finally {
      setDeletingPipeline(false);
    }
  }, [pipeline, activeRun, pipelineId, onPipelinesChange, navigate]);

  const requestDeletePipeline = useCallback(() => {
    if (activeRun) return;
    setDeletePipelineError(null);
    setDeleteConfirmOpen(true);
  }, [activeRun]);

  const toggleExpanded = (prId: number) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(prId)) next.delete(prId);
      else next.add(prId);
      return next;
    });
  };

  if (!pipeline) {
    return (
      <>
        <header className="page-header">
          <h1>Pipeline not found</h1>
        </header>
        <div className="page-body">
          <div className="empty-state">
            <p>This pipeline does not exist or was removed.</p>
          </div>
        </div>
      </>
    );
  }

  return (
    <>
      <header className="page-header pipeline-header">
        <div>
          <h1>{pipeline.name}</h1>
          <p className="mono">
            {pipeline.sourceBranch} → {pipeline.targetBranch}
            {pipeline.repoName && <> · {pipeline.repoName}</>}
          </p>
          {syncing && syncProgress && syncProgress.totalCount > 0 ? (
            <p className="sync-meta mono">
              Loading {syncProgress.loadedCount} of {syncProgress.totalCount} commits
              {(syncProgress.pendingCount > 0 || syncProgress.promotedCount > 0) && (
                <>
                  {" "}
                  · {syncProgress.pendingCount} pending · {syncProgress.promotedCount}{" "}
                  promoted
                </>
              )}
            </p>
          ) : (
            syncInfo && (
              <p className="sync-meta mono">
                Synced {formatRelativeTime(syncInfo.syncedAt)} ·{" "}
                {syncInfo.pendingCount} pending · {syncInfo.promotedCount} promoted
              </p>
            )
          )}
        </div>
        <div className="pipeline-actions">
          <button
            type="button"
            className="btn btn-subtle pipeline-delete-btn"
            disabled={deletingPipeline || Boolean(activeRun)}
            title={activeRun ? "Abort the active promotion before deleting this pipeline" : undefined}
            onClick={requestDeletePipeline}
          >
            {deletingPipeline ? "Deleting…" : "Delete pipeline"}
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => void runSync()}
            disabled={syncing || modalStep === "running"}
          >
            <span className={syncing ? "spin-icon" : ""}>↻</span>
            {syncLabel}
          </button>
          {selected.size > 0 && (
            <button
              type="button"
              className="btn btn-subtle"
              disabled={modalStep === "running"}
              onClick={deselectAll}
            >
              Deselect all
            </button>
          )}
          <button
            type="button"
            className="btn btn-primary"
            disabled={selected.size === 0 || modalStep === "running" || Boolean(activeRun) || syncing}
            onClick={() => setModalStep("confirm")}
          >
            Promote selected ({selected.size})
          </button>
        </div>
      </header>

      <div className="page-body">
        {error && <div className="pipeline-error">{error}</div>}

        {activeRun && modalStep === "none" && (
          <div className="active-run-banner">
            <div>
              <strong className="mono">
                {activeRun.recoverable
                  ? activeRun.canContinue
                    ? "Ready to continue"
                    : "Conflict in progress"
                  : "Promotion stopped"}
              </strong>
              <p>
                {activeRun.recoverable ? (
                  activeRun.canContinue ? (
                    <>
                      All conflicts are resolved and staged. Continue the promotion to finish
                      merging onto <span className="mono">{activeRun.targetBranch}</span>.
                    </>
                  ) : (
                    <>
                      Promotion stopped on <span className="mono">{activeRun.branchName}</span>
                      {activeRun.conflictPhase === "merge" ? (
                        <> while merging into <span className="mono">{activeRun.targetBranch}</span></>
                      ) : (
                        <> during cherry-pick</>
                      )}
                      . Resolve and stage the listed files, then continue the promotion here.
                    </>
                  )
                ) : (
                  <>Git could not apply one of the selected changes. Review the error, then abort and refresh the pipeline.</>
                )}
              </p>
            </div>
            <div className="active-run-actions">
              {activeRun.recoverable && (
                <button
                  type="button"
                  className="btn btn-primary"
                  onClick={() => {
                    setRunResult(activeRun);
                    setModalStep("result");
                  }}
                >
                  {activeRun.canContinue ? "Continue promotion" : "Resolve conflict"}
                </button>
              )}
              <button
                type="button"
                className="btn btn-subtle"
                onClick={() => {
                  setRunResult(activeRun);
                  setModalStep("result");
                }}
              >
                View details
              </button>
            </div>
          </div>
        )}

        {items.length === 0 && !syncing ? (
          <div className="empty-state">
            <h2>No changes loaded yet</h2>
            <p>
              Hit <strong>Refresh</strong> to scan{" "}
              <span className="mono">{pipeline.sourceBranch}</span> for commits
              not yet on{" "}
              <span className="mono">{pipeline.targetBranch}</span>.
            </p>
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => void runSync()}
              disabled={syncing}
            >
              Refresh pipeline
            </button>
          </div>
        ) : items.length === 0 && syncing ? (
          <div className="empty-state">
            <h2>Loading commits…</h2>
            <p>
              {syncProgress && syncProgress.totalCount > 0
                ? `Showing the first commits as they land. ${syncProgress.loadedCount} of ${syncProgress.totalCount} loaded.`
                : "Scanning the repository. Commits will appear at the top of the list as they load."}
            </p>
          </div>
        ) : (
          <>
            {pendingItems.length > 0 && (
              <section className="checklist-section">
                <div className="checklist-section-head">
                  <h2>Ready to promote</h2>
                  <span className="badge mono">{pendingItems.length} pending</span>
                </div>
                <div className="checklist">
                  {pendingItems.map((item) => (
                    <ChecklistRow
                      key={item.prId}
                      item={item}
                      checked={selected.has(item.prId)}
                      expanded={expanded.has(item.prId)}
                      onToggle={() => toggleSelected(item.prId)}
                      onExpand={() => toggleExpanded(item.prId)}
                    />
                  ))}
                </div>
              </section>
            )}

            {promotedItems.length > 0 && (
              <section className="checklist-section">
                <div className="checklist-section-head">
                  <h2>On target</h2>
                  <span className="badge mono">{promotedItems.length} promoted</span>
                </div>
                <div className="checklist checklist-dimmed">
                  {promotedItems.map((item) => (
                    <ChecklistRow
                      key={item.prId}
                      item={item}
                      checked={false}
                      expanded={expanded.has(item.prId)}
                      onToggle={() => {}}
                      onExpand={() => toggleExpanded(item.prId)}
                      promoted
                    />
                  ))}
                </div>
              </section>
            )}

            {items.length > 0 && pendingItems.length === 0 && promotedItems.length === 0 && (
              <div className="empty-state">
                <p>No pending or promoted items in this pipeline.</p>
              </div>
            )}
          </>
        )}
      </div>

      {deleteConfirmOpen && (
        <div className="modal-backdrop" onClick={closeDeleteConfirm}>
          <div
            ref={modalRef}
            className="modal delete-pipeline-modal"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="delete-pipeline-title"
            aria-describedby="delete-pipeline-description"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="delete-pipeline-heading">
              <span className="delete-pipeline-icon" aria-hidden="true">
                <svg viewBox="0 0 24 24" fill="none">
                  <path d="M4 7h16M9 3h6l1 4H8l1-4Z" />
                  <path d="m6 7 1 14h10l1-14M10 11v6M14 11v6" />
                </svg>
              </span>
              <div>
                <span className="delete-pipeline-kicker">Pipeline settings</span>
                <h2 id="delete-pipeline-title">Delete this pipeline?</h2>
              </div>
            </div>

            <div className="delete-pipeline-body">
              <p className="modal-desc" id="delete-pipeline-description">
                Branchgate will remove this pipeline from your workspace and stop syncing it.
              </p>

              <div className="delete-pipeline-summary">
                <strong>{pipeline.name}</strong>
                <span className="mono">
                  {pipeline.sourceBranch} → {pipeline.targetBranch}
                </span>
              </div>

              <p className="delete-pipeline-note">
                <span aria-hidden="true">✓</span>
                Promotion history stays available in History.
              </p>

              {deletePipelineError && (
                <div className="delete-pipeline-error" role="alert">
                  {deletePipelineError}
                </div>
              )}
            </div>

            <div className="delete-pipeline-actions">
              <button
                type="button"
                className="btn btn-subtle"
                disabled={deletingPipeline}
                onClick={closeDeleteConfirm}
              >
                Keep pipeline
              </button>
              <button
                type="button"
                className="btn delete-pipeline-confirm"
                disabled={deletingPipeline}
                onClick={handleDeletePipeline}
              >
                {deletingPipeline ? "Deleting…" : "Delete pipeline"}
              </button>
            </div>
          </div>
        </div>
      )}

      {modalStep !== "none" && (
        <div className="modal-backdrop" onClick={modalStep === "running" ? undefined : closeModal}>
          <div
            ref={modalRef}
            className={`modal card${modalStep === "result" ? " modal-result" : ""}`}
            onClick={(e) => e.stopPropagation()}
            role="dialog"
            aria-modal="true"
            aria-labelledby="promotion-modal-title"
            aria-describedby="promotion-modal-description"
          >
            {modalStep === "confirm" && (
              <>
                <h2 id="promotion-modal-title">Promote {selected.size} change{selected.size === 1 ? "" : "s"}?</h2>
                <p className="modal-desc" id="promotion-modal-description">
                  Branchgate will cherry-pick the selected merges in order, merge them into{" "}
                  <span className="mono">{pipeline.targetBranch}</span>, and clean up the
                  temporary branch automatically.
                </p>
                {promoteError && <div className="pipeline-error">{promoteError}</div>}
                <ul className="modal-pr-list">
                  {selectedItems.map((item) => (
                    <li key={item.prId}>
                      <span className="modal-pr-title">{item.title}</span>
                      <span className="modal-pr-meta mono">
                        {shortSha(item.mergeCommitSha)} · {item.filesChanged} file
                        {item.filesChanged === 1 ? "" : "s"}
                      </span>
                    </li>
                  ))}
                </ul>
                <div className="modal-actions">
                  <button type="button" className="btn btn-subtle" onClick={closeModal}>
                    Cancel
                  </button>
                  <button type="button" className="btn btn-primary" onClick={startPromote}>
                    Start promotion
                  </button>
                </div>
              </>
            )}

            {modalStep === "running" && (
              <>
                <h2 id="promotion-modal-title">Promoting changes…</h2>
                <p className="modal-desc" id="promotion-modal-description">
                  Cherry-picking in order and merging onto{" "}
                  <span className="mono">{pipeline.targetBranch}</span>. This may take a moment.
                </p>
                <div className="modal-spinner mono">Applying in merge order…</div>
              </>
            )}

            {modalStep === "result" && runResult && (
              <div className="modal-result-layout">
                <h2 id="promotion-modal-title">
                  {runResult.status === "merged"
                    ? "Promotion complete"
                    : runResult.canContinue && runResult.recoverable
                      ? "Conflicts resolved"
                      : runResult.recoverable
                        ? "Conflict — needs a hand"
                        : "Promotion stopped"}
                </h2>
                {runResult.status === "merged" ? (
                  <p className="modal-desc" id="promotion-modal-description">
                    Merged {runResult.items.length} change
                    {runResult.items.length === 1 ? "" : "s"} onto{" "}
                    <span className="mono">{pipeline.targetBranch}</span>.
                  </p>
                ) : runResult.recoverable ? (
                  <div className="modal-desc conflict-guidance" id="promotion-modal-description">
                    {runResult.canContinue ? (
                      <p>
                        All conflicted files are resolved and staged. Continue the promotion to
                        finish merging onto{" "}
                        <span className="mono">{runResult.targetBranch}</span>.
                      </p>
                    ) : (
                      <>
                        <p>
                          Branchgate paused{" "}
                          {runResult.conflictPhase === "merge" ? "the final merge" : "a cherry-pick"}{" "}
                          on <span className="mono">
                            {runResult.conflictPhase === "merge"
                              ? runResult.targetBranch
                              : runResult.branchName}
                          </span>.
                        </p>
                        <ol>
                          <li>Open the repository in your editor.</li>
                          <li>Resolve every listed file and stage it with Git.</li>
                          <li>Return here and choose Continue promotion.</li>
                        </ol>
                      </>
                    )}
                    <p className="conflict-live-status" aria-live="polite">
                      {runResult.canContinue
                        ? "All conflicts are staged. The promotion is ready to continue."
                        : remainingConflictCount > 0
                          ? `${remainingConflictCount} conflicted ${remainingConflictCount === 1 ? "file" : "files"} remaining.`
                          : "Git is not ready to continue yet. Check the operation in your repository, then try again."}
                    </p>
                  </div>
                ) : (
                  <div className="modal-desc" id="promotion-modal-description">
                    Git could not apply one of the selected changes and did not enter conflict
                    resolution. Review the error below, abort this run, then refresh the pipeline
                    before trying again.
                  </div>
                )}

                {editorError && <div className="pipeline-error" role="alert">{editorError}</div>}
                {promoteError && <div className="pipeline-error" role="alert">{promoteError}</div>}

                {runResult.status === "failed" && runResult.recoverable && (
                  <div className="modal-editor-actions">
                    <button
                      type="button"
                      className="btn btn-ghost"
                      onClick={() => handleOpenRepo(runResult.runId)}
                      disabled={runBusy}
                    >
                      Open repo in {runResult.preferredEditor ?? "editor"}
                    </button>
                  </div>
                )}

                <ul className="run-items">
                  {runResult.items.map((item) => (
                    <li key={item.prId} className={`run-item run-item-${item.status}`}>
                      <span className="run-item-status mono">{item.status}</span>
                      <span className="run-item-title">{item.title}</span>
                      {item.errorMessage && (
                        <p className="run-item-error">{item.errorMessage}</p>
                      )}
                      {item.conflictFiles.length > 0 && (
                        <ul className="conflict-files">
                          {item.conflictFiles.map((f) => (
                            <li key={f} className="conflict-file-row">
                              <span className="mono">{f}</span>
                              <button
                                type="button"
                                className="btn btn-subtle btn-compact"
                                onClick={() => handleOpenFile(runResult.runId, f)}
                              >
                                Open in {runResult.preferredEditor ?? "editor"}
                              </button>
                            </li>
                          ))}
                        </ul>
                      )}
                    </li>
                  ))}
                </ul>

                {runResult.status === "failed" && runResult.recoverable ? (
                  <div
                    key="recovery-actions"
                    className="modal-actions modal-actions-sticky recovery-actions"
                  >
                    <>
                      <button
                        type="button"
                        className="btn btn-subtle recovery-abort"
                        onClick={handleAbort}
                        disabled={runBusy}
                      >
                        Abort run
                      </button>
                      <button
                        type="button"
                        className="btn btn-ghost"
                        disabled={runBusy}
                        onClick={async () => {
                          setRunBusy(true);
                          setPromoteError(null);
                          try {
                            await refreshConflict(runResult.runId);
                          } catch (err) {
                            setPromoteError(err instanceof Error ? err.message : String(err));
                          } finally {
                            setRunBusy(false);
                          }
                        }}
                      >
                        {runBusy ? "Checking…" : "Check again"}
                      </button>
                      <button
                        type="button"
                        className="btn btn-primary"
                        onClick={continueResolving}
                        disabled={!runResult.canContinue || runBusy}
                      >
                        {runBusy ? "Continuing…" : "Continue promotion"}
                      </button>
                    </>
                    <button
                      type="button"
                      className="btn btn-subtle"
                      onClick={closeModal}
                      disabled={runBusy}
                    >
                      Close
                    </button>
                  </div>
                ) : runResult.status === "failed" ? (
                  <div key="failure-actions" className="modal-actions modal-actions-sticky">
                    <button
                      type="button"
                      className="btn btn-subtle"
                      onClick={handleAbort}
                      disabled={runBusy}
                    >
                      Abort run
                    </button>
                    <button
                      type="button"
                      className="btn btn-primary"
                      onClick={closeModal}
                      disabled={runBusy}
                    >
                      Close
                    </button>
                  </div>
                ) : (
                  <div key="completion-actions" className="modal-actions modal-actions-sticky">
                    <button type="button" className="btn btn-primary" onClick={closeModal}>
                      Done
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
}

function ChecklistRow({
  item,
  checked,
  expanded,
  onToggle,
  onExpand,
  promoted = false,
}: {
  item: ChecklistItem;
  checked: boolean;
  expanded: boolean;
  onToggle: () => void;
  onExpand: () => void;
  promoted?: boolean;
}) {
  return (
    <div className={`checklist-row${promoted ? " checklist-row-promoted" : ""}`}>
      <label className="checklist-check">
        <input
          type="checkbox"
          checked={checked}
          onChange={onToggle}
          disabled={promoted}
        />
      </label>

      <div className="checklist-main">
        <div className="checklist-title-row">
          <button type="button" className="checklist-title" onClick={onExpand}>
            {item.ticketRef && (
              <span className="ticket-ref mono">{item.ticketRef}</span>
            )}
            <span>{item.title}</span>
          </button>
          {promoted && <span className="status-chip promoted mono">on target</span>}
        </div>

        <div className="checklist-meta mono">
          <span>{item.author ?? "unknown"}</span>
          <span>·</span>
          <span>{formatRelativeTime(item.mergedAt)}</span>
          <span>·</span>
          <span>{shortSha(item.mergeCommitSha)}</span>
          {item.mergeStrategy && (
            <>
              <span>·</span>
              <span>{item.mergeStrategy}</span>
            </>
          )}
        </div>

        <div className="checklist-diff mono">
          <span className="diff-stat">
            {item.filesChanged} file{item.filesChanged === 1 ? "" : "s"}
          </span>
          {item.insertions > 0 && (
            <span className="diff-add">+{item.insertions}</span>
          )}
          {item.deletions > 0 && (
            <span className="diff-del">−{item.deletions}</span>
          )}
        </div>

        {expanded && item.changedFiles.length > 0 && (
          <ul className="changed-files">
            {item.changedFiles.map((file) => (
              <li key={file} className="mono">
                {file}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

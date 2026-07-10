import { useCallback, useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import {
  abortPromotionRun,
  continuePromotionRun,
  formatRelativeTime,
  getActivePromotionRun,
  getPipelineChecklist,
  getPipelineSyncStatus,
  openConflictInEditor,
  openRepoInEditor,
  shortSha,
  startPromotionRun,
  syncPipeline,
  type ChecklistItem,
  type Pipeline,
  type PromotionRunResult,
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
  onPipelinesChange: () => void;
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
  const pipelineId = Number(id);
  const pipeline = pipelines.find((p) => p.id === pipelineId);

  const [items, setItems] = useState<ChecklistItem[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const [syncInfo, setSyncInfo] = useState<SyncSummary | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [modalStep, setModalStep] = useState<ModalStep>("none");
  const [runResult, setRunResult] = useState<PromotionRunResult | null>(null);
  const [promoteError, setPromoteError] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<PromotionRunResult | null>(null);
  const [editorError, setEditorError] = useState<string | null>(null);

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

  const runSync = useCallback(async () => {
    if (!pipelineId) return;
    setError(null);
    setSyncing(true);
    try {
      const summary = await syncPipeline(pipelineId);
      setSyncInfo(summary);
      await loadChecklist();
      onPipelinesChange();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSyncing(false);
    }
  }, [pipelineId, loadChecklist, onPipelinesChange]);

  const closeModal = useCallback(() => {
    setModalStep("none");
    setRunResult(null);
    setPromoteError(null);
  }, []);

  useEffect(() => {
    if (!pipelineId || !pipeline) return;

    let cancelled = false;

    setItems([]);
    setSelected(new Set());
    setExpanded(new Set());
    setSyncInfo(null);
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
          setSyncing(true);
          try {
            const summary = await syncPipeline(pipelineId);
            if (cancelled) return;
            setSyncInfo(summary);
            const updated = await getPipelineChecklist(pipelineId);
            if (cancelled) return;
            setItems(updated);
            applySelection(updated);
            onPipelinesChange();
            void loadActiveRun();
          } catch (err) {
            if (!cancelled) {
              setError(err instanceof Error ? err.message : String(err));
            }
          } finally {
            if (!cancelled) setSyncing(false);
          }
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [pipelineId, pipeline, onPipelinesChange, applySelection, loadActiveRun]);

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
        if (modalStep !== "none" && modalStep !== "running") {
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
    persistSelection,
  ]);

  const selectedItems = items.filter((i) => selected.has(i.prId));

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
    try {
      await abortPromotionRun(runResult.runId);
      await loadChecklist();
      await loadActiveRun();
      onPipelinesChange();
      closeModal();
      setActiveRun(null);
    } catch (err) {
      setPromoteError(err instanceof Error ? err.message : String(err));
    }
  }, [runResult, loadChecklist, loadActiveRun, onPipelinesChange, closeModal]);

  const continueResolving = useCallback(async () => {
    if (!activeRun) return;
    setEditorError(null);
    setPromoteError(null);
    try {
      const result = await continuePromotionRun(activeRun.runId);
      setRunResult(result);
      setActiveRun(result);
      setModalStep("result");
      recordPromotionResult(result);
    } catch (err) {
      setPromoteError(err instanceof Error ? err.message : String(err));
      setModalStep("result");
      setRunResult(activeRun);
    }
  }, [activeRun, recordPromotionResult]);

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
          {syncInfo && (
            <p className="sync-meta mono">
              Synced {formatRelativeTime(syncInfo.syncedAt)} ·{" "}
              {syncInfo.pendingCount} pending · {syncInfo.promotedCount} promoted
            </p>
          )}
        </div>
        <div className="pipeline-actions">
          <button
            type="button"
            className="btn btn-ghost"
            onClick={runSync}
            disabled={syncing || modalStep === "running"}
          >
            <span className={syncing ? "spin-icon" : ""}>↻</span>
            {syncing ? "Syncing…" : "Refresh"}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={selected.size === 0 || modalStep === "running"}
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
              <strong className="mono">Conflict in progress</strong>
              <p>
                Promotion stopped on{" "}
                <span className="mono">{activeRun.branchName}</span>
                {activeRun.conflictPhase === "merge" ? (
                  <> while merging into <span className="mono">{activeRun.targetBranch}</span></>
                ) : (
                  <> during cherry-pick</>
                )}
                . Pick up where you left off or abort the run.
              </p>
            </div>
            <div className="active-run-actions">
              <button type="button" className="btn btn-primary" onClick={continueResolving}>
                Continue resolving
              </button>
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
              onClick={runSync}
              disabled={syncing}
            >
              {syncing ? "Syncing…" : "Refresh pipeline"}
            </button>
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

      {modalStep !== "none" && (
        <div className="modal-backdrop" onClick={modalStep === "running" ? undefined : closeModal}>
          <div
            className="modal card"
            onClick={(e) => e.stopPropagation()}
            role="dialog"
            aria-modal="true"
          >
            {modalStep === "confirm" && (
              <>
                <h2>Promote {selected.size} change{selected.size === 1 ? "" : "s"}?</h2>
                <p className="modal-desc">
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
                <h2>Promoting changes…</h2>
                <p className="modal-desc">
                  Cherry-picking in order and merging onto{" "}
                  <span className="mono">{pipeline.targetBranch}</span>. This may take a moment.
                </p>
                <div className="modal-spinner mono">Applying in merge order…</div>
              </>
            )}

            {modalStep === "result" && runResult && (
              <>
                <h2>
                  {runResult.status === "merged"
                    ? "Promotion complete"
                    : "Conflict — needs a hand"}
                </h2>
                {runResult.status === "merged" ? (
                  <p className="modal-desc">
                    Merged {runResult.items.length} change
                    {runResult.items.length === 1 ? "" : "s"} onto{" "}
                    <span className="mono">{pipeline.targetBranch}</span>.
                  </p>
                ) : (
                  <p className="modal-desc">
                    Promotion stopped on a conflict. Resolve files below, then continue in git
                    {runResult.conflictPhase === "merge" ? (
                      <>
                        {" "}on <span className="mono">{runResult.targetBranch}</span>
                      </>
                    ) : (
                      <>
                        {" "}on <span className="mono">{runResult.branchName}</span>
                      </>
                    )}
                    , or abort to undo.
                  </p>
                )}

                {editorError && <div className="pipeline-error">{editorError}</div>}
                {promoteError && <div className="pipeline-error">{promoteError}</div>}

                {runResult.status === "failed" && (
                  <div className="modal-editor-actions">
                    <button
                      type="button"
                      className="btn btn-ghost"
                      onClick={() => handleOpenRepo(runResult.runId)}
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

                <div className="modal-actions">
                  {runResult.status === "failed" && (
                    <button type="button" className="btn btn-subtle" onClick={handleAbort}>
                      Abort run
                    </button>
                  )}
                  <button type="button" className="btn btn-primary" onClick={closeModal}>
                    Done
                  </button>
                </div>
              </>
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

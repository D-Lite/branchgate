import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import {
  formatRelativeTime,
  listPromotionHistory,
  type HistoryRun,
} from "../lib/tauri";
import "./History.css";

function statusLabel(status: string): string {
  switch (status) {
    case "merged":
      return "Completed";
    case "failed":
      return "Conflict";
    case "closed":
      return "Aborted";
    default:
      return status;
  }
}

function statusClass(status: string): string {
  switch (status) {
    case "merged":
      return "history-status-merged";
    case "failed":
      return "history-status-failed";
    case "closed":
      return "history-status-closed";
    default:
      return "";
  }
}

export function HistoryPage() {
  const [runs, setRuns] = useState<HistoryRun[]>([]);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<Set<number>>(new Set());

  useEffect(() => {
    listPromotionHistory()
      .then(setRuns)
      .catch(() => setRuns([]))
      .finally(() => setLoading(false));
  }, []);

  const toggleExpanded = (runId: number) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(runId)) next.delete(runId);
      else next.add(runId);
      return next;
    });
  };

  return (
    <>
      <header className="page-header">
        <h1>History</h1>
        <p>Past promotion runs and which changes were included.</p>
      </header>
      <div className="page-body">
        {loading ? (
          <p className="history-loading mono">Loading history…</p>
        ) : runs.length === 0 ? (
          <div className="empty-state">
            <h2>No promotion runs yet</h2>
            <p>
              When you promote selected changes, each run will appear here with
              the branch used and which items were included.
            </p>
          </div>
        ) : (
          <div className="history-list">
            {runs.map((run) => (
              <article key={run.runId} className="history-run card">
                <div className="history-run-head">
                  <div>
                    <div className="history-run-top">
                      <Link
                        to={`/pipeline/${run.pipelineId}`}
                        className="history-pipeline-link"
                      >
                        {run.pipelineName}
                      </Link>
                      <span className={`history-status mono ${statusClass(run.status)}`}>
                        {statusLabel(run.status)}
                      </span>
                    </div>
                    <p className="history-run-meta mono">
                      {run.sourceBranch} → {run.targetBranch} ·{" "}
                      {formatRelativeTime(run.createdAt)} · {run.itemCount} change
                      {run.itemCount === 1 ? "" : "s"}
                    </p>
                    <p className="history-run-branch mono">{run.branchName}</p>
                  </div>
                  <button
                    type="button"
                    className="btn btn-ghost"
                    onClick={() => toggleExpanded(run.runId)}
                  >
                    {expanded.has(run.runId) ? "Hide" : "Show"} items
                  </button>
                </div>

                {run.status === "failed" && (
                  <p className="history-run-note">
                    Conflict during{" "}
                    {run.conflictPhase === "merge" ? "merge to target" : "cherry-pick"}.
                    {" "}
                    <Link to={`/pipeline/${run.pipelineId}`}>Continue resolving</Link>
                  </p>
                )}

                {expanded.has(run.runId) && (
                  <ul className="history-items">
                    {run.items.map((item) => (
                      <li key={item.prId} className={`history-item history-item-${item.status}`}>
                        <span className="history-item-status mono">{item.status}</span>
                        <span className="history-item-title">{item.title}</span>
                      </li>
                    ))}
                  </ul>
                )}
              </article>
            ))}
          </div>
        )}
      </div>
    </>
  );
}

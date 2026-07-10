import { Link, useNavigate } from "react-router-dom";
import type { Pipeline } from "../lib/tauri";

interface DashboardProps {
  pipelines: Pipeline[];
}

export function Dashboard({ pipelines }: DashboardProps) {
  const navigate = useNavigate();

  return (
    <>
      <header className="page-header">
        <h1>Pipelines</h1>
        <p>Selectively promote merged PRs between branches.</p>
      </header>

      <div className="page-body">
        {pipelines.length === 0 ? (
          <div className="empty-state">
            <h2>No pipelines yet</h2>
            <p>
              Connect a repo and define a source → target branch pair to start
              tracking which PRs are ready to promote.
            </p>
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => navigate("/connect")}
            >
              Connect a repo
            </button>
          </div>
        ) : (
          <div className="pipeline-grid">
            {pipelines.map((p) => (
              <Link
                key={p.id}
                to={`/pipeline/${p.id}`}
                className="card card-interactive pipeline-card"
              >
                <div className="pipeline-card-top">
                  <span className="badge mono">
                    {p.repoName ?? "local repo"}
                  </span>
                  {p.pendingCount > 0 && (
                    <span className="pending-pill mono">{p.pendingCount} pending</span>
                  )}
                </div>
                <h3>{p.name}</h3>
                <p className="branch-flow mono">
                  {p.sourceBranch} → {p.targetBranch}
                </p>
              </Link>
            ))}
            <button
              type="button"
              className="card card-interactive add-pipeline-card"
              onClick={() => navigate("/connect")}
            >
              <span className="add-icon">+</span>
              <span>Add pipeline</span>
            </button>
          </div>
        )}
      </div>
    </>
  );
}

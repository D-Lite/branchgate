use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRunItem {
    pub pr_id: i64,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRun {
    pub run_id: i64,
    pub pipeline_id: i64,
    pub pipeline_name: String,
    pub source_branch: String,
    pub target_branch: String,
    pub branch_name: String,
    pub status: String,
    pub conflict_phase: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub item_count: usize,
    pub items: Vec<HistoryRunItem>,
}

pub async fn list_runs(pool: &SqlitePool) -> Result<Vec<HistoryRun>, String> {
    let rows = sqlx::query_as::<_, RunRow>(
        r#"
        SELECT
            prun.id AS run_id,
            prun.pipeline_id,
            p.name AS pipeline_name,
            p.source_branch,
            p.target_branch,
            prun.branch_name,
            prun.status,
            prun.conflict_phase,
            prun.created_at,
            prun.completed_at
        FROM promotion_runs prun
        JOIN pipelines p ON p.id = prun.pipeline_id
        WHERE prun.status IN ('merged', 'failed', 'closed')
        ORDER BY prun.created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut runs = Vec::new();
    for row in rows {
        let items = sqlx::query_as::<_, ItemRow>(
            r#"
            SELECT pr.id AS pr_id, pr.title, pm.status
            FROM promotion_run_prs prp
            JOIN pull_requests pr ON pr.id = prp.pr_id
            JOIN promotions pm ON pm.pr_id = pr.id AND pm.pipeline_id = ?
            WHERE prp.run_id = ?
            ORDER BY pr.merged_at ASC, pr.id ASC
            "#,
        )
        .bind(row.pipeline_id)
        .bind(row.run_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        let history_items: Vec<HistoryRunItem> = items
            .into_iter()
            .map(|i| HistoryRunItem {
                pr_id: i.pr_id,
                title: i.title,
                status: map_item_status(&i.status),
            })
            .collect();

        runs.push(HistoryRun {
            run_id: row.run_id,
            pipeline_id: row.pipeline_id,
            pipeline_name: row.pipeline_name,
            source_branch: row.source_branch,
            target_branch: row.target_branch,
            branch_name: row.branch_name,
            status: row.status,
            conflict_phase: row.conflict_phase,
            created_at: row.created_at,
            completed_at: row.completed_at,
            item_count: history_items.len(),
            items: history_items,
        });
    }

    Ok(runs)
}

fn map_item_status(status: &str) -> String {
    match status {
        "promoted" | "promoting" => "done".into(),
        "conflict" => "conflict".into(),
        "pending" | "selected" | "skipped" => "queued".into(),
        other => other.into(),
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RunRow {
    run_id: i64,
    pipeline_id: i64,
    pipeline_name: String,
    source_branch: String,
    target_branch: String,
    branch_name: String,
    status: String,
    conflict_phase: Option<String>,
    created_at: i64,
    completed_at: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct ItemRow {
    pr_id: i64,
    title: String,
    status: String,
}

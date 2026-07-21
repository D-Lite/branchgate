use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PipelineRow {
    pub id: i64,
    pub name: String,
    pub source_branch: String,
    pub target_branch: String,
    pub repo_owner: Option<String>,
    pub repo_name: Option<String>,
    pub pending_count: i64,
}

#[tauri::command]
pub async fn list_pipelines() -> Result<Vec<PipelineRow>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;

    let rows = sqlx::query_as::<_, PipelineRow>(
        r#"
        SELECT
            p.id,
            p.name,
            p.source_branch,
            p.target_branch,
            r.owner AS repo_owner,
            r.name AS repo_name,
            COALESCE((
                SELECT COUNT(*)
                FROM promotions pr
                WHERE pr.pipeline_id = p.id AND pr.status = 'pending'
            ), 0) AS pending_count
        FROM pipelines p
        JOIN repos r ON r.id = p.repo_id
        WHERE p.active = 1
        ORDER BY p.created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows)
}

#[tauri::command]
pub async fn pipeline_count() -> Result<i64, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pipelines WHERE active = 1")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(count)
}

#[tauri::command]
pub async fn delete_pipeline(pipeline_id: i64) -> Result<(), String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    let active_run_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM promotion_runs WHERE pipeline_id = ? AND status IN ('running', 'failed')",
    )
    .bind(pipeline_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    if active_run_count > 0 {
        return Err("Abort the active promotion before deleting this pipeline".into());
    }

    let result = sqlx::query("UPDATE pipelines SET active = 0 WHERE id = ? AND active = 1")
        .bind(pipeline_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    if result.rows_affected() == 0 {
        return Err("Pipeline not found or already deleted".into());
    }

    Ok(())
}

pub async fn fetch_pipeline_row(
    pool: &sqlx::SqlitePool,
    pipeline_id: i64,
) -> Result<PipelineRow, String> {
    sqlx::query_as::<_, PipelineRow>(
        r#"
        SELECT
            p.id,
            p.name,
            p.source_branch,
            p.target_branch,
            r.owner AS repo_owner,
            r.name AS repo_name,
            COALESCE((
                SELECT COUNT(*)
                FROM promotions pr
                WHERE pr.pipeline_id = p.id AND pr.status = 'pending'
            ), 0) AS pending_count
        FROM pipelines p
        JOIN repos r ON r.id = p.repo_id
        WHERE p.id = ?
        "#,
    )
    .bind(pipeline_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

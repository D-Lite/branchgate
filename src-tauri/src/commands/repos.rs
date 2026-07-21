use std::path::Path;

use serde::Serialize;

use crate::commands::pipelines::{fetch_pipeline_row, PipelineRow};
use crate::git;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRepoInfo {
    pub path: String,
    pub name: String,
    pub branches: Vec<String>,
    pub default_branch: Option<String>,
    pub existing_repo_id: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectLocalRequest {
    pub local_path: String,
    pub pipeline_name: String,
    pub source_branch: String,
    pub target_branch: String,
    pub repo_id: Option<i64>,
}

#[tauri::command]
pub async fn inspect_local_repo(path: String) -> Result<LocalRepoInfo, String> {
    let path = Path::new(&path);
    git::ensure_git_repo(path)?;

    let canonical = git::canonical_path(path)?;
    let branches = git::list_branches(path)?;
    if branches.is_empty() {
        return Err("No local branches found in this repository".into());
    }

    let default_branch = git::current_branch(path)?
        .filter(|b| branches.iter().any(|existing| existing == b));

    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    let existing_repo_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM repos WHERE kind = 'local' AND local_path = ?",
    )
    .bind(&canonical)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(LocalRepoInfo {
        path: canonical,
        name: git::repo_display_name(path),
        branches,
        default_branch,
        existing_repo_id,
    })
}

#[tauri::command]
pub async fn connect_local_repo(request: ConnectLocalRequest) -> Result<PipelineRow, String> {
    validate_pipeline_request(&request)?;

    let path = Path::new(&request.local_path);
    git::ensure_git_repo(path)?;
    validate_branches(path, &request.source_branch, &request.target_branch)?;

    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    let canonical = git::canonical_path(path)?;

    let repo_id = if let Some(id) = request.repo_id {
        let stored_path: Option<String> = sqlx::query_scalar(
            "SELECT local_path FROM repos WHERE id = ? AND kind = 'local'",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .flatten();

        let stored = stored_path.ok_or_else(|| "Repository not found".to_string())?;
        if stored != canonical {
            return Err("Selected repository path does not match the connected repo".into());
        }
        id
    } else {
        find_or_create_local_repo(pool, path, &canonical).await?
    };

    create_pipeline(
        pool,
        repo_id,
        request.pipeline_name.trim(),
        &request.source_branch,
        &request.target_branch,
    )
    .await
}

#[tauri::command]
pub async fn list_connected_repos() -> Result<Vec<ConnectedRepo>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    let rows = sqlx::query_as::<_, ConnectedRepo>(
        r#"
        SELECT
            r.id,
            r.kind,
            r.owner,
            r.name,
            r.local_path,
            r.working_copy_mode,
            r.default_branch,
            r.default_merge_strategy,
            r.git_backend,
            r.wsl_distro,
            r.created_at,
            COALESCE((
                SELECT COUNT(*) FROM pipelines p
                WHERE p.repo_id = r.id AND p.active = 1
            ), 0) AS pipeline_count
        FROM repos r
        ORDER BY r.created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedRepo {
    pub id: i64,
    pub kind: String,
    pub owner: Option<String>,
    pub name: Option<String>,
    pub local_path: Option<String>,
    pub working_copy_mode: String,
    pub default_branch: Option<String>,
    pub default_merge_strategy: String,
    pub git_backend: String,
    pub wsl_distro: Option<String>,
    pub created_at: i64,
    pub pipeline_count: i64,
}

fn validate_pipeline_request(request: &ConnectLocalRequest) -> Result<(), String> {
    if request.pipeline_name.trim().is_empty() {
        return Err("Pipeline name is required".into());
    }
    if request.source_branch == request.target_branch {
        return Err("Source and target branches must be different".into());
    }
    Ok(())
}

fn validate_branches(path: &Path, source: &str, target: &str) -> Result<(), String> {
    let branches = git::list_branches(path)?;
    if !branches.iter().any(|b| b == source) {
        return Err(format!("Source branch '{source}' not found in repository"));
    }
    if !branches.iter().any(|b| b == target) {
        return Err(format!("Target branch '{target}' not found in repository"));
    }
    Ok(())
}

async fn find_or_create_local_repo(
    pool: &sqlx::SqlitePool,
    path: &Path,
    canonical: &str,
) -> Result<i64, String> {
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM repos WHERE kind = 'local' AND local_path = ?",
    )
    .bind(canonical)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    {
        return Ok(id);
    }

    let now = chrono::Utc::now().timestamp();
    let display_name = git::repo_display_name(path);
    let default_branch = git::current_branch(path)?.unwrap_or_else(|| "main".to_string());

    sqlx::query_scalar(
        r#"
        INSERT INTO repos (kind, name, local_path, working_copy_mode, default_branch, created_at)
        VALUES ('local', ?, ?, 'existing_local', ?, ?)
        RETURNING id
        "#,
    )
    .bind(&display_name)
    .bind(canonical)
    .bind(&default_branch)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

async fn create_pipeline(
    pool: &sqlx::SqlitePool,
    repo_id: i64,
    name: &str,
    source_branch: &str,
    target_branch: &str,
) -> Result<PipelineRow, String> {
    let now = chrono::Utc::now().timestamp();

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let pipeline_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO pipelines (repo_id, name, source_branch, target_branch, created_at)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(repo_id, source_branch, target_branch) DO UPDATE SET
            name = excluded.name,
            active = 1,
            created_at = excluded.created_at
        RETURNING id
        "#,
    )
    .bind(repo_id)
    .bind(name)
    .bind(source_branch)
    .bind(target_branch)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "A pipeline with this source and target branch already exists for this repo".into()
        } else {
            e.to_string()
        }
    })?;

    sqlx::query(
        r#"
        INSERT INTO branch_sync_state (pipeline_id, source_head_sha, target_head_sha, last_synced_at)
        VALUES (?, NULL, NULL, NULL)
        ON CONFLICT(pipeline_id) DO UPDATE SET
            source_head_sha = NULL,
            target_head_sha = NULL,
            last_synced_at = NULL
        "#,
    )
    .bind(pipeline_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    fetch_pipeline_row(pool, pipeline_id).await
}

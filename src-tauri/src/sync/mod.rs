use crate::git;
use std::collections::HashSet;

mod local;

use serde::Serialize;
use sqlx::SqlitePool;

use local::{discover_units, parse_ticket_ref, LogicalUnit};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub pipeline_id: i64,
    pub source_head: String,
    pub target_head: String,
    pub units_found: usize,
    pub pending_count: usize,
    pub promoted_count: usize,
    pub synced_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct PipelineContext {
    repo_id: i64,
    source_branch: String,
    target_branch: String,
    local_path: Option<String>,
    kind: String,
}

pub async fn sync_pipeline(pool: &SqlitePool, pipeline_id: i64) -> Result<SyncSummary, String> {
    let ctx = sqlx::query_as::<_, PipelineContext>(
        r#"
        SELECT p.repo_id, p.source_branch, p.target_branch, r.local_path, r.kind
        FROM pipelines p
        JOIN repos r ON r.id = p.repo_id
        WHERE p.id = ? AND p.active = 1
        "#,
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Pipeline not found".to_string())?;

    if ctx.kind != "local" {
        return Err("Only local repositories can be synced right now".into());
    }

    let local_path = ctx
        .local_path
        .ok_or_else(|| "Repository has no local path".to_string())?;
    let path = std::path::Path::new(&local_path);
    git::ensure_git_repo(path)?;

    let source_head = git::branch_head(path, &ctx.source_branch)?;
    let target_head = git::branch_head(path, &ctx.target_branch)?;
    let cherry = git::cherry_on_target(path, &ctx.target_branch, &ctx.source_branch)?;
    let units = discover_units(path, &ctx.source_branch, &ctx.target_branch, &target_head)?;

    let now = chrono::Utc::now().timestamp();
    let mut pending = 0usize;
    let mut promoted = 0usize;

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for unit in &units {
        let on_target = cherry.get(&unit.merge_commit_sha).copied().unwrap_or(false);
        let status = if on_target { "promoted" } else { "pending" };
        if on_target {
            promoted += 1;
        } else {
            pending += 1;
        }

        let pr_id = upsert_pull_request(&mut tx, ctx.repo_id, unit, &ctx.source_branch).await?;
        upsert_pr_commits(&mut tx, pr_id, unit).await?;
        upsert_promotion(&mut tx, pipeline_id, pr_id, status, on_target, now).await?;
    }

    let current_shas: HashSet<&str> = units
        .iter()
        .map(|unit| unit.merge_commit_sha.as_str())
        .collect();
    let pending_rows = sqlx::query_as::<_, (i64, String)>(
        r#"
        SELECT pm.pr_id, pr.merge_commit_sha
        FROM promotions pm
        JOIN pull_requests pr ON pr.id = pm.pr_id
        WHERE pm.pipeline_id = ? AND pm.status = 'pending'
        "#,
    )
    .bind(pipeline_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    for (pr_id, merge_commit_sha) in pending_rows {
        if !current_shas.contains(merge_commit_sha.as_str()) {
            sqlx::query(
                r#"
                UPDATE promotions
                SET status = 'skipped',
                    error_message = 'Source history changed; refresh selected the current commits'
                WHERE pipeline_id = ? AND pr_id = ? AND status = 'pending'
                "#,
            )
            .bind(pipeline_id)
            .bind(pr_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        }
    }

    sqlx::query(
        r#"
        INSERT INTO branch_sync_state (pipeline_id, source_head_sha, target_head_sha, last_synced_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(pipeline_id) DO UPDATE SET
            source_head_sha = excluded.source_head_sha,
            target_head_sha = excluded.target_head_sha,
            last_synced_at = excluded.last_synced_at
        "#,
    )
    .bind(pipeline_id)
    .bind(&source_head)
    .bind(&target_head)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(SyncSummary {
        pipeline_id,
        source_head,
        target_head,
        units_found: units.len(),
        pending_count: pending,
        promoted_count: promoted,
        synced_at: now,
    })
}

async fn upsert_pull_request(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    repo_id: i64,
    unit: &LogicalUnit,
    base_branch: &str,
) -> Result<i64, String> {
    let ticket_ref = parse_ticket_ref(&unit.title);
    let changed_files_json =
        serde_json::to_string(&unit._diff.changed_files).unwrap_or_else(|_| "[]".to_string());
    let files_changed = unit._diff.files_changed as i64;
    let insertions = unit._diff.insertions as i64;
    let deletions = unit._diff.deletions as i64;

    if let Some(existing) = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM pull_requests WHERE repo_id = ? AND merge_commit_sha = ?",
    )
    .bind(repo_id)
    .bind(&unit.merge_commit_sha)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| e.to_string())?
    {
        sqlx::query(
            r#"
            UPDATE pull_requests
            SET title = ?, author = ?, base_branch = ?, merge_strategy = ?,
                ticket_ref = ?, merged_at = ?,
                files_changed = ?, insertions = ?, deletions = ?, changed_files_json = ?
            WHERE id = ?
            "#,
        )
        .bind(&unit.title)
        .bind(&unit.author)
        .bind(base_branch)
        .bind(unit.merge_strategy)
        .bind(&ticket_ref)
        .bind(unit.merged_at)
        .bind(files_changed)
        .bind(insertions)
        .bind(deletions)
        .bind(&changed_files_json)
        .bind(existing)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
        return Ok(existing);
    }

    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO pull_requests (
            repo_id, number, title, author, base_branch, merge_strategy,
            merge_commit_sha, ticket_ref, merged_at,
            files_changed, insertions, deletions, changed_files_json
        )
        VALUES (?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id
        "#,
    )
    .bind(repo_id)
    .bind(&unit.title)
    .bind(&unit.author)
    .bind(base_branch)
    .bind(unit.merge_strategy)
    .bind(&unit.merge_commit_sha)
    .bind(&ticket_ref)
    .bind(unit.merged_at)
    .bind(files_changed)
    .bind(insertions)
    .bind(deletions)
    .bind(&changed_files_json)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;

    Ok(id)
}

async fn upsert_pr_commits(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    pr_id: i64,
    unit: &LogicalUnit,
) -> Result<(), String> {
    for (sha, patch) in unit.commit_shas.iter().zip(unit.patch_ids.iter()) {
        sqlx::query(
            r#"
            INSERT INTO pr_commits (pr_id, commit_sha, patch_id, authored_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(pr_id, commit_sha) DO UPDATE SET patch_id = excluded.patch_id
            "#,
        )
        .bind(pr_id)
        .bind(sha)
        .bind(patch)
        .bind(unit.merged_at)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn upsert_promotion(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    pipeline_id: i64,
    pr_id: i64,
    status: &str,
    on_target: bool,
    now: i64,
) -> Result<(), String> {
    let existing_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM promotions WHERE pipeline_id = ? AND pr_id = ?",
    )
    .bind(pipeline_id)
    .bind(pr_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;

    if let Some(current) = existing_status {
        if matches!(
            current.as_str(),
            "selected" | "promoting" | "conflict" | "promoted"
        ) {
            return Ok(());
        }
    }

    if on_target {
        sqlx::query(
            r#"
            INSERT INTO promotions (pipeline_id, pr_id, status, promoted_at)
            VALUES (?, ?, 'promoted', ?)
            ON CONFLICT(pipeline_id, pr_id) DO UPDATE SET
                status = 'promoted',
                promoted_at = COALESCE(promotions.promoted_at, excluded.promoted_at)
            "#,
        )
        .bind(pipeline_id)
        .bind(pr_id)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    } else {
        sqlx::query(
            r#"
            INSERT INTO promotions (pipeline_id, pr_id, status)
            VALUES (?, ?, 'pending')
            ON CONFLICT(pipeline_id, pr_id) DO UPDATE SET
                status = CASE
                    WHEN promotions.status IN ('selected', 'promoting', 'conflict', 'promoted') THEN promotions.status
                    ELSE 'pending'
                END
            "#,
        )
        .bind(pipeline_id)
        .bind(pr_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    let _ = status;
    Ok(())
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub pr_id: i64,
    pub merge_commit_sha: String,
    pub title: String,
    pub author: Option<String>,
    pub ticket_ref: Option<String>,
    pub merge_strategy: Option<String>,
    pub merged_at: Option<i64>,
    pub status: String,
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
    pub changed_files: Vec<String>,
}

pub async fn list_checklist(
    pool: &SqlitePool,
    pipeline_id: i64,
) -> Result<Vec<ChecklistItem>, String> {
    let rows = sqlx::query_as::<_, ChecklistRow>(
        r#"
        SELECT
            pr.id AS pr_id,
            pr.merge_commit_sha,
            pr.title,
            pr.author,
            pr.ticket_ref,
            pr.merge_strategy,
            pr.merged_at,
            pm.status,
            pr.files_changed,
            pr.insertions,
            pr.deletions,
            pr.changed_files_json
        FROM promotions pm
        JOIN pull_requests pr ON pr.id = pm.pr_id
        WHERE pm.pipeline_id = ?
        ORDER BY pr.merged_at ASC, pr.id ASC
        "#,
    )
    .bind(pipeline_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for row in rows {
        let changed_files: Vec<String> =
            serde_json::from_str(&row.changed_files_json).unwrap_or_default();

        items.push(ChecklistItem {
            pr_id: row.pr_id,
            merge_commit_sha: row.merge_commit_sha,
            title: row.title,
            author: row.author,
            ticket_ref: row.ticket_ref,
            merge_strategy: row.merge_strategy,
            merged_at: row.merged_at,
            status: row.status,
            files_changed: row.files_changed.max(0) as u32,
            insertions: row.insertions.max(0) as u32,
            deletions: row.deletions.max(0) as u32,
            changed_files,
        });
    }

    Ok(items)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineSyncStatus {
    pub needs_sync: bool,
    pub synced_at: Option<i64>,
    pub source_head: Option<String>,
    pub target_head: Option<String>,
    pub pending_count: i64,
    pub promoted_count: i64,
}

pub async fn get_pipeline_sync_status(
    pool: &SqlitePool,
    pipeline_id: i64,
) -> Result<PipelineSyncStatus, String> {
    let ctx = sqlx::query_as::<_, PipelineContext>(
        r#"
        SELECT p.repo_id, p.source_branch, p.target_branch, r.local_path, r.kind
        FROM pipelines p
        JOIN repos r ON r.id = p.repo_id
        WHERE p.id = ? AND p.active = 1
        "#,
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Pipeline not found".to_string())?;

    let counts = sqlx::query_as::<_, StatusCounts>(
        r#"
        SELECT
            SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) AS pending_count,
            SUM(CASE WHEN status = 'promoted' THEN 1 ELSE 0 END) AS promoted_count
        FROM promotions
        WHERE pipeline_id = ?
        "#,
    )
    .bind(pipeline_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let sync_state = sqlx::query_as::<_, SyncStateRow>(
        r#"
        SELECT source_head_sha, target_head_sha, last_synced_at
        FROM branch_sync_state
        WHERE pipeline_id = ?
        "#,
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut needs_sync = true;
    let (synced_at, stored_source, stored_target) = match &sync_state {
        Some(s) => (
            s.last_synced_at,
            s.source_head_sha.clone(),
            s.target_head_sha.clone(),
        ),
        None => (None, None, None),
    };

    if ctx.kind == "local" {
        if let Some(local_path) = &ctx.local_path {
            let path = std::path::Path::new(local_path);
            if git::ensure_git_repo(path).is_ok() {
                if let (Ok(source_head), Ok(target_head)) = (
                    git::branch_head(path, &ctx.source_branch),
                    git::branch_head(path, &ctx.target_branch),
                ) {
                    needs_sync = sync_state.is_none()
                        || stored_source.as_deref() != Some(source_head.as_str())
                        || stored_target.as_deref() != Some(target_head.as_str());

                    return Ok(PipelineSyncStatus {
                        needs_sync,
                        synced_at,
                        source_head: Some(source_head),
                        target_head: Some(target_head),
                        pending_count: counts.pending_count.unwrap_or(0),
                        promoted_count: counts.promoted_count.unwrap_or(0),
                    });
                }
            }
        }
    }

    Ok(PipelineSyncStatus {
        needs_sync,
        synced_at,
        source_head: stored_source,
        target_head: stored_target,
        pending_count: counts.pending_count.unwrap_or(0),
        promoted_count: counts.promoted_count.unwrap_or(0),
    })
}

#[derive(Debug, sqlx::FromRow)]
struct StatusCounts {
    pending_count: Option<i64>,
    promoted_count: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct SyncStateRow {
    source_head_sha: Option<String>,
    target_head_sha: Option<String>,
    last_synced_at: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct ChecklistRow {
    pr_id: i64,
    merge_commit_sha: String,
    title: String,
    author: Option<String>,
    ticket_ref: Option<String>,
    merge_strategy: Option<String>,
    merged_at: Option<i64>,
    status: String,
    files_changed: i64,
    insertions: i64,
    deletions: i64,
    changed_files_json: String,
}

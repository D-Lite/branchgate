use std::path::Path;

use serde::Serialize;
use sqlx::SqlitePool;

use crate::git::{self, promote};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionRunItem {
    pub pr_id: i64,
    pub title: String,
    pub merge_commit_sha: String,
    pub status: String,
    pub error_message: Option<String>,
    pub conflict_files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionRunResult {
    pub run_id: i64,
    pub branch_name: String,
    pub target_branch: String,
    pub conflict_phase: Option<String>,
    pub status: String,
    pub items: Vec<PromotionRunItem>,
    pub preferred_editor: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct PromoteContext {
    target_branch: String,
    local_path: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ActiveRunRow {
    run_id: i64,
    branch_name: String,
    target_branch: String,
    conflict_phase: Option<String>,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PromotePr {
    pr_id: i64,
    title: String,
    merge_commit_sha: String,
    merge_strategy: Option<String>,
}

pub async fn run_promotion(
    pool: &SqlitePool,
    pipeline_id: i64,
    pr_ids: Vec<i64>,
) -> Result<PromotionRunResult, String> {
    if pr_ids.is_empty() {
        return Err("Select at least one change to promote".into());
    }

    let ctx = sqlx::query_as::<_, PromoteContext>(
        r#"
        SELECT p.target_branch, r.local_path
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

    let local_path = ctx
        .local_path
        .ok_or_else(|| "Repository has no local path".to_string())?;
    let path = Path::new(&local_path);
    git::ensure_git_repo(path)?;

    let placeholders = pr_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!(
        r#"
        SELECT pr.id AS pr_id, pr.title, pr.merge_commit_sha, pr.merge_strategy
        FROM pull_requests pr
        JOIN promotions pm ON pm.pr_id = pr.id AND pm.pipeline_id = ?
        WHERE pr.id IN ({placeholders}) AND pm.status IN ('pending', 'selected')
        ORDER BY pr.merged_at ASC, pr.id ASC
        "#
    );

    let mut q = sqlx::query_as::<_, PromotePr>(&query).bind(pipeline_id);
    for id in &pr_ids {
        q = q.bind(id);
    }
    let prs = q.fetch_all(pool).await.map_err(|e| e.to_string())?;

    if prs.is_empty() {
        return Err("No promotable changes found for the current selection".into());
    }
    if prs.len() != pr_ids.len() {
        return Err("Some selected changes are no longer pending — refresh and try again".into());
    }

    let now = chrono::Utc::now().timestamp();
    let branch_name = format!("branchgate/promote-{now}");
    let original_branch = promote::current_branch(path)?;

    let run_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO promotion_runs (pipeline_id, branch_name, status, created_at)
        VALUES (?, ?, 'running', ?)
        RETURNING id
        "#,
    )
    .bind(pipeline_id)
    .bind(&branch_name)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    for pr in &prs {
        sqlx::query(
            "INSERT INTO promotion_run_prs (run_id, pr_id) VALUES (?, ?)",
        )
        .bind(run_id)
        .bind(pr.pr_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        sqlx::query(
            "UPDATE promotions SET status = 'selected' WHERE pipeline_id = ? AND pr_id = ?",
        )
        .bind(pipeline_id)
        .bind(pr.pr_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    promote::create_branch(path, &branch_name, &ctx.target_branch)?;

    let mut items = Vec::new();
    let mut run_status = "running".to_string();
    let mut conflict_phase: Option<&str> = None;
    let mut cherry_picked: Vec<i64> = Vec::new();

    for pr in prs {
        sqlx::query(
            "UPDATE promotions SET status = 'promoting' WHERE pipeline_id = ? AND pr_id = ?",
        )
        .bind(pipeline_id)
        .bind(pr.pr_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        let is_merge = pr.merge_strategy.as_deref() == Some("merge");
        let outcome = promote::cherry_pick(path, &pr.merge_commit_sha, is_merge);

        if outcome.success {
            cherry_picked.push(pr.pr_id);
            items.push(PromotionRunItem {
                pr_id: pr.pr_id,
                title: pr.title,
                merge_commit_sha: pr.merge_commit_sha,
                status: "done".into(),
                error_message: None,
                conflict_files: Vec::new(),
            });
        } else {
            run_status = "failed".into();
            conflict_phase = Some("cherry_pick");

            for file in &outcome.conflict_files {
                sqlx::query(
                    r#"
                    INSERT INTO conflicts (run_id, pr_id, file_path, status)
                    VALUES (?, ?, ?, 'open')
                    "#,
                )
                .bind(run_id)
                .bind(pr.pr_id)
                .bind(file)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            }

            sqlx::query(
                r#"
                UPDATE promotions
                SET status = 'conflict', error_message = ?
                WHERE pipeline_id = ? AND pr_id = ?
                "#,
            )
            .bind(&outcome.stderr)
            .bind(pipeline_id)
            .bind(pr.pr_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

            items.push(PromotionRunItem {
                pr_id: pr.pr_id,
                title: pr.title.clone(),
                merge_commit_sha: pr.merge_commit_sha,
                status: "conflict".into(),
                error_message: Some(if outcome.stderr.is_empty() {
                    "Cherry-pick conflict".into()
                } else {
                    outcome.stderr
                }),
                conflict_files: outcome.conflict_files,
            });

            break;
        }
    }

    if run_status == "running" && !cherry_picked.is_empty() {
        promote::switch_branch(path, &ctx.target_branch)?;
        let merge_outcome = promote::merge_branch(path, &branch_name);

        if merge_outcome.success {
            let target_head = promote::head_sha(path)?;
            let _ = promote::delete_branch(path, &branch_name);

            for pr_id in &cherry_picked {
                sqlx::query(
                    r#"
                    UPDATE promotions
                    SET status = 'promoted', promoted_commit_sha = ?, promoted_at = ?, error_message = NULL
                    WHERE pipeline_id = ? AND pr_id = ?
                    "#,
                )
                .bind(&target_head)
                .bind(now)
                .bind(pipeline_id)
                .bind(pr_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            }

            sqlx::query(
                r#"
                INSERT INTO branch_sync_state (pipeline_id, target_head_sha, last_synced_at)
                VALUES (?, ?, ?)
                ON CONFLICT(pipeline_id) DO UPDATE SET
                    target_head_sha = excluded.target_head_sha,
                    last_synced_at = excluded.last_synced_at
                "#,
            )
            .bind(pipeline_id)
            .bind(&target_head)
            .bind(now)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

            run_status = "merged".into();

            if original_branch != ctx.target_branch {
                let _ = promote::switch_branch(path, &original_branch);
            }
        } else {
            run_status = "failed".into();
            conflict_phase = Some("merge");
            let conflict_pr_id = cherry_picked.last().copied().unwrap_or(0);

            for file in &merge_outcome.conflict_files {
                sqlx::query(
                    r#"
                    INSERT INTO conflicts (run_id, pr_id, file_path, status)
                    VALUES (?, ?, ?, 'open')
                    "#,
                )
                .bind(run_id)
                .bind(conflict_pr_id)
                .bind(file)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            }

            let error = if merge_outcome.stderr.is_empty() {
                "Merge conflict while promoting to target branch".into()
            } else {
                merge_outcome.stderr
            };

            for pr_id in &cherry_picked {
                sqlx::query(
                    r#"
                    UPDATE promotions
                    SET status = 'conflict', error_message = ?
                    WHERE pipeline_id = ? AND pr_id = ?
                    "#,
                )
                .bind(&error)
                .bind(pipeline_id)
                .bind(pr_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            }

            if let Some(last) = items.last_mut() {
                last.status = "conflict".into();
                last.error_message = Some(error.clone());
                last.conflict_files = merge_outcome.conflict_files;
            }
        }
    }

    if run_status == "merged" {
        sqlx::query(
            "UPDATE promotion_runs SET status = 'merged', completed_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(run_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    } else if run_status == "failed" {
        sqlx::query(
            "UPDATE promotion_runs SET status = 'failed', conflict_phase = ?, completed_at = ? WHERE id = ?",
        )
        .bind(conflict_phase)
        .bind(now)
        .bind(run_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    let preferred_editor = preferred_editor_name(pool).await.ok();

    Ok(PromotionRunResult {
        run_id,
        branch_name,
        target_branch: ctx.target_branch,
        conflict_phase: conflict_phase.map(str::to_string),
        status: run_status,
        items,
        preferred_editor,
    })
}

async fn preferred_editor_name(pool: &SqlitePool) -> Result<String, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT name FROM editors WHERE is_preferred = 1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "No preferred editor — set one in Settings".into())
}

async fn preferred_editor_command(pool: &SqlitePool) -> Result<String, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT command FROM editors WHERE is_preferred = 1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "No preferred editor — set one in Settings".into())
}

pub async fn get_active_run(
    pool: &SqlitePool,
    pipeline_id: i64,
) -> Result<Option<PromotionRunResult>, String> {
    let row = sqlx::query_as::<_, ActiveRunRow>(
        r#"
        SELECT
            prun.id AS run_id,
            prun.branch_name,
            p.target_branch,
            prun.conflict_phase,
            prun.status
        FROM promotion_runs prun
        JOIN pipelines p ON p.id = prun.pipeline_id
        WHERE prun.pipeline_id = ? AND prun.status = 'failed'
        ORDER BY prun.created_at DESC
        LIMIT 1
        "#,
    )
    .bind(pipeline_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let Some(row) = row else {
        return Ok(None);
    };

    build_run_result(pool, &row).await.map(Some)
}

pub async fn prepare_conflict_resolution(
    pool: &SqlitePool,
    run_id: i64,
) -> Result<PromotionRunResult, String> {
    let row = sqlx::query_as::<_, ActiveRunRow>(
        r#"
        SELECT
            prun.id AS run_id,
            prun.branch_name,
            p.target_branch,
            prun.conflict_phase,
            prun.status
        FROM promotion_runs prun
        JOIN pipelines p ON p.id = prun.pipeline_id
        WHERE prun.id = ? AND prun.status = 'failed'
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Promotion run not found or already closed".to_string())?;

    let local_path: Option<String> = sqlx::query_scalar(
        r#"
        SELECT r.local_path
        FROM promotion_runs prun
        JOIN pipelines p ON p.id = prun.pipeline_id
        JOIN repos r ON r.id = p.repo_id
        WHERE prun.id = ?
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .flatten();

    if let Some(local_path) = local_path {
        let path = Path::new(&local_path);
        git::ensure_git_repo(path)?;
        let checkout_branch = match row.conflict_phase.as_deref() {
            Some("merge") => &row.target_branch,
            _ => &row.branch_name,
        };
        promote::switch_branch(path, checkout_branch)?;
    }

    build_run_result(pool, &row).await
}

pub async fn open_conflict_file(
    pool: &SqlitePool,
    run_id: i64,
    file_path: &str,
) -> Result<String, String> {
    let local_path: Option<String> = sqlx::query_scalar(
        r#"
        SELECT r.local_path
        FROM promotion_runs prun
        JOIN pipelines p ON p.id = prun.pipeline_id
        JOIN repos r ON r.id = p.repo_id
        WHERE prun.id = ?
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .flatten();

    let local_path = local_path.ok_or_else(|| "Repository has no local path".to_string())?;
    let repo_path = Path::new(&local_path);
    let editor_name = preferred_editor_name(pool).await?;
    let editor_command = preferred_editor_command(pool).await?;
    let full_path = repo_path.join(file_path);

    crate::editors::open_file(&editor_command, &full_path)?;

    sqlx::query(
        r#"
        UPDATE conflicts
        SET opened_in = ?
        WHERE run_id = ? AND file_path = ? AND status = 'open'
        "#,
    )
    .bind(&editor_name)
    .bind(run_id)
    .bind(file_path)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(editor_name)
}

pub async fn open_repo_in_editor(pool: &SqlitePool, run_id: i64) -> Result<(), String> {
    let local_path: Option<String> = sqlx::query_scalar(
        r#"
        SELECT r.local_path
        FROM promotion_runs prun
        JOIN pipelines p ON p.id = prun.pipeline_id
        JOIN repos r ON r.id = p.repo_id
        WHERE prun.id = ?
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .flatten();

    let local_path = local_path.ok_or_else(|| "Repository has no local path".to_string())?;
    let editor_command = preferred_editor_command(pool).await?;
    crate::editors::open_repo(&editor_command, Path::new(&local_path))
}

async fn build_run_result(
    pool: &SqlitePool,
    row: &ActiveRunRow,
) -> Result<PromotionRunResult, String> {
    let pr_rows = sqlx::query_as::<_, (i64, String, String, String, Option<String>)>(
        r#"
        SELECT pr.id, pr.title, pr.merge_commit_sha, pm.status, pm.error_message
        FROM promotion_run_prs prp
        JOIN pull_requests pr ON pr.id = prp.pr_id
        JOIN promotions pm ON pm.pr_id = pr.id AND pm.pipeline_id = (
            SELECT pipeline_id FROM promotion_runs WHERE id = ?
        )
        WHERE prp.run_id = ?
        ORDER BY pr.merged_at ASC, pr.id ASC
        "#,
    )
    .bind(row.run_id)
    .bind(row.run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let conflict_rows = sqlx::query_as::<_, (i64, String)>(
        r#"
        SELECT pr_id, file_path
        FROM conflicts
        WHERE run_id = ? AND status = 'open'
        ORDER BY id ASC
        "#,
    )
    .bind(row.run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for (pr_id, title, merge_commit_sha, status, error_message) in pr_rows {
        let conflict_files: Vec<String> = conflict_rows
            .iter()
            .filter(|(conflict_pr_id, _)| *conflict_pr_id == pr_id)
            .map(|(_, path)| path.clone())
            .collect();

        let item_status = if status == "conflict" {
            "conflict"
        } else if status == "promoted" || status == "promoting" {
            "done"
        } else {
            "queued"
        };

        items.push(PromotionRunItem {
            pr_id,
            title,
            merge_commit_sha,
            status: item_status.into(),
            error_message,
            conflict_files,
        });
    }

    let preferred_editor = preferred_editor_name(pool).await.ok();

    Ok(PromotionRunResult {
        run_id: row.run_id,
        branch_name: row.branch_name.clone(),
        target_branch: row.target_branch.clone(),
        conflict_phase: row.conflict_phase.clone(),
        status: row.status.clone(),
        items,
        preferred_editor,
    })
}

pub async fn abort_promotion_run(
    pool: &SqlitePool,
    run_id: i64,
) -> Result<(), String> {
    let row: Option<(i64, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT prun.pipeline_id, prun.branch_name, p.target_branch, r.local_path
        FROM promotion_runs prun
        JOIN pipelines p ON p.id = prun.pipeline_id
        JOIN repos r ON r.id = p.repo_id
        WHERE prun.id = ? AND prun.status IN ('running', 'failed')
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let (pipeline_id, branch_name, target_branch, local_path) =
        row.ok_or_else(|| "Promotion run not found".to_string())?;
    let local_path = local_path.ok_or_else(|| "Repository has no local path".to_string())?;
    let path = Path::new(&local_path);

    let _ = promote::cherry_pick_abort(path);
    let _ = promote::merge_abort(path);
    let _ = promote::switch_branch(path, &target_branch);
    let _ = promote::delete_branch(path, &branch_name);

    let pr_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT pr_id FROM promotion_run_prs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    for pr_id in pr_ids {
        sqlx::query(
            r#"
            UPDATE promotions
            SET status = 'pending', promoted_commit_sha = NULL, promoted_at = NULL, error_message = NULL
            WHERE pipeline_id = ? AND pr_id = ?
            "#,
        )
        .bind(pipeline_id)
        .bind(pr_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    sqlx::query(
        "UPDATE promotion_runs SET status = 'closed', completed_at = ? WHERE id = ?",
    )
    .bind(chrono::Utc::now().timestamp())
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query("UPDATE conflicts SET status = 'abandoned' WHERE run_id = ?")
        .bind(run_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

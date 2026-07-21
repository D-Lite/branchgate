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
    pub can_continue: bool,
    pub recoverable: bool,
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
    promote::ensure_clean(path)?;

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM promotion_runs WHERE pipeline_id = ? AND status IN ('running', 'failed')",
    )
    .bind(pipeline_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    if active_count > 0 {
        return Err(
            "This pipeline already has a promotion in progress. Continue or abort it first.".into(),
        );
    }

    let placeholders = pr_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
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
    let mut missing_commits = Vec::new();
    for pr in &prs {
        if !promote::commit_exists(path, &pr.merge_commit_sha)? {
            missing_commits.push(pr.title.clone());
        }
    }
    if !missing_commits.is_empty() {
        return Err(format!(
            "{} selected change{} no longer exist{} in this repository. Refresh the pipeline and select the current changes.",
            missing_commits.len(),
            if missing_commits.len() == 1 { "" } else { "s" },
            if missing_commits.len() == 1 { "s" } else { "" },
        ));
    }

    let now = chrono::Utc::now().timestamp();
    let branch_name = format!(
        "branchgate/promote-{}",
        chrono::Utc::now().timestamp_millis()
    );
    let original_branch = promote::current_branch(path)?;
    promote::create_branch(path, &branch_name, &ctx.target_branch)?;

    let run_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO promotion_runs (pipeline_id, branch_name, original_branch, status, created_at)
        VALUES (?, ?, ?, 'running', ?)
        RETURNING id
        "#,
    )
    .bind(pipeline_id)
    .bind(&branch_name)
    .bind(&original_branch)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    for pr in &prs {
        sqlx::query("INSERT INTO promotion_run_prs (run_id, pr_id) VALUES (?, ?)")
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
            let has_conflicts = !outcome.conflict_files.is_empty();
            conflict_phase = has_conflicts.then_some("cherry_pick");

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
                status: if has_conflicts { "conflict" } else { "failed" }.into(),
                error_message: Some(if outcome.stderr.is_empty() {
                    if has_conflicts {
                        "Cherry-pick conflict".into()
                    } else {
                        "Cherry-pick failed".into()
                    }
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
            conflict_phase = (!merge_outcome.conflict_files.is_empty()).then_some("merge");
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
        sqlx::query("UPDATE promotion_runs SET status = 'merged', completed_at = ? WHERE id = ?")
            .bind(now)
            .bind(run_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    } else if run_status == "failed" {
        sqlx::query(
            "UPDATE promotion_runs SET status = 'failed', conflict_phase = ?, completed_at = NULL WHERE id = ?",
        )
        .bind(conflict_phase)
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
        can_continue: false,
        recoverable: conflict_phase.is_some(),
    })
}

async fn preferred_editor_name(pool: &SqlitePool) -> Result<String, String> {
    sqlx::query_scalar::<_, String>("SELECT name FROM editors WHERE is_preferred = 1 LIMIT 1")
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No preferred editor — set one in Settings".into())
}

async fn preferred_editor_launch(
    pool: &SqlitePool,
) -> Result<(String, Option<String>), String> {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT command, detected_path FROM editors WHERE is_preferred = 1 LIMIT 1",
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

    let local_path = local_path.ok_or_else(|| "Repository has no local path".to_string())?;
    let path = Path::new(&local_path);
    git::ensure_git_repo(path)?;
    let live_files = reconcile_conflicts(pool, run_id, path).await?;
    let mut result = build_run_result(pool, &row).await?;
    result.can_continue = live_files.is_empty()
        && match row.conflict_phase.as_deref() {
            Some("merge") => {
                promote::merge_in_progress(path)?
                    || promote::is_ancestor(path, &row.branch_name, "HEAD")?
            }
            _ => {
                promote::cherry_pick_in_progress(path)?
                    || manually_completed_cherry_pick(pool, run_id, path).await?
            }
        };
    if live_files.is_empty() && !result.can_continue && row.conflict_phase.is_some() {
        sqlx::query("UPDATE promotion_runs SET conflict_phase = NULL WHERE id = ?")
            .bind(run_id)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
        result.conflict_phase = None;
        result.recoverable = false;
    }
    Ok(result)
}

async fn manually_completed_cherry_pick(
    pool: &SqlitePool,
    run_id: i64,
    path: &Path,
) -> Result<bool, String> {
    let conflicted_sha: Option<String> = sqlx::query_scalar(
        r#"
        SELECT pr.merge_commit_sha
        FROM promotion_run_prs prp
        JOIN promotion_runs prun ON prun.id = prp.run_id
        JOIN promotions pm ON pm.pipeline_id = prun.pipeline_id AND pm.pr_id = prp.pr_id
        JOIN pull_requests pr ON pr.id = prp.pr_id
        WHERE prp.run_id = ? AND pm.status = 'conflict'
        ORDER BY prp.rowid DESC LIMIT 1
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(if let Some(sha) = conflicted_sha {
        git::patch_id(path, &sha).ok() == git::patch_id(path, "HEAD").ok()
    } else {
        false
    })
}

async fn reconcile_conflicts(
    pool: &SqlitePool,
    run_id: i64,
    path: &Path,
) -> Result<Vec<String>, String> {
    let live_files = promote::conflict_files(path)?;
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "UPDATE conflicts SET status = 'resolved', resolved_at = ? WHERE run_id = ? AND status = 'open'",
    )
    .bind(now)
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    if live_files.is_empty() {
        return Ok(live_files);
    }

    let conflict_pr_id: i64 = sqlx::query_scalar(
        r#"
        SELECT prp.pr_id
        FROM promotion_run_prs prp
        JOIN promotion_runs prun ON prun.id = prp.run_id
        JOIN promotions pm ON pm.pipeline_id = prun.pipeline_id AND pm.pr_id = prp.pr_id
        WHERE prp.run_id = ?
        ORDER BY CASE WHEN pm.status = 'conflict' THEN 0 ELSE 1 END, prp.rowid DESC
        LIMIT 1
        "#,
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    for file in &live_files {
        let existing_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM conflicts WHERE run_id = ? AND file_path = ? ORDER BY id DESC LIMIT 1",
        )
        .bind(run_id)
        .bind(file)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

        if let Some(id) = existing_id {
            sqlx::query(
                "UPDATE conflicts SET pr_id = ?, status = 'open', resolved_at = NULL WHERE id = ?",
            )
            .bind(conflict_pr_id)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        } else {
            sqlx::query(
                "INSERT INTO conflicts (run_id, pr_id, file_path, status) VALUES (?, ?, ?, 'open')",
            )
            .bind(run_id)
            .bind(conflict_pr_id)
            .bind(file)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(live_files)
}

pub async fn continue_promotion_run(
    pool: &SqlitePool,
    run_id: i64,
) -> Result<PromotionRunResult, String> {
    let context: Option<(
        i64,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
            SELECT prun.pipeline_id, prun.branch_name, p.target_branch, prun.conflict_phase,
                   r.local_path, prun.original_branch
            FROM promotion_runs prun
            JOIN pipelines p ON p.id = prun.pipeline_id
            JOIN repos r ON r.id = p.repo_id
            WHERE prun.id = ? AND prun.status = 'failed'
            "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let (pipeline_id, branch_name, target_branch, phase, local_path, original_branch) =
        context.ok_or_else(|| "Promotion run not found or already closed".to_string())?;
    if phase.is_none() {
        return Err(
            "This promotion stopped because Git could not apply a change. Abort it, refresh the pipeline, and try again."
                .into(),
        );
    }
    let local_path = local_path.ok_or_else(|| "Repository has no local path".to_string())?;
    let path = Path::new(&local_path);
    git::ensure_git_repo(path)?;

    let expected_branch = if phase.as_deref() == Some("merge") {
        &target_branch
    } else {
        &branch_name
    };
    let current_branch = promote::current_branch(path)?;
    if current_branch != expected_branch.as_str() {
        return Err(format!(
            "Repository is on '{current_branch}'. Switch back to '{expected_branch}' before continuing."
        ));
    }

    let unresolved = reconcile_conflicts(pool, run_id, path).await?;
    if !unresolved.is_empty() {
        let row = active_run_row(pool, run_id).await?;
        return build_run_result(pool, &row).await;
    }

    if phase.as_deref() == Some("cherry_pick") {
        if promote::cherry_pick_in_progress(path)? {
            promote::cherry_pick_continue(path)?;
        } else if !manually_completed_cherry_pick(pool, run_id, path).await? {
                return Err(
                    "Git is no longer in the expected cherry-pick. Return the repository to the promotion branch’s conflict state, or abort this run."
                        .into(),
                );
        }

        sqlx::query(
            r#"
            UPDATE promotions
            SET status = 'promoting', error_message = NULL
            WHERE pipeline_id = ? AND status = 'conflict'
              AND pr_id IN (SELECT pr_id FROM promotion_run_prs WHERE run_id = ?)
            "#,
        )
        .bind(pipeline_id)
        .bind(run_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        let queued = sqlx::query_as::<_, PromotePr>(
            r#"
            SELECT pr.id AS pr_id, pr.title, pr.merge_commit_sha, pr.merge_strategy
            FROM promotion_run_prs prp
            JOIN pull_requests pr ON pr.id = prp.pr_id
            JOIN promotions pm ON pm.pipeline_id = ? AND pm.pr_id = pr.id
            WHERE prp.run_id = ? AND pm.status = 'selected'
            ORDER BY pr.merged_at ASC, pr.id ASC
            "#,
        )
        .bind(pipeline_id)
        .bind(run_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        for pr in queued {
            sqlx::query(
                "UPDATE promotions SET status = 'promoting' WHERE pipeline_id = ? AND pr_id = ?",
            )
            .bind(pipeline_id)
            .bind(pr.pr_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

            let outcome = promote::cherry_pick(
                path,
                &pr.merge_commit_sha,
                pr.merge_strategy.as_deref() == Some("merge"),
            );
            if !outcome.success {
                persist_conflict(
                    pool,
                    run_id,
                    pipeline_id,
                    pr.pr_id,
                    "cherry_pick",
                    &outcome.conflict_files,
                    &outcome.stderr,
                )
                .await?;
                reconcile_conflicts(pool, run_id, path).await?;
                return build_run_result(pool, &active_run_row(pool, run_id).await?).await;
            }
        }

        promote::switch_branch(path, &target_branch)?;
        let merge = promote::merge_branch(path, &branch_name);
        if !merge.success {
            let conflict_pr_id: i64 = sqlx::query_scalar(
                "SELECT pr_id FROM promotion_run_prs WHERE run_id = ? ORDER BY rowid DESC LIMIT 1",
            )
            .bind(run_id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
            persist_conflict(
                pool,
                run_id,
                pipeline_id,
                conflict_pr_id,
                "merge",
                &merge.conflict_files,
                &merge.stderr,
            )
            .await?;
            reconcile_conflicts(pool, run_id, path).await?;
            return build_run_result(pool, &active_run_row(pool, run_id).await?).await;
        }
    } else if phase.as_deref() == Some("merge") {
        if promote::merge_in_progress(path)? {
            promote::merge_continue(path)?;
        } else if !promote::is_ancestor(path, &branch_name, "HEAD")? {
            return Err(
                "Git is no longer in the expected merge. Restore the target branch’s merge state, or abort this run."
                    .into(),
            );
        }
    }

    finalize_run(
        pool,
        run_id,
        pipeline_id,
        path,
        &branch_name,
        original_branch.as_deref(),
    )
    .await?;

    Ok(PromotionRunResult {
        run_id,
        branch_name,
        target_branch,
        conflict_phase: None,
        status: "merged".into(),
        items: build_run_result(pool, &active_run_row_any(pool, run_id).await?)
            .await?
            .items,
        preferred_editor: preferred_editor_name(pool).await.ok(),
        can_continue: false,
        recoverable: false,
    })
}

async fn active_run_row(pool: &SqlitePool, run_id: i64) -> Result<ActiveRunRow, String> {
    active_run_row_with_status(pool, run_id, true).await
}

async fn active_run_row_any(pool: &SqlitePool, run_id: i64) -> Result<ActiveRunRow, String> {
    active_run_row_with_status(pool, run_id, false).await
}

async fn active_run_row_with_status(
    pool: &SqlitePool,
    run_id: i64,
    failed_only: bool,
) -> Result<ActiveRunRow, String> {
    let status_clause = if failed_only {
        " AND prun.status = 'failed'"
    } else {
        ""
    };
    sqlx::query_as::<_, ActiveRunRow>(&format!(
        r#"
        SELECT prun.id AS run_id, prun.branch_name, p.target_branch,
               prun.conflict_phase, prun.status
        FROM promotion_runs prun
        JOIN pipelines p ON p.id = prun.pipeline_id
        WHERE prun.id = ?{status_clause}
        "#
    ))
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Promotion run not found".to_string())
}

async fn persist_conflict(
    pool: &SqlitePool,
    run_id: i64,
    pipeline_id: i64,
    pr_id: i64,
    phase: &str,
    files: &[String],
    error: &str,
) -> Result<(), String> {
    let message = if error.trim().is_empty() {
        if files.is_empty() {
            format!("Git {phase} failed before applying the change")
        } else {
            format!("Git {phase} stopped on a conflict")
        }
    } else {
        error.trim().to_string()
    };
    sqlx::query(
        "UPDATE promotion_runs SET status = 'failed', conflict_phase = ?, completed_at = NULL WHERE id = ?",
    )
    .bind((!files.is_empty()).then_some(phase))
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE promotions SET status = 'conflict', error_message = ? WHERE pipeline_id = ? AND pr_id = ?",
    )
    .bind(message)
    .bind(pipeline_id)
    .bind(pr_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    for file in files {
        sqlx::query(
            "INSERT INTO conflicts (run_id, pr_id, file_path, status) VALUES (?, ?, ?, 'open')",
        )
        .bind(run_id)
        .bind(pr_id)
        .bind(file)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn finalize_run(
    pool: &SqlitePool,
    run_id: i64,
    pipeline_id: i64,
    path: &Path,
    branch_name: &str,
    original_branch: Option<&str>,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let target_head = promote::head_sha(path)?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        r#"
        UPDATE promotions
        SET status = 'promoted', promoted_commit_sha = ?, promoted_at = ?, error_message = NULL
        WHERE pipeline_id = ? AND pr_id IN (
            SELECT pr_id FROM promotion_run_prs WHERE run_id = ?
        )
        "#,
    )
    .bind(&target_head)
    .bind(now)
    .bind(pipeline_id)
    .bind(run_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE conflicts SET status = 'resolved', resolved_at = ? WHERE run_id = ? AND status = 'open'",
    )
    .bind(now)
    .bind(run_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE promotion_runs SET status = 'merged', conflict_phase = NULL, completed_at = ? WHERE id = ?",
    )
    .bind(now)
    .bind(run_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
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
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;

    let _ = promote::delete_branch(path, branch_name);
    if let Some(original) = original_branch {
        if original != promote::current_branch(path)? {
            let _ = promote::switch_branch(path, original);
        }
    }
    Ok(())
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
    let relative_path = Path::new(file_path);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Conflict file path is outside the repository".into());
    }
    let editor_name = preferred_editor_name(pool).await?;
    let (editor_command, detected_path) = preferred_editor_launch(pool).await?;
    let full_path = repo_path.join(relative_path);

    crate::editors::open_file_with_path(&editor_command, detected_path.as_deref(), &full_path)?;

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
    let (editor_command, detected_path) = preferred_editor_launch(pool).await?;
    crate::editors::open_repo_with_path(
        &editor_command,
        detected_path.as_deref(),
        Path::new(&local_path),
    )
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
            if row.conflict_phase.is_some() {
                "conflict"
            } else {
                "failed"
            }
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
        can_continue: row.status == "failed" && conflict_rows.is_empty(),
        recoverable: row.conflict_phase.is_some(),
    })
}

pub async fn abort_promotion_run(pool: &SqlitePool, run_id: i64) -> Result<(), String> {
    let row: Option<(i64, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT prun.pipeline_id, prun.branch_name, p.target_branch, r.local_path,
               prun.original_branch
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

    let (pipeline_id, branch_name, target_branch, local_path, original_branch) =
        row.ok_or_else(|| "Promotion run not found".to_string())?;
    let local_path = local_path.ok_or_else(|| "Repository has no local path".to_string())?;
    let path = Path::new(&local_path);

    let _ = promote::cherry_pick_abort(path);
    let _ = promote::merge_abort(path);
    let restore_branch = original_branch.as_deref().unwrap_or(&target_branch);
    let _ = promote::switch_branch(path, restore_branch);
    let _ = promote::delete_branch(path, &branch_name);

    let pr_ids: Vec<i64> =
        sqlx::query_scalar("SELECT pr_id FROM promotion_run_prs WHERE run_id = ?")
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

    sqlx::query("UPDATE promotion_runs SET status = 'closed', completed_at = ? WHERE id = ?")
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

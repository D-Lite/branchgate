use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};

use crate::editors;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub notify_on_conflict: bool,
    pub notify_on_complete: bool,
    pub default_merge_strategy: String,
    pub managed_clones_root: Option<String>,
    pub share_anonymous_usage: bool,
    pub analytics_consent_decided: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EditorRow {
    pub id: i64,
    pub name: String,
    pub command: String,
    pub detected_path: Option<String>,
    pub is_preferred: bool,
    pub last_verified_at: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsInfo {
    pub db_path: String,
    pub repo_count: i64,
    pub pipeline_count: i64,
    pub editor_count: i64,
    pub github_connected: bool,
    pub mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppSettingsRequest {
    pub notify_on_conflict: Option<bool>,
    pub notify_on_complete: Option<bool>,
    pub default_merge_strategy: Option<String>,
    pub managed_clones_root: Option<String>,
    pub share_anonymous_usage: Option<bool>,
    pub analytics_consent_decided: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRepoSettingsRequest {
    pub repo_id: i64,
    pub working_copy_mode: Option<String>,
    pub default_merge_strategy: Option<String>,
}

const KEY_NOTIFY_CONFLICT: &str = "notify_on_conflict";
const KEY_NOTIFY_COMPLETE: &str = "notify_on_complete";
const KEY_DEFAULT_MERGE: &str = "default_merge_strategy";
const KEY_CLONES_ROOT: &str = "managed_clones_root";
const KEY_SHARE_USAGE: &str = "share_anonymous_usage";
const KEY_CONSENT_DECIDED: &str = "analytics_consent_decided";

pub async fn get_settings(pool: &SqlitePool) -> Result<AppSettings, String> {
    Ok(AppSettings {
        notify_on_conflict: get_bool(pool, KEY_NOTIFY_CONFLICT, true).await?,
        notify_on_complete: get_bool(pool, KEY_NOTIFY_COMPLETE, true).await?,
        default_merge_strategy: get_string(pool, KEY_DEFAULT_MERGE, "auto").await?,
        managed_clones_root: get_optional_string(pool, KEY_CLONES_ROOT).await?,
        share_anonymous_usage: get_bool(pool, KEY_SHARE_USAGE, true).await?,
        analytics_consent_decided: get_bool(pool, KEY_CONSENT_DECIDED, true).await?,
    })
}

pub async fn update_settings(
    pool: &SqlitePool,
    request: UpdateAppSettingsRequest,
) -> Result<AppSettings, String> {
    if let Some(value) = request.notify_on_conflict {
        set_bool(pool, KEY_NOTIFY_CONFLICT, value).await?;
    }
    if let Some(value) = request.notify_on_complete {
        set_bool(pool, KEY_NOTIFY_COMPLETE, value).await?;
    }
    if let Some(ref value) = request.default_merge_strategy {
        validate_merge_strategy(value)?;
        set_string(pool, KEY_DEFAULT_MERGE, value).await?;
    }
    if let Some(ref value) = request.managed_clones_root {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            delete_setting(pool, KEY_CLONES_ROOT).await?;
        } else {
            set_string(pool, KEY_CLONES_ROOT, trimmed).await?;
        }
    }
    if let Some(value) = request.share_anonymous_usage {
        set_bool(pool, KEY_SHARE_USAGE, value).await?;
    }
    if let Some(value) = request.analytics_consent_decided {
        set_bool(pool, KEY_CONSENT_DECIDED, value).await?;
    }
    get_settings(pool).await
}

pub async fn list_editors(pool: &SqlitePool) -> Result<Vec<EditorRow>, String> {
    let rows = sqlx::query_as::<_, EditorDbRow>(
        r#"
        SELECT id, name, command, detected_path, is_preferred, last_verified_at
        FROM editors
        ORDER BY is_preferred DESC, name ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows.into_iter().map(EditorRow::from).collect())
}

pub async fn detect_editors(pool: &SqlitePool) -> Result<Vec<EditorRow>, String> {
    let detected = editors::detect();
    let now = chrono::Utc::now().timestamp();

    let preferred_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM editors WHERE is_preferred = 1 LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

    for editor in detected {
        sqlx::query(
            r#"
            INSERT INTO editors (name, command, detected_path, is_preferred, last_verified_at)
            VALUES (?, ?, ?, 0, ?)
            ON CONFLICT(name) DO UPDATE SET
                command = excluded.command,
                detected_path = excluded.detected_path,
                last_verified_at = excluded.last_verified_at
            "#,
        )
        .bind(&editor.name)
        .bind(&editor.command)
        .bind(&editor.detected_path)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    if let Some(name) = preferred_name {
        let still_exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM editors WHERE name = ?")
            .bind(&name)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
        if !still_exists {
            sqlx::query("UPDATE editors SET is_preferred = 0")
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    list_editors(pool).await
}

pub async fn set_preferred_editor(pool: &SqlitePool, editor_id: i64) -> Result<Vec<EditorRow>, String> {
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM editors WHERE id = ?")
        .bind(editor_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    if !exists {
        return Err("Editor not found".into());
    }

    sqlx::query("UPDATE editors SET is_preferred = 0")
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query("UPDATE editors SET is_preferred = 1 WHERE id = ?")
        .bind(editor_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    list_editors(pool).await
}

pub async fn update_repo_settings(
    pool: &SqlitePool,
    request: UpdateRepoSettingsRequest,
) -> Result<(), String> {
    if let Some(ref mode) = request.working_copy_mode {
        if mode != "existing_local" && mode != "managed" {
            return Err("Invalid working copy mode".into());
        }
        if mode == "managed" {
            return Err("Managed clones are not available yet — use an existing local repo".into());
        }
        sqlx::query("UPDATE repos SET working_copy_mode = ? WHERE id = ?")
            .bind(mode)
            .bind(request.repo_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    if let Some(ref strategy) = request.default_merge_strategy {
        validate_merge_strategy(strategy)?;
        sqlx::query("UPDATE repos SET default_merge_strategy = ? WHERE id = ?")
            .bind(strategy)
            .bind(request.repo_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub async fn disconnect_repo(pool: &SqlitePool, repo_id: i64) -> Result<(), String> {
    let deleted = sqlx::query("DELETE FROM repos WHERE id = ?")
        .bind(repo_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();

    if deleted == 0 {
        return Err("Repository not found".into());
    }
    Ok(())
}

pub async fn clear_sync_cache(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE branch_sync_state
        SET source_head_sha = NULL, target_head_sha = NULL, last_synced_at = NULL
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn reset_app_data(pool: &SqlitePool) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM conflicts").execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM promotion_run_prs").execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM promotion_runs").execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM promotions").execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM pr_commits").execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM pull_requests").execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM branch_sync_state").execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM pipelines").execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM repos").execute(&mut *tx).await.map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_diagnostics(app: &AppHandle, pool: &SqlitePool) -> Result<DiagnosticsInfo, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("branchgate.db");

    let repo_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repos")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let pipeline_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pipelines WHERE active = 1")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let editor_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM editors")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(DiagnosticsInfo {
        db_path: db_path.display().to_string(),
        repo_count,
        pipeline_count,
        editor_count,
        github_connected: false,
        mode: "local".into(),
    })
}

async fn get_bool(pool: &SqlitePool, key: &str, default: bool) -> Result<bool, String> {
    let value = get_optional_string(pool, key).await?;
    Ok(match value.as_deref() {
        Some("1") | Some("true") => true,
        Some("0") | Some("false") => false,
        None => default,
        Some(other) => other != "0",
    })
}

async fn get_string(pool: &SqlitePool, key: &str, default: &str) -> Result<String, String> {
    Ok(get_optional_string(pool, key)
        .await?
        .unwrap_or_else(|| default.to_string()))
}

async fn get_optional_string(pool: &SqlitePool, key: &str) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

async fn set_bool(pool: &SqlitePool, key: &str, value: bool) -> Result<(), String> {
    set_string(pool, key, if value { "1" } else { "0" }).await
}

async fn set_string(pool: &SqlitePool, key: &str, value: &str) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value) VALUES (?, ?)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn delete_setting(pool: &SqlitePool, key: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_merge_strategy(value: &str) -> Result<(), String> {
    if matches!(value, "auto" | "merge" | "squash" | "rebase") {
        Ok(())
    } else {
        Err("Merge strategy must be auto, merge, squash, or rebase".into())
    }
}

#[derive(Debug, sqlx::FromRow)]
struct EditorDbRow {
    id: i64,
    name: String,
    command: String,
    detected_path: Option<String>,
    is_preferred: i64,
    last_verified_at: Option<i64>,
}

impl From<EditorDbRow> for EditorRow {
    fn from(row: EditorDbRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            command: row.command,
            detected_path: row.detected_path,
            is_preferred: row.is_preferred != 0,
            last_verified_at: row.last_verified_at,
        }
    }
}

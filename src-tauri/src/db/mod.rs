use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::error::AppResult;

static DB: tokio::sync::OnceCell<SqlitePool> = tokio::sync::OnceCell::const_new();

pub async fn init(app: &AppHandle) -> AppResult<()> {
    let db_path = db_path(app)?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            crate::error::AppError::Message(format!("failed to create app data dir: {e}"))
        })?;
    }

    let url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let repo_backends = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT local_path, git_backend, wsl_distro FROM repos WHERE local_path IS NOT NULL",
    )
    .fetch_all(&pool)
    .await?;
    for (path, backend, distro) in repo_backends {
        crate::git::runner::configure_repo_backend(&path, &backend, distro.as_deref());
    }

    let _ = crate::settings::detect_editors(&pool).await;

    DB.set(pool)
        .map_err(|_| crate::error::AppError::Message("database already initialized".into()))?;

    Ok(())
}

pub fn pool() -> AppResult<&'static SqlitePool> {
    DB.get()
        .ok_or_else(|| crate::error::AppError::Message("database not initialized".into()))
}

fn db_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Message(e.to_string()))?;
    Ok(dir.join("branchgate.db"))
}

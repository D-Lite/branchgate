use tauri::{AppHandle, Emitter};

#[tauri::command]
pub async fn sync_pipeline(
    app: AppHandle,
    pipeline_id: i64,
) -> Result<crate::sync::SyncSummary, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::sync::sync_pipeline_with_progress(pool, pipeline_id, |progress| {
        let _ = app.emit("pipeline-sync-progress", &progress);
    })
    .await
}

#[tauri::command]
pub async fn get_pipeline_checklist(
    pipeline_id: i64,
) -> Result<Vec<crate::sync::ChecklistItem>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::sync::list_checklist(pool, pipeline_id).await
}

#[tauri::command]
pub async fn get_pipeline_sync_status(
    pipeline_id: i64,
) -> Result<crate::sync::PipelineSyncStatus, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::sync::get_pipeline_sync_status(pool, pipeline_id).await
}

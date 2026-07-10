#[tauri::command]
pub async fn sync_pipeline(pipeline_id: i64) -> Result<crate::sync::SyncSummary, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::sync::sync_pipeline(pool, pipeline_id).await
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

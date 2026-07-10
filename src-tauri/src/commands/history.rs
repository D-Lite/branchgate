#[tauri::command]
pub async fn list_promotion_history(
) -> Result<Vec<crate::history::HistoryRun>, String> {
    let pool = crate::db::pool().map_err(|e| e.to_string())?;
    crate::history::list_runs(pool).await
}
